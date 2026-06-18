# Le pipeline d'automatisation

Le cœur métier : transformer un besoin exprimé en chat en un workflow n8n livré.
Code : `backend/src/pipeline.rs` (machine à états), `backend/src/agents.rs` (agents IA),
`backend/src/pitch.rs` (assemblage du pitch), handlers `/api/pipeline/*` et `/api/pitch/*`.

## Machine à états — `PipelineStage`

Enum sérialisée `{ "stage": "building", ... }` (`pipeline.rs`). Les étapes, dans l'ordre :

| Étape | Quand | Note |
|-------|-------|------|
| `Qualifying` | chat de qualification (`/api/ai/chat`) | avant tout |
| `Researching` | l'agent de recherche tourne | |
| `Designing` | le designer rédige l'esquisse (pas de JSON) | |
| `DesignValidating` | le critique de design valide viabilité/complétude | avant de chiffrer |
| `Pricing` | l'agent de pricing calcule le devis | |
| `PricingValidating` | le critique de pricing valide rentabilité + équité client | |
| `AwaitingPayment` | **pause** en attente du paiement Stripe | reprend **uniquement** via webhook |
| `Decomposing` | décide si le design doit être scindé en sous-flux chaînés | **post-paiement** |
| `Building` | le builder génère le vrai JSON n8n | **post-paiement** |
| `Validating` | le critique valide le JSON | post-paiement |
| `Deploying` | déploiement vers n8n après paiement confirmé | |
| `Live` | workflow en production | terminal |
| `SavedForHuman { reason }` | le critique n'a pas approuvé après `MAX_BUILD_ATTEMPTS` | terminal, revue humaine |
| `Failed { reason }` | erreur irrécupérable | terminal |

**Point clé** : le vrai JSON n'est construit qu'**après le paiement**. Avant paiement on
ne produit qu'un design + un prix. Ça évite de cramer des tokens à construire des
workflows pour des visiteurs qui ne paieront pas.

Limites de tentatives (constantes) : `MAX_BUILD_ATTEMPTS = 3`, `MAX_PRICING_ATTEMPTS = 2`,
`MAX_DESIGN_ATTEMPTS = 3`.

## Reprise après redémarrage

`PipelineStage::is_resumable()` dit si `spawn()` doit re-piloter un pipeline au boot.
**Faux** pour les états terminaux (`Live` / `SavedForHuman` / `Failed`) **et** pour
`AwaitingPayment` — celui-ci est en pause volontaire : il ne reprend que par le webhook
Stripe, jamais au boot.

## Décomposition en sous-flux

Quand un design est trop gros pour un seul workflow fiable, `run_decomposer` le scinde en
`SubWorkflowPlan` (sous-flux ≤ ~8 nœuds), chaînés par un contrat d'entrée/sortie explicite
(le contexte d'exécution n8n se brise après un nœud trigger, donc le hand-off entre
sous-flux est matérialisé via `executeWorkflow`/webhook). Atteint seulement post-paiement,
juste avant `Building`.

## Flux complet (du chat à la production)

```
qualify (chat)        → /api/ai/chat, gate free-tier (voir auth-sessions.md)
  → research → design → design critic     ← boucle si le critique refuse
  → pricing → pricing critic              ← idem
  → AwaitingPayment   → pitch + prix renvoyés au front (pitch.rs)
                        front affiche le pitch modal avec le prix
  ─── le client paie (Stripe Checkout) ───
  → webhook Stripe    → reprend le pipeline
  → decompose → build → build critic      ← boucle ≤ MAX_BUILD_ATTEMPTS
  → guardrails        → gate anti-abus (voir guardrails.md) avant activation
  → deploy (n8n)      → Live
                        checklist de livraison : le client connecte ses comptes
                        (clés API → credentials.md ; OAuth → oauth.md)
```

## Endpoints

| Route | Rôle |
|-------|------|
| `POST /api/pipeline/start` | démarre un pipeline |
| `GET /api/pipeline/:id` | statut (sérialisation de `PipelineStage`) — polling front |
| `POST /api/pipeline/:id/resume` | reprise manuelle |
| `GET /api/pipeline/:id/delivery` | la checklist de livraison (intégrations à connecter) |
| `POST /api/pitch/result` / `POST /api/pitch/pipeline-result` | callbacks de résultat |

Le front **poll** `/api/pipeline/:id` pour suivre l'avancement, puis ouvre le pitch modal
avec le prix une fois `AwaitingPayment` atteint.
