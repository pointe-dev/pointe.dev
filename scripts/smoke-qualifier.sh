#!/usr/bin/env bash
# End-to-end smoke test for the qualifier funnel.
#
# Drives a real multi-turn conversation through /api/ai/chat exactly as the
# browser does, then verifies the whole chain is alive:
#   1. the bot replies,
#   2. selectable option chips appear at least once    (soft — model's choice),
#   3. the qualify block gates on email (needs_unlock)  (hard assert),
#   4. confirming the email spawns a pipeline           (hard assert*),
#   5. the agent chain publishes a pitch                (hard assert*).
#
#   * Steps 4-5 require SESSION_SECRET so the script can forge the double
#     opt-in confirm token (the same HMAC the backend signs). Without it the
#     run stops after step 3 with a pass on the gate and a warning — the
#     pipeline is correctly gated, we just can't drive past the email wall.
#
# Makes real Anthropic calls (costs money, ~1-3 min) — run on demand or
# post-deploy, NOT in the CI gate.
#
# Usage:
#   Local dev (backend on :3001):
#     cargo run -p backend &
#     SESSION_SECRET=<dev-secret> bash scripts/smoke-qualifier.sh
#
#   Prod server (backend not published to host — go over the docker network;
#   pull SESSION_SECRET from the running env so the confirm token matches):
#     export $(grep -E '^SESSION_SECRET=' /opt/pointe/.env.prod | sed 's/\r//')
#     DOCKER_NET=pointe_pointe-net BASE_URL=http://backend:3001 \
#       bash scripts/smoke-qualifier.sh
#
# Env:
#   BASE_URL        backend base url           (default http://localhost:3001)
#   DOCKER_NET      run curl in a throwaway container on this docker network
#   PITCH_TIMEOUT   seconds to wait for a pitch (default 360)
#   SESSION_SECRET  HMAC secret to forge the confirm token (enables steps 4-5)
#
# Exit code: 0 = all reachable hard checks passed, 1 = a hard check failed.
set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:3001}"
PITCH_TIMEOUT="${PITCH_TIMEOUT:-360}"
SID="smoke-$(date +%s)"
EMAIL="smoke-$(date +%s)@example.com"
HIST='[]'
options_seen=0
pipeline_id=""
needs_unlock=0

command -v jq >/dev/null || { echo "✗ jq is required"; exit 1; }

# curl indirection: direct, or inside a container on a docker network.
do_curl() {
  if [ -n "${DOCKER_NET:-}" ]; then
    docker run --rm -i --network "$DOCKER_NET" curlimages/curl:latest "$@"
  else
    curl "$@"
  fi
}

# Packed so the model collects sector + pain + tools + volume in a few turns.
MESSAGES=(
  "Bonjour !"
  "Je gère une boutique e-commerce de cosmétiques bio sur Shopify. Mon souci : je recopie chaque commande à la main dans mon logiciel de compta Sage, c'est interminable."
  "Environ 80 commandes par jour. Les outils : Shopify pour la boutique, Sage pour la compta, et Gmail pour les confirmations."
  "Oui exactement, j'aimerais que tout se synchronise automatiquement."
)

chat() {
  local msg="$1" payload
  payload=$(jq -n --arg d "$msg" --argjson h "$HIST" --arg s "$SID" \
    '{description:$d, history:$h, session_id:$s}')
  do_curl -s -XPOST "$BASE_URL/api/ai/chat" \
    -H 'content-type: application/json' -d @- <<<"$payload"
}

echo "▶ Driving qualifier — session=$SID base=$BASE_URL"
turn=0
for msg in "${MESSAGES[@]}"; do
  turn=$((turn + 1))
  echo "──────────────────────────────────────────"
  echo "👤 turn $turn: $msg"
  resp=$(chat "$msg")

  text=$(jq -r '.response // ""' <<<"$resp")
  if [ -z "$text" ] && ! jq -e '.response' >/dev/null 2>&1 <<<"$resp"; then
    echo "✗ no valid chat response: $resp"; exit 1
  fi
  opts=$(jq -c '.options // []' <<<"$resp")
  pid=$(jq -r '.pipeline_id // empty' <<<"$resp")
  gate=$(jq -r '.needs_unlock // false' <<<"$resp")

  echo "🤖 ${text:0:240}"
  [ "$opts" != "[]" ] && { echo "🔘 options: $opts"; options_seen=1; }

  HIST=$(jq -c --argjson h "$HIST" --arg u "$msg" --arg a "$text" \
    '$h + [{role:"user",content:$u},{role:"assistant",content:$a}]' <<<'null')

  # Already-unlocked sessions spawn immediately; anonymous ones gate on email.
  if [ -n "$pid" ]; then pipeline_id="$pid"; echo "🚀 pipeline spawned: $pid"; break; fi
  if [ "$gate" = "true" ]; then needs_unlock=1; echo "🔒 qualified — pipeline gated behind email"; break; fi
done

# ── Hard assert 1: the funnel qualified (spawned now, or gated on email) ───
if [ -z "$pipeline_id" ] && [ "$needs_unlock" != "1" ]; then
  echo "✗ FAIL: conversation never qualified (no pipeline, no email gate)"; exit 1
fi

# ── Drive the double opt-in to spawn the gated pipeline ────────────────────
# The confirm token is HMAC-SHA256(SESSION_SECRET, "<email>|<sid>"), matching
# sessions::sign_confirm_token. We forge it here to click the link the user
# would normally click in their inbox.
if [ "$needs_unlock" = "1" ]; then
  if [ -z "${SESSION_SECRET:-}" ]; then
    echo "⚠️  SESSION_SECRET not set — can't forge the confirm token."
    echo "✅ GATE VERIFIED: pipeline correctly blocked until email confirmation."
    echo "   (set SESSION_SECRET to drive steps 4-5 — pitch publication)"
    exit 0
  fi
  command -v openssl >/dev/null || { echo "✗ openssl is required to forge the confirm token"; exit 1; }

  echo "──────────────────────────────────────────"
  echo "✉️  requesting unlock for $EMAIL"
  do_curl -s -XPOST "$BASE_URL/api/auth/unlock" \
    -H 'content-type: application/json' \
    -d "$(jq -n --arg s "$SID" --arg e "$EMAIL" '{session_id:$s, email:$e}')" >/dev/null

  token=$(printf '%s' "${EMAIL}|${SID}" \
    | openssl dgst -sha256 -hmac "$SESSION_SECRET" -r | awk '{print $1}')
  echo "🔗 confirming email (token ${token:0:12}…)"
  do_curl -s -G "$BASE_URL/api/auth/confirm" \
    --data-urlencode "e=$EMAIL" --data-urlencode "s=$SID" --data-urlencode "t=$token" \
    -o /dev/null

  # The pipeline is spawned on confirm; its id surfaces via /api/auth/status.
  for _ in $(seq 1 10); do
    st=$(do_curl -s "$BASE_URL/api/auth/status?sid=$SID" || echo '{}')
    pipeline_id=$(jq -r '.pipeline_id // empty' <<<"$st")
    [ -n "$pipeline_id" ] && break
    sleep 1
  done
  if [ -z "$pipeline_id" ]; then
    echo "✗ FAIL: email confirmed but no pipeline spawned (auth/status had no pipeline_id)"; exit 1
  fi
  echo "🚀 pipeline spawned after unlock: $pipeline_id"
fi

# ── Hard assert 2: a pitch gets published before the timeout ──────────────
echo "──────────────────────────────────────────"
echo "⏳ polling /api/pitch/result (timeout ${PITCH_TIMEOUT}s)…"
deadline=$(( $(date +%s) + PITCH_TIMEOUT ))
while [ "$(date +%s)" -lt "$deadline" ]; do
  pr=$(do_curl -s "$BASE_URL/api/pitch/result?pid=$pipeline_id" || echo '{}')
  if [ "$(jq -r '.ready // false' <<<"$pr")" = "true" ]; then
    manual=$(jq -r '.manual_quote // false' <<<"$pr")
    cents=$(jq -r '.price_eur_cents // 0' <<<"$pr")
    slides=$(jq -r '.slides | length' <<<"$pr" 2>/dev/null || echo "?")
    echo "✅ pitch published — manual_quote=$manual price_cents=$cents slides=$slides"
    [ "$options_seen" = "1" ] \
      && echo "✅ option chips appeared during the conversation" \
      || echo "⚠️  no option chips this run (model's discretion — not a failure)"
    echo "✅ SMOKE PASSED"
    exit 0
  fi
  sleep 5
done

echo "✗ FAIL: no pitch within ${PITCH_TIMEOUT}s (pipeline $pipeline_id stuck or failed)"
exit 1
