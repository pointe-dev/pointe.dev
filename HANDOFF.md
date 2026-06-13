# Handoff — 2026-06-13 (machine switch)

Portable handoff in the repo because the detailed `~/.claude/.../memory/` notes do **not**
travel between machines. Read this first on the new machine.

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
display URLs are user-accessible — the trade-off is that the best-effort REST auto-activation
gets CF-challenged, so deployed workflows are **activated manually** by the owner (which already
fits the manual credential-wiring step).

## Running the live dogfood tests (note the corrected grep)

The old docstring grep `^(…|CF_|N8N_MCP_)=` silently dropped `CF_`/`N8N_MCP_` (the `=` can't
follow a prefix). Use:

```
set -a; . <(grep -E '^(ANTHROPIC_API_KEY|CF_|N8N_)' .env.prod | sed 's/\r$//'); set +a
cargo test -p backend --test dogfood -- --ignored --nocapture dogfood_decomposed_code_live
```

These make real LLM calls and create real n8n workflows — **archive them afterward** (ids are
printed). `cargo test -p backend --lib` = 106 tests, no external deps.

## What's on THIS branch (not on main)

Pre-existing working-tree WIP, preserved so the machine switch loses nothing:

- `landing/index.html` — the apex landing page. **Already live on Cloudflare Pages**, just never
  tracked in git until now. Separate from the backend Docker build.
- `prompts/qualifier-chatbot-prompt.txt` (+68 lines) — conversational tone/rhythm guidance for
  the qualifier bot (warmer, less robotic, one question per message). **Source only — NOT pushed
  to Langfuse**, so it is not live yet. To activate: push to Langfuse from the server (the backend
  reads the Langfuse prompt in prod; the `.txt` is only the `include_str!` fallback). See the
  memory note on `push-prompts.sh` (must run from the server — local `.env` points at the dead
  self-hosted Langfuse).
- `.gitignore` — ignore `n8n.mcp` (local MCP config holding a secret token; never commit it).

## Next step (the only one left)

**Credential auto-injection at deploy.** Today the builder emits `credentials: {}`; the API keys
(Google / Apify / ElevenLabs / Creatomate / YouTube / Instagram…) are added to the n8n keychain
and attached per node + the workflow activated **manually** after deploy. Improvement: at deploy,
use the MCP `list_credentials` to match and attach existing credentials by node, then activate.
I never see the secrets (the MCP catalogue is read-only).

Optional later: tighten the node budget if sub-flows keep coming out at 9 nodes; an
execute→inspect→fix loop via `test_workflow` (limited here — paid APIs, no dry-run).

## Useful pointers

- Server access: `ssh pointe-server`. Prod env: `/opt/pointe/.env.prod` (patch line-by-line,
  **never** scp the local one over it — server values differ). Backend container: `pointe-backend`
  on docker network `pointe_pointe-net` (same as `pointe-n8n`, `pointe-caddy`).
- Prod site: `https://go.pointe.dev` (the Leptos app). Apex `https://pointe.dev` = the landing page.
- CI gates deploy: push to `main` → CI (unit/integration/coverage/docker, pushes `latest`) →
  Build & Deploy (pull + restart) → E2E/Lighthouse post-deploy. The post-deploy E2E shows a known
  Cloudflare-403 false-red (`continue-on-error`, gates nothing).
