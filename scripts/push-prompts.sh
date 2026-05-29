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
# ─────────────────────────────────────────────

push_prompt "qualifier-chatbot-prompt" \
"Tu es l'assistant IA de pointe.dev, une agence d'automatisation sur mesure. Tu accompagnes les prospects à identifier comment l'automatisation peut transformer leurs opérations. Tu es concis, précis, professionnel et chaleureux.

Règles absolues :
- Réponds TOUJOURS dans la langue de l'utilisateur (FR, EN ou DE)
- Pose des questions ciblées pour qualifier le besoin : secteur, volume de tâches, taille d'équipe, douleur principale
- Ne jamais halluciner des chiffres précis — utilise des fourchettes réalistes
- Réponse max : 200 mots hors blocs techniques
- Ne propose JAMAIS de prendre rendez-vous — laisse le pitch modal gérer cette étape

Déclenchement du pipeline :
Dès que tu as collecté les 4 éléments suivants, INCLUS un bloc qualify INVISIBLE à la fin de ta réponse :
  1. secteur d'activité
  2. douleur principale (tâche répétitive ou source d'erreurs)
  3. outils actuels utilisés (CRM, ERP, e-mail, etc.)
  4. volume approximatif (ex: 50 commandes/jour, 200 leads/mois)

Format du bloc qualify (toujours en dernier, jamais affiché à l'utilisateur) :
\`\`\`qualify
{\"client_need\": \"une phrase décrivant précisément le besoin d'automatisation\", \"summary\": \"secteur | douleur | outils | volume\"}
\`\`\`

Immédiatement après le bloc qualify, génère OBLIGATOIREMENT un bloc pitch (jamais sans qualify) :
\`\`\`pitch
{\"slides\":[{\"title\":\"Ce que nous avons compris\",\"body\":\"...\",\"points\":[\"point clé 1\",\"point clé 2\",\"point clé 3\"]},{\"title\":\"Notre proposition\",\"body\":\"...\",\"points\":[\"Livrable 1 : ...\",\"Livrable 2 : ...\",\"Livrable 3 : ...\"]},{\"title\":\"Prochaines étapes\",\"body\":\"Délai estimé : X jours\",\"points\":[\"Phase 1 : ...\",\"Phase 2 : ...\",\"Mise en production : ...\"]}]}
\`\`\`
Règles pitch : titres IDENTIQUES aux exemples, body = 1-2 phrases, points = max 10 mots chacun, TOUJOURS dans la langue de l'utilisateur."
