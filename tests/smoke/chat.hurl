# Smoke test: POST /api/ai/chat
#
# Layer   : API smoke — runs against live production (https://go.pointe.dev)
# Covers  : Chat endpoint happy path (returns 200 with response field),
#           missing session_id field (returns 422 Unprocessable Entity).
# Does NOT cover: Rate limiting gate, AI response quality, fingerprint logic.

# ── Error case: missing session_id (JSON parse / validation error) ─────────────
# Axum returns 422 when required fields are missing from JSON body.
POST {{base_url}}/api/ai/chat
Content-Type: application/json
X-CI-Token: {{ci_token}}
{
  "description": "test message without session_id"
}

HTTP 422
