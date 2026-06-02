# Per-PR staging environment

An ephemeral copy of the backend, on the **same Hetzner box** as prod but
**fully data-isolated**. Opt-in per PR via the `staging` label; torn down when
the label is removed or the PR closes.

## How it works

```
                Caddy (pointe-caddy, :443 — shared)
               /                                   \
  go.pointe.dev → backend:latest      staging.pointe.dev → pointe-backend-staging:sha-<commit>
        │                                              │
        └──────────► Postgres (pointe-postgres) ◄──────┘
             DB: pointe                       DB: pointe_staging   ← isolated
```

Only the **stateless backend** is duplicated. The heavy infra (Postgres server,
Caddy, n8n) is shared, and staging gets its **own database** + **own secrets**.

`.github/workflows/deploy-staging.yml`:
- **Deploy** (PR labelled `staging`, or a new push while labelled): builds & pushes
  the PR commit image, brings up `docker compose -p pointe-staging`, then runs
  `scripts/smoke-qualifier.sh` against the staging container over the docker network.
- **Teardown** (label removed, or PR closed): `compose down`. The empty
  `pointe_staging` database is kept (isolated and ~free).

## Data isolation (vs prod)

| Concern | Staging |
|---|---|
| Postgres | same server, **separate database** `pointe_staging` |
| n8n | **disabled** — no `N8N_API_KEY`, never triggers a prod automation |
| Stripe | **TEST** keys |
| Email (Resend) | **unset** — confirm links logged, no real mail leaves the box |
| `SESSION_SECRET` | **distinct** — tokens are invalid across envs |
| Langfuse | **unset** — no pollution of prod traces |
| Volumes | none from prod (logs to stdout) |

## Perf cost on the box (2 vCPU / 4 GB)

| State | RAM | CPU |
|---|---|---|
| Staging **down** (default) | 0 | 0 |
| Staging **up**, idle | ~50–90 MB (one Rust binary) | ~0 % |
| Staging during a smoke | + brief spikes | modest (pipeline is I/O-bound on the LLM API) |

Sharing the Postgres *server* (separate DB) and Caddy keeps the marginal cost to
a single backend container. Don't duplicate the full stack (2nd Postgres + n8n +
qdrant ≈ +500–800 MB) — that's what would strain the 4 GB box.

## One-time server setup

Required once before the first labelled deploy.

1. **DNS** — A record `staging.pointe.dev` → `167.235.20.209`.

2. **Caddy vhost** — this repo's `Caddyfile` already has the `staging.pointe.dev`
   block. Sync it to the server and reload (the server `/opt/pointe` is not a git
   checkout, so copy the file):
   ```bash
   scp Caddyfile pointe-server:/opt/pointe/Caddyfile
   ssh pointe-server 'docker exec pointe-caddy caddy reload --config /etc/caddy/Caddyfile'
   ```

3. **Staging env file** — on the server, from the template:
   ```bash
   ssh pointe-server
   cd /opt/pointe
   cp /path/to/repo/.env.staging.example .env.staging   # or scp it over
   # fill in: ANTHROPIC_API_KEY, DATABASE_URL (pg password from secrets/pg_password.txt),
   #          SESSION_SECRET (openssl rand -hex 32), Stripe TEST keys
   ```

   The `pointe_staging` database is created automatically by the deploy workflow
   (idempotent), so no manual `CREATE DATABASE` is needed.

4. **GitHub** — create a `staging` label on the repo. The workflow reuses the
   existing deploy secrets (`HETZNER_HOST`, `HETZNER_USER`, `HETZNER_SSH_KEY`,
   `GHCR_TOKEN`); no new secrets required.

## Usage

- **Deploy a PR**: add the `staging` label. Watch the *Staging (per-PR)* workflow;
  it comments the URL when the smoke passes. New commits redeploy automatically.
- **Tear down**: remove the label, or close/merge the PR.

## Notes

- A labelled PR is built twice (the gating CI build + this staging build), both
  gha-cached so the second is mostly a cache hit. The build runs on Actions, never
  on the Hetzner box.
- While staging is down, `https://staging.pointe.dev` returns 502 — expected; prod
  hosts are unaffected.
