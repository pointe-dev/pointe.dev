# pointe.dev — Contexte projet

## Vision
pointe.dev est une agence d'automatisation IA positionnée qualité premium
à prix abordable. Plusieurs produits SaaS en sous-domaines de pointe.dev.
Stack principale : Rust (prioritaire pour performance) / Python / TypeScript, Axum, LangGraph, n8n, Supabase,
Qdrant, Railway, Vercel.

## Playground (fonctionnalité centrale du site)

Un chat gratuit accessible via :
- Bouton « Playground » dans le header
- CTA sous le hero : « Décrivez votre besoin → »
- Galerie de templates (chaque template ouvre le chat avec son contexte
  pré-chargé)

### Flow conversationnel
L'agent guide l'utilisateur avec des options cliquables (pas juste du
texte libre) :
1. Intake : nom, entreprise ou particulier, secteur
2. Qualifier : analyse du besoin, score de complexité
3. Routing : produit prêt → template → RDV expert

### Backend playground
- Axum gateway : POST /chat/session, WebSocket streaming
- Supabase tables : sessions, messages, leads, templates
- LLM : Claude Haiku (Anthropic API directe, pas OpenRouter)
- Observabilité : Langfuse sur chaque session
- Agents : LangGraph multi-agent (Intake → Qualifier → Routing)

### Templates n8n disponibles
- Chatbot RAG entreprise (WhatsApp/Telegram)
- Lead qualification automatique
- Répondeur vocal IA
- Triage et traitement emails/support

---

## Produit 1 — Agent de qualification et suivi de leads

### Problème résolu
Un commercial passe 45 min par lead : recherche manuelle d'infos
entreprise, rédaction email personnalisé, relances, mise à jour CRM.
L'agent fait tout ça en 2 minutes.

### Pipeline technique
lead entrant (formulaire / LinkedIn / CSV)
→ enrichissement profil (secteur, taille, CA, actualités)
→ score de qualification
→ draft email personnalisé
→ séquence de relance automatique (J+2, J+5, J+10)
→ mise à jour CRM (HubSpot ou Pipedrive)

### Stack
- n8n : orchestration du pipeline
- LangGraph : agent d'enrichissement et de décision
- Claude Haiku : rédaction des emails
- Supabase : état des leads et historique
- Intégrations natives : HubSpot, Pipedrive, LinkedIn

### Modèle économique
400–800€/mois par commercial.
Coût infra : ~20€/mois. Marge brute >90%.

### Argument de vente clé
Un commercial traite 10 leads/jour manuellement → 50 avec l'agent.
Démo live avec les données réelles du prospect = signé dans la semaine.

---

## Produit 2 — Agent de veille et brief commercial quotidien

### Problème résolu
Les commerciaux ratent des opportunités (levées de fonds, nominations,
appels d'offres) chez leurs prospects faute de temps pour faire de la
veille. L'agent livre un brief personnalisé chaque matin à 7h.

### Contenu du brief quotidien
- Actualités des prospects chauds (Google News, LinkedIn)
- Nouvelles nominations dans les comptes cibles
- Signaux d'achat : levées de fonds, recrutements, appels d'offres
  (Pappers, Societe.com)
- 3 opportunités de prise de contact avec message rédigé prêt à envoyer

### Pipeline technique
n8n déclenche chaque nuit à 23h
→ agents scrapent LinkedIn / Google News / Pappers / Societe.com
→ LangGraph corrèle avec la liste de comptes du commercial
→ Claude Sonnet rédige le brief (format email 2 pages)
→ envoi email ou Slack à 7h

### Stack
- n8n : orchestration et scheduling
- LangGraph : corrélation signaux / comptes cibles
- Qdrant : mémoire vectorielle des comptes (historique enrichi)
- Claude Sonnet : rédaction du brief
- Delivery : email direct ou Slack

### Modèle économique
300–500€/mois par utilisateur.
Coût infra : ~15€/mois. Marge brute >92%.

### Argument de vente clé
Mesurable immédiatement : le commercial voit le lendemain matin si
le brief est utile. Pas besoin de convaincre sur 3 mois.

---

## Combo packagé
Produit 1 + Produit 2 = offre commerciale complète à 1000€/mois.
Cible : équipes commerciales B2B de 3 à 20 personnes.

---

## Conventions de code
- Python : Axum, Pydantic, async/await partout
- TypeScript : Next.js (frontend pointe.dev)
- Agents : LangGraph (StateGraph, pas AgentExecutor)
- DB : Supabase (PostgreSQL + pgvector pour RAG)
- Déploiement : Railway (backend) + Vercel (frontend)
- Pas de commentaires inutiles, code autodocumenté
- Typage strict partout (Python type hints + TS strict mode)