#!/usr/bin/env bash
# Push prompts to Langfuse.
# Usage: bash scripts/push-prompts.sh
# Reads LANGFUSE_PUBLIC_KEY, LANGFUSE_SECRET_KEY, LANGFUSE_BASE_URL from .env

set -euo pipefail

# Load .env (strips Windows CRLF line endings)
if [[ -f .env ]]; then
  set -a; source <(sed 's/\r//' .env); set +a
fi

: "${LANGFUSE_PUBLIC_KEY:?LANGFUSE_PUBLIC_KEY not set}"
: "${LANGFUSE_SECRET_KEY:?LANGFUSE_SECRET_KEY not set}"
: "${LANGFUSE_BASE_URL:?LANGFUSE_BASE_URL not set}"

push_prompt() {
  local name="$1"
  local prompt="$2"
  local labels="${3:-production}"

  local payload
  payload=$(jq -n \
    --arg name    "$name" \
    --arg prompt  "$prompt" \
    --argjson labels "$(echo "$labels" | jq -R 'split(",")')" \
    '{type:"text", name:$name, prompt:$prompt, labels:$labels}')

  local resp
  resp=$(curl -s -w "\n%{http_code}" \
    -X POST "${LANGFUSE_BASE_URL}/api/public/v2/prompts" \
    -u "${LANGFUSE_PUBLIC_KEY}:${LANGFUSE_SECRET_KEY}" \
    -H "Content-Type: application/json" \
    -d "$payload")

  local body http_code
  body=$(echo "$resp" | head -n -1)
  http_code=$(echo "$resp" | tail -n 1)

  if [[ "$http_code" -ge 200 && "$http_code" -lt 300 ]]; then
    local version
    version=$(echo "$body" | jq -r '.version // "?"')
    echo "✓ ${name} (v${version})"
  else
    echo "✗ ${name}: HTTP ${http_code}" >&2
    echo "$body" | jq . 2>/dev/null || echo "$body" >&2
    exit 1
  fi
}

# ─────────────────────────────────────────────
# Prompts
#
# The chatbot prompt is kept in prompts/qualifier-chatbot-prompt.txt so this
# script and the Rust FALLBACK_PROMPT (include_str! in main.rs) stay in sync.
# Edit the .txt file, never inline a copy here.
# ─────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

push_prompt "qualifier-chatbot-prompt" \
"$(cat "${SCRIPT_DIR}/prompts/qualifier-chatbot-prompt.txt")"
