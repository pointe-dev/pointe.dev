# Handoff — 2026-06-13 (machine switch) · updated after P1 (auto-activation)

Portable handoff in the repo because the detailed `~/.claude/.../memory/` notes do **not**
travel between machines. Read this first on the new machine.

> **New-machine note:** the `ssh pointe-server` alias does **not** travel either. On this box,
> use `ssh -i ~/.ssh/pointe_deploy root@167.235.20.209`. Also: the **local `.env` ANTHROPIC_API_KEY
> is stale (401)** — for any live test, pull `ANTHROPIC_API_KEY` (and `N8N_MCP_TOKEN`) from the
> server's `/opt/pointe/.env.prod`.

## TL;DR — where things stand

The **build-hardening track is COMPLETE, merged to `main`, and live in prod.**

- **PR #25 merged** (merge commit `c271773` on `main`). PR #24 auto-closed (its commit
  `17e007f` landed). The branch `feat/mcp-builder-grounding` was deleted.
- Shipped end-to-end: qualification/build split · MCP grounding of builder/critic/designer ·
  sub-workflow decomposition (chained sub-flows ≤8 nodes) · **option (a) SDK-code deploy**
  (builder emits n8n Workflow SDK code → `validate_workflow` → `create_workflow_from_code`).
- Proven **live**, green: mono code path (`dogfood_code_pipeline_live`) and decomposed code
  path (`dogfood_decomposed_code_live` — 5 sub-flows, all `validate_workflow valid=true` on
  the first try, all deployed + chained). Test workflows were archived.
- **MCP is ACTIVE in prod.** `N8N_MCP_URL` + `N8N_MCP_TOKEN` are in the server
  `/opt/pointe/.env.prod`; the redeployed backend loaded them (verified).
- **P1 SHIPPED (commit `de5b709` on `main`, pushed):** the code-deploy path now **auto-activates
  via the MCP `publish_workflow`** instead of the CF-blocked REST `n8n_activate`. New
  `N8nMcpConfig::publish_workflow` in `mcp.rs`; `deploy_from_code` publishes **leaf-first**
  (`ordered.iter().rev()`, entry last). Verified live (decomposed dogfood, 4 sub-flows, green).
  See the `~/.claude` memory note **n8n-mcp-credentials-activation** for the full MCP capability
  map + the two publish constraints below.

## ⚠️ Critical prod gotcha — internal MCP URL

On the **server**, `N8N_MCP_URL` must be the **internal** docker alias:

```
N8N_MCP_URL=http://n8n:5678/mcp-server/http
```

**NOT** the public `https://n8n.pointe.dev/mcp-server/http`. When the box hits its own public
domain it gets **Cloudflare's "Just a moment…" JS challenge** → every MCP call fails. And
code-mode is gated only on `n8n_mcp.is_some()`, so a configured-but-unreachable MCP makes
**every post-payment build go code-mode and fail at deploy** (no JSON fallback). The MCP is
served by n8n itself on port 5678 (Caddy proxies `n8n.pointe.dev → n8n:5678`); there is no
separate MCP container. Verified internally: `node fetch` → `http://localhost:5678/mcp-server/http`
with the Bearer token returns **HTTP 200 `tools/list`**.

`N8N_TEST_FOLDER` is intentionally **not** set in prod (folderId is ignored by this MCP build,
and real client workflows should land in the project root). `N8N_URL` stays **public** so the
display URLs are user-accessible. Activation no longer uses that public REST (P1): it goes
through the internal MCP `publish_workflow`.

**Probing the MCP from local works against the PUBLIC url** (external IP isn't CF-challenged —
only the server hitting its own domain is). So to drive the MCP from this machine, set
`N8N_MCP_URL=https://n8n.pointe.dev/mcp-server/http` + the server's token. To probe from the
server, exec inside the n8n container against `http://localhost:5678/mcp-server/http`.

## Running the live dogfood tests (note the corrected grep)

**On THIS machine there is no local `.env.prod`** (machine switch). What worked, run from repo root:

```
set -a
. <(grep -E '^(N8N_URL|N8N_API_KEY)=' .env | sed 's/\r$//')
export N8N_MCP_URL="https://n8n.pointe.dev/mcp-server/http"   # public url is reachable from local
creds=$(ssh -i ~/.ssh/pointe_deploy root@167.235.20.209 'grep -E "^(ANTHROPIC_API_KEY|N8N_MCP_TOKEN)=" /opt/pointe/.env.prod | tr -d "\r"')
export ANTHROPIC_API_KEY="$(echo "$creds" | grep '^ANTHROPIC_API_KEY=' | cut -d= -f2-)"   # local .env key is 401
export N8N_MCP_TOKEN="$(echo "$creds" | grep '^N8N_MCP_TOKEN=' | cut -d= -f2-)"
set +a
cargo test -p backend --test dogfood -- --ignored --nocapture dogfood_decomposed_code_live
```

These make real LLM calls (~cents) and create real n8n workflows — **archive them afterward**
(ids are printed). **Do NOT archive before verifying `active`** — archived workflows become
unreadable (`get_workflow_details` errors), so inspect first, archive last. `cargo test -p
backend --lib` = 106 tests, no external deps. Decomposition count is non-deterministic (3–5 sub-flows).

## What's on THIS branch

This branch = `main` (incl. P1 `de5b709`) merged in **plus** the pre-existing working-tree WIP
below, preserved so the machine switch loses nothing. So checking it out gives you everything;
`main` itself has the shipped code but not this WIP or this handoff.

- `landing/index.html` — the apex landing page. **Already live on Cloudflare Pages**, just never
  tracked in git until now. Separate from the backend Docker build.
- `prompts/qualifier-chatbot-prompt.txt` (+68 lines) — conversational tone/rhythm guidance for
  the qualifier bot (warmer, less robotic, one question per message). **Source only — NOT pushed
  to Langfuse**, so it is not live yet. To activate: push to Langfuse from the server (the backend
  reads the Langfuse prompt in prod; the `.txt` is only the `include_str!` fallback). See the
  memory note on `push-prompts.sh` (must run from the server — local `.env` points at the dead
  self-hosted Langfuse).
- `.gitignore` — ignore `n8n.mcp` (local MCP config holding a secret token; never commit it).

## Next step — credential auto-injection (P2/P3)

Live probing reshaped this. The MCP's own `create_workflow_from_code` **already auto-assigns**
keychain credentials by type for *service* nodes (returns `autoAssignedCredentials`) and **skips
httpRequest** nodes. So the gap is narrower than first thought:

- **P2 — log it.** Parse the `create_from_code` result's `note`/`autoAssignedCredentials` and log
  auto-wired vs. skipped so the owner knows exactly what's left manual.
- **P3 — explicit inject for service nodes not auto-assigned.** After create, `list_credentials(type)`;
  on a unique type match → `update_workflow` op `setNodeCredential {nodeName, credentialKey,
  credentialId, credentialName}`. Default-deny (0 or ≥2 matches → leave manual + log). We never see
  secrets (catalogue is read-only).
- **P4 — httpRequest stays MANUAL (decided).** The MCP rejects credentials on httpRequest nodes:
  `setNodeCredential` → `"node type 'n8n-nodes-base.httpRequest' does not accept credential 'httpHeaderAuth'"`,
  and `create_from_code` strips them. Owner wires header-auth by hand.

**⚠️ Two hard publish constraints (verified live) — why activation isn't a free win:**
1. **Leaf-first.** Publishing a workflow whose Execute Workflow targets aren't published fails
   (`"references workflow X which is not published"`). P1 already publishes `ordered.iter().rev()`.
2. **Complete config required.** n8n refuses to publish a node with a missing required credential
   or param (`"Missing required credential: slackApi; Missing or invalid required parameters…"`).
   A `newCredential('Label')` empty shell does **not** count. **So a fresh deploy whose creds aren't
   wired CANNOT auto-activate** — n8n itself blocks it. P1 is therefore best-effort + warn; full
   auto-activation only lands together with P2/P3 (creds present → publish succeeds).

Optional later: tighten the node budget if sub-flows keep coming out at 9 nodes; an
execute→inspect→fix loop via `test_workflow` (limited here — paid APIs, no dry-run).

## Useful pointers

- Server access: `ssh -i ~/.ssh/pointe_deploy root@167.235.20.209` (no `pointe-server` alias on this
  machine). Prod env: `/opt/pointe/.env.prod` (patch line-by-line, **never** scp the local one over
  it — server values differ). Backend container: `pointe-backend` on docker network
  `pointe_pointe-net` (same as `pointe-n8n`, `pointe-caddy`).
- Prod site: `https://go.pointe.dev` (the Leptos app). Apex `https://pointe.dev` = the landing page.
- CI gates deploy: push to `main` → CI (unit/integration/coverage/docker, pushes `latest`) →
  Build & Deploy (pull + restart) → E2E/Lighthouse post-deploy. The post-deploy E2E shows a known
  Cloudflare-403 false-red (`continue-on-error`, gates nothing). **No path filters** — even a
  docs-only push to `main` triggers a full rebuild+deploy, so this handoff lives on a `wip/` branch.

## Side track — infra/DevOps learning (owner's goal)

Separate from the product: the owner wants to learn pro infra **deeply and cheaply**, using this
project as the lab. Full reasoning in the `~/.claude` memory **infra-learning-roadmap**. Short version:
- **Don't learn beside the project — learn on it.** It already runs a real prod (Hetzner VPS +
  Docker Compose: backend/n8n/postgres/caddy/qdrant + Caddy/LE + GitHub Actions + Cloudflare).
- **Cost:** AWS isn't free long-term (EKS ≈ $73/mo control plane) → learn k8s via **self-hosted k3s
  on Hetzner** (~5 €/mo), local is free (kind/minikube/LocalStack). Recommended order:
  **Terraform/OpenTofu** (codify the existing Hetzner+Cloudflare) → **Prometheus+Grafana** →
  **k3s** (migrate the Compose stack) → **ArgoCD/GitOps**. Keep GitHub Actions, **skip Jenkins** (legacy).
- **Fork to confirm:** employability (→ AWS + cert + k8s priority) vs. self-mastery (→ stay Hetzner/k3s).
- Offered-but-not-done: sketch a Terraform module for the current Hetzner server + Cloudflare DNS as
  the first hands-on exercise.
