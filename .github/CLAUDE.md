# CLAUDE.md — pointe.dev

Fichier de contexte pour Claude Code. Lis ce fichier en entier avant toute action.

---

## Projet

**pointe.dev** — agence d'automatisation IA, positionnement qualité premium à prix abordable.
Architecture : plusieurs services SaaS en sous-domaines de pointe.dev.
Objectif immédiat : deux produits B2B vendables, un playground de qualification client.

---

## Stack technique

| Couche | Technologie |
|---|---|
| Frontend | Next.js (TypeScript strict) + Tailwind |
| Backend | FastAPI (Python, async partout) |
| Agents | LangGraph — StateGraph uniquement, jamais AgentExecutor |
| LLM | Claude Haiku (Anthropic API) pour les agents, Claude Sonnet pour la rédaction |
| Base de données | Supabase (PostgreSQL + pgvector) |
| Broker | Redis Streams (communication inter-flottes) |
| Orchestration workflows | n8n (self-hosted) |
| Observabilité | Langfuse (tracer chaque appel LLM, chaque session) |
| Déploiement | Railway (backend + workers) · Vercel (frontend) |
| Paiements | Stripe Billing (webhooks → activation packages) |
| Perf critique | Rust + PyO3 pour les modules Python CPU-intensifs |

---

## Architecture distribuée — principe central

**Un seul point d'entrée client. Des flottes d'agents qui s'empilent.**

```
Interface client (chat playground)
        ↓
Agent Orchestrateur (LangGraph)
   → vérifie packages souscrits dans Supabase
   → publie sur Redis Stream selon le package
        ↓                    ↓                  ↓
Flotte Lead Agent    Flotte Veille Agent    Flotte X (futur)
(Package 1)          (Package 2)            (Package N)
        ↓
Infrastructure partagée (Supabase · Langfuse · Redis)
        ↓
Stripe webhook → active/désactive package → update Supabase
```

### Règle de routage dynamique

L'orchestrateur construit le graph LangGraph à partir des packages souscrits :

```python
def build_graph(client_packages: list[str]) -> CompiledGraph:
    graph = StateGraph(AgentState)
    graph.add_node("orchestrator", orchestrator_node)
    if "leads" in client_packages:
        graph.add_node("leads_fleet", leads_fleet_node)
    if "veille" in client_packages:
        graph.add_node("veille_fleet", veille_fleet_node)
    # Ajouter un package = ajouter un noeud ici, rien d'autre
    return graph.compile()
```

**Ajouter un package ne touche jamais au code existant.**

---

## Produit 1 — Agent de qualification et suivi de leads

### Problème résolu
Un commercial passe 45 min par lead (recherche manuelle, email, relances, CRM).
L'agent fait tout ça en 2 minutes. Un commercial traite 10 leads/jour → 50 avec l'agent.

### Pipeline
```
lead entrant (formulaire / LinkedIn / CSV)
→ enrichissement (secteur, taille, CA, actualités récentes)
→ score de qualification (0-100)
→ draft email personnalisé
→ séquence de relance (J+2, J+5, J+10)
→ update CRM (HubSpot ou Pipedrive natif)
```

### Agents internes à cette flotte
- `EnrichmentAgent` : scrape LinkedIn, Google News, Pappers, Societe.com
- `ScoringAgent` : calcule score de qualification selon critères client
- `DraftAgent` : rédige l'email avec Claude Haiku
- `CRMAgent` : pousse vers HubSpot/Pipedrive via API

### Prix
400–800€/mois par commercial. Coût infra : ~20€/mois. Marge >90%.

---

## Produit 2 — Agent de veille et brief commercial quotidien

### Problème résolu
Les commerciaux ratent des opportunités (levées de fonds, nominations, appels d'offres)
faute de temps pour faire de la veille. Brief personnalisé livré chaque matin à 7h.

### Contenu du brief
- Actualités des prospects chauds (Google News, LinkedIn)
- Nouvelles nominations dans les comptes cibles
- Signaux d'achat : levées de fonds, recrutements, appels d'offres
- 3 opportunités de prise de contact avec message rédigé prêt à envoyer

### Pipeline
```
n8n déclenche à 23h chaque nuit
→ agents scrapent LinkedIn / Google News / Pappers / Societe.com
→ LangGraph corrèle signaux avec liste de comptes du commercial
→ Claude Sonnet rédige le brief
→ envoi email ou Slack à 7h
```

### Agents internes à cette flotte
- `ScraperAgent` : collecte les signaux par source
- `CorrelationAgent` : matche signaux avec comptes cibles (Qdrant pour la mémoire)
- `BriefAgent` : rédige le brief avec Claude Sonnet
- `DeliveryAgent` : envoie par email ou Slack

### Prix
300–500€/mois par utilisateur. Coût infra : ~15€/mois. Marge >92%.

---

## Combo packagé
Produit 1 + Produit 2 = offre complète à 800€/mois (remise bundle).
Cible : équipes commerciales B2B de 3 à 20 personnes.

---

## Playground (acquisition client sur pointe.dev)

Interface de qualification gratuite accessible depuis :
- Bouton « Playground » dans le header
- CTA sous le hero : « Décrivez votre besoin → »
- Galerie de templates (ouvre le chat avec contexte pré-chargé)

### Flow conversationnel (boutons, pas texte libre)
1. **Intake** : nom, entreprise ou particulier, secteur
2. **Qualifier** : analyse du besoin, score de complexité
3. **Routing** : produit prêt → template → RDV expert

### Backend playground
- `POST /chat/session` — crée session, retourne session_id
- `WebSocket /chat/{session_id}` — streaming des réponses
- `GET /templates` — liste des templates avec métadonnées
- `POST /leads` — enregistre les détails qualifiés

### Tables Supabase

```sql
-- Clients et packages
clients (id, email, nom, entreprise, created_at)
client_packages (client_id, package_slug, active, stripe_subscription_id)

-- Playground
sessions (id, client_id, source, template_context, created_at)
messages (id, session_id, role, content, created_at)
leads (id, session_id, nom, entreprise, probleme_resume,
       score_complexite, routing_decision, created_at)

-- Produits
templates (id, slug, titre, description, n8n_json_url, categorie)
```

### Templates n8n disponibles
- `chatbot-rag` : Chatbot RAG entreprise (WhatsApp/Telegram)
- `lead-qualification` : Qualification automatique de leads
- `repondeur-vocal` : Répondeur vocal IA
- `triage-email` : Triage et traitement emails/support

---

## Stripe → activation automatique des packages

```python
@app.post("/webhooks/stripe")
async def stripe_webhook(event: StripeEvent):
    if event.type == "customer.subscription.created":
        package_slug = event.data.metadata["package_slug"]
        client_id = event.data.metadata["client_id"]
        await supabase.activate_package(client_id, package_slug)
    if event.type == "customer.subscription.deleted":
        await supabase.deactivate_package(client_id, package_slug)
```

---

## Conventions de code — non négociables

### Python
- Axum avec `async/await` partout — zéro code synchrone bloquant
- Pydantic v2 pour tous les schémas (BaseModel strict)
- Type hints complets sur toutes les fonctions
- LangGraph : `StateGraph` uniquement, jamais `AgentExecutor`
- Langfuse : wrapper sur chaque appel LLM sans exception

```python
# Pattern agent LangGraph standard
class AgentState(TypedDict):
    messages: list[BaseMessage]
    client_id: str
    packages: list[str]
    context: dict

async def orchestrator_node(state: AgentState) -> AgentState:
    # logique ici
    return state
```

### TypeScript
- `strict: true` dans tsconfig — aucune exception
- Pas de `any` — jamais
- Composants React fonctionnels uniquement, hooks custom pour la logique
- Fetch API natif ou `ky` pour les appels HTTP, pas axios

### Général
- Pas de commentaires inutiles — le code est autodocumenté
- Variables et fonctions en anglais, commentaires en français si nécessaire
- Chaque module a son propre fichier — pas de fichiers god-object
- Zéro secret dans le code — tout dans les variables d'environnement

---

## Variables d'environnement attendues

```bash
# Anthropic
ANTHROPIC_API_KEY=

# Supabase
SUPABASE_URL=
SUPABASE_SERVICE_KEY=

# Redis
REDIS_URL=

# Stripe
STRIPE_SECRET_KEY=
STRIPE_WEBHOOK_SECRET=

# Langfuse
LANGFUSE_PUBLIC_KEY=
LANGFUSE_SECRET_KEY=
LANGFUSE_HOST=https://cloud.langfuse.com

# n8n
N8N_WEBHOOK_URL=
```

---

## Structure de dossiers cible

```
TODO
```

---

## Priorités de build dans l'ordre

1. Schéma Supabase + migrations
2. Axum gateway (sessions + WebSocket streaming)
3. Agent Orchestrateur avec routage dynamique + qdrant (containing n8n and apify docs and templates)
4. Flotte Lead Agent (Package 1) — pipeline complet
5. Playground frontend (chat widget + boutons options)
6. Stripe webhooks + activation automatique
7. Flotte Veille Agent (Package 2)
8. Langfuse — instrumentation complète
