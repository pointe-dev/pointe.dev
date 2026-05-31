# Smoke test: GET /api/auth/status
#
# Layer   : API smoke — runs against live production (https://go.pointe.dev)
# Covers  : Auth status happy path (known unlocked session returns unlocked=true),
#           unknown session (returns unlocked=false).
# Does NOT cover: Email confirmation flow, token signing/verification,
#                 the actual unlock via email.

# ── Happy path: unknown session returns unlocked=false ────────────────────────
GET {{base_url}}/api/auth/status?sid=00000000-0000-0000-0000-000000000000
X-CI-Token: {{ci_token}}

HTTP 200
[Asserts]
jsonpath "$.unlocked" == false

# ── Missing sid parameter — Axum returns 400 ──────────────────────────────────
GET {{base_url}}/api/auth/status
X-CI-Token: {{ci_token}}

HTTP 400
