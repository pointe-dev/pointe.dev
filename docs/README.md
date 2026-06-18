# Documentation interne — pointe.dev

Doc d'ingénierie pour les mainteneurs du système (pas pour les clients — celle-ci
vivra sur le site). Elle décrit **comment les pièces s'assemblent** et **renvoie au
code** (`crate::module`, `fichier:ligne`) plutôt que de le recopier : la source reste
la vérité, ces pages donnent la carte.

## Par où commencer

1. **[architecture.md](architecture.md)** — la vue d'ensemble. Lis ça en premier :
   workspace, parcours d'une requête, où vit quoi.
2. **[pipeline.md](pipeline.md)** — le cœur métier : du chat de qualification au
   workflow n8n livré, en passant par le paiement.
3. **[auth-sessions.md](auth-sessions.md)** — le gating (5 messages gratuits → double
   opt-in email → session débloquée) et le rate-limit par empreinte.
4. **[oauth.md](oauth.md)** — la connexion OAuth2 des comptes clients, déléguée à n8n.
5. **[credentials.md](credentials.md)** — le provisioning par clé API + le catalogue
   des capacités (`capabilities.rs`).
6. **[guardrails.md](guardrails.md)** — le gate anti-abus ASP/clingo avant activation.
7. **[deployment.md](deployment.md)** — Hetzner, Docker, rollback.
8. **[env.md](env.md)** — référence de toutes les variables d'environnement.

### Process (existant)

- **[WORKFLOW.md](WORKFLOW.md)** — le flux dev : branche → PR → (staging) → main.
- **[STAGING.md](STAGING.md)** — environnement de staging éphémère par PR (label `staging`).

## Le système en une phrase

Un visiteur décrit son besoin dans un chat → des agents IA qualifient, conçoivent et
chiffrent une automatisation → le client paie via Stripe → le backend construit le
workflow, le fait passer par un gate anti-abus, le déploie sur **n8n**, et guide le
client pour connecter ses comptes (clés API ou OAuth). On ne détient jamais les tokens
OAuth des clients — n8n les chiffre et les garde.

## Stack

| Couche | Techno |
|--------|--------|
| Frontend | Leptos 0.6 (Rust → WASM), Tailwind |
| Backend | Axum 0.7 (Rust), Tokio |
| Données | PostgreSQL (sqlx), Qdrant + Cloudflare Vectorize (embeddings BGE-M3) |
| IA | Claude (Anthropic), prompts/traces via Langfuse |
| Automatisation | n8n (le moteur de workflow livré au client) |
| Paiement | Stripe Checkout |
| Email | Resend |
| Anti-abus | clingo (Answer Set Programming) |

Détails et variables : [env.md](env.md).
