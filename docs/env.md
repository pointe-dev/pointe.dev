# Variables d'environnement

Référence des variables lues par le backend. Source de vérité = les `env::var(...)`
dans `crates/backend/src` + le mapping OAuth dans `oauth::app_keys_for`. Le chargement
en `.env` se fait via `dotenvy`.

**Secrets à fallback fichier** (`config.rs`) : `SESSION_SECRET` et `ADMIN_INGEST_TOKEN`
se résolvent dans l'ordre **env → fichier (`*_FILE` ou chemin par défaut) → généré**.
Pour `SESSION_SECRET`, s'il est généré au boot il est écrit sur disque pour survivre au
redémarrage — sinon les sessions seraient invalidées à chaque restart.

## Serveur / runtime

| Variable | Rôle |
|----------|------|
| `BIND_ADDR` | Adresse d'écoute du serveur Axum. |
| `BASE_URL` / `APP_URL` | URL publique du site (liens email, redirections). |
| `LOG_DIR` | Dossier des logs (tracing-appender). |
| `DATABASE_URL` | Postgres. **Absent → SessionStore purement en mémoire** (cf. `sessions.rs`). |
| `TEST_DATABASE_URL` | Postgres pour les tests. |

## Secrets de session / admin

| Variable | Rôle |
|----------|------|
| `SESSION_SECRET` / `SESSION_SECRET_FILE` | Clé HMAC de signature des sessions. Fallback fichier → généré+persisté. |
| `ADMIN_INGEST_TOKEN` / `ADMIN_INGEST_TOKEN_FILE` | Bearer token de `/api/admin/ingest`. |
| `OWNER_EMAIL` | Email de notification owner (nouveaux dossiers, etc.). |

## IA / observabilité

| Variable | Rôle |
|----------|------|
| `ANTHROPIC_API_KEY` | Clé Claude (agents IA). |
| `LANGFUSE_BASE_URL` / `LANGFUSE_PUBLIC_KEY` / `LANGFUSE_SECRET_KEY` | Prompts + traces. |

## Vectoriel / embeddings

| Variable | Rôle |
|----------|------|
| `QDRANT_URL` | Qdrant (dev / local). |
| `CF_ACCOUNT_ID` / `CF_API_TOKEN` / `CF_VECTORIZE_INDEX` | Cloudflare Vectorize (prod). |

## Paiement / email

| Variable | Rôle |
|----------|------|
| `STRIPE_SECRET_KEY` | Clé API Stripe. |
| `STRIPE_WEBHOOK_SECRET` | Vérification de signature du webhook Stripe. |
| `RESEND_API_KEY` | Envoi d'emails (double opt-in, notifications). |

## n8n (automatisation)

| Variable | Rôle |
|----------|------|
| `N8N_URL` | Base de l'instance n8n (partagée API publique + login owner). |
| `N8N_API_KEY` | Clé `/api/v1` (création de credentials par clé API). |
| `N8N_OWNER_EMAIL` / `N8N_OWNER_PASSWORD` | Login owner `/rest` pour minter les URL de consentement OAuth. **2FA/SSO doit être OFF.** Voir [oauth.md](oauth.md). |
| `N8N_MCP_URL` / `N8N_MCP_TOKEN` | Endpoint MCP n8n (publish_workflow, etc.). |
| `N8N_TEST_FOLDER` | Dossier n8n pour les workflows de test. |

## Guardrails (anti-abus)

| Variable | Rôle |
|----------|------|
| `CLINGO_PATH` | Chemin du binaire clingo (sinon cherché dans le PATH). |
| `GUARDRAILS_FAIL_CLOSED` | Si vrai, une erreur d'évaluation **bloque** au lieu de laisser passer. Voir [guardrails.md](guardrails.md). |

## OAuth — clés d'app par provider

Lues dynamiquement par `oauth::app_keys_for` sous la forme
`<PREFIX>_OAUTH_CLIENT_ID` / `<PREFIX>_OAUTH_CLIENT_SECRET`. Ce sont **nos** clés d'app
(pas celles des clients). Préfixes : `GOOGLE` (Gmail/Drive/Calendar/Sheets/YouTube),
`MICROSOFT`, `SLACK`, `HUBSPOT`, `SALESFORCE`, `ZOHO`, `MAILCHIMP`, `ASANA`, `SHOPIFY`,
`TWITTER`, `LINKEDIN`. Détail et table complète : [oauth.md](oauth.md).
