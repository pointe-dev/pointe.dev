# Architecture

## Workspace

Monorepo Cargo à trois crates (`crates/`) :

| Crate | Rôle | Entrée |
|-------|------|--------|
| `frontend` | UI Leptos compilée en WASM. Pages + composants. | `frontend/src/lib.rs` |
| `backend` | Serveur Axum : API, agents IA, intégrations. | `backend/src/main.rs` (binaire) + `backend/src/lib.rs` (shim pour les tests) |
| `shared` | Types partagés front/back. | `shared/src/lib.rs` |

`backend/src/lib.rs` re-exporte les modules internes pour que les tests d'intégration
les importent sans dupliquer `main()`. Le binaire et la lib déclarent **tous les deux**
la liste des modules — quand tu ajoutes un module, ajoute-le aux **deux** (`main.rs` et
`lib.rs`), sinon la lib ou le binaire ne le verra pas.

## Carte des modules backend

```
main.rs / lib.rs        entrée + routeur Axum + déclaration des modules
state.rs                AppState : http client, SessionStore, db pool, … (injecté dans chaque handler)
config.rs               chargement des secrets (env OU fichier OU généré) — voir env.md

handlers/               les handlers HTTP, un fichier par domaine
  ├── pipeline.rs       /api/pipeline/* — démarrage, statut, resume, delivery
  ├── stripe.rs         /api/stripe/* — checkout + webhook
  ├── credentials.rs    /api/credentials/provision + /api/oauth/start
  ├── client.rs         /api/client/workflows — l'espace client
  ├── admin.rs          /api/admin/* — dossiers, respawn (token-gated)
  ├── ingest.rs         /api/admin/ingest — alimentation de la base de connaissances
  ├── services.rs       /api/services — catalogue exposé
  ├── mcp.rs            /mcp — endpoint MCP
  └── health.rs         /api/health

pipeline.rs             la machine à états du pipeline (PipelineStage) + orchestration
agents.rs               les appels aux agents IA (research, design, build, pricing, critics)
pitch.rs                assemblage du pitch + prix renvoyé au front
capabilities.rs         le CATALOG : quels services on sait livrer + leur mode d'auth
credentials.rs          client REST n8n (/api/v1) : création de credentials par clé API
oauth.rs                connexion OAuth2 déléguée à n8n (login owner /rest → consent URL)
guardrails/             gate anti-abus ASP/clingo avant activation (mod, facts, clingo, policy.lp)
sessions.rs             SessionStore : gating free-tier + rate-limit empreinte (in-memory + DB)
mcp.rs                  client MCP vers n8n (publish_workflow, credentials…)
email.rs                Resend : double opt-in, notifications owner
embeddings.rs           BGE-M3 (fastembed) — vecteurs pour la recherche
qdrant.rs / cloudflare.rs   stockage vectoriel (Qdrant local / CF Vectorize prod)
langfuse.rs             fetch des prompts + envoi des traces
stripe.rs               client Stripe (checkout, vérif webhook)
pending.rs              file des opérations en attente
```

## Parcours d'une requête (chemin nominal)

```
Navigateur (Leptos/WASM)
   │  fetch JSON
   ▼
Axum router (main.rs)  ──► handler (handlers/*.rs)
   │
   ├─ lit/écrit l'état via AppState (state.rs) — sessions, http client, db
   ├─ appelle les agents IA (agents.rs) — prompts via Langfuse
   ├─ parle à n8n (credentials.rs / oauth.rs / mcp.rs)
   ├─ parle à Stripe (stripe.rs) / Resend (email.rs)
   └─ renvoie du JSON
```

`AppState` (`state.rs`) est l'unique conteneur d'état injecté dans chaque handler via
`State<Arc<AppState>>`. Il porte le client `reqwest` partagé (`http`), le `SessionStore`,
le pool Postgres optionnel (`db`), et la config. **N'ouvre pas de nouveau client HTTP
dans un handler** — réutilise `state.http` (sauf cas OAuth qui a besoin d'un jar de
cookies dédié, voir [oauth.md](oauth.md)).

## Frontend

```
frontend/src/lib.rs         montage + routeur
  pages/                    home, chat, merci (post-paiement), espace (dashboard client), admin
  components/               hero, layout, theme(+toggle), workflow_canvas, contact_modal,
                            consent_banner, delivery (la checklist de connexion des comptes)
  i18n.rs                   internationalisation (FR/EN/…) — la doc client passera par là
```

Le CSS est généré : **`input.css` est la source Tailwind**, `styles.css` est le **build**
(`npm run tailwind:build`). Édite `input.css`, jamais `styles.css` directement.

## Persistance

- **Postgres** (sqlx) est **optionnel** : sans `DATABASE_URL`, le `SessionStore` tourne
  purement en mémoire (cf. `sessions.rs` — L1 mémoire = chemin chaud, L2 DB = survit aux
  redémarrages). Au boot, les maps sont hydratées depuis la DB si elle existe.
- **Vectoriel** : Qdrant en local, Cloudflare Vectorize en prod (embeddings BGE-M3).

## Voir aussi

- [pipeline.md](pipeline.md) pour la machine à états métier.
- [env.md](env.md) pour ce qu'il faut configurer.
