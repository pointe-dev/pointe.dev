# Smoke test: GET /api/pitch/result
#
# Layer   : API smoke — runs against live production (https://go.pointe.dev)
# Covers  : Pitch poll returns ready=false for an unknown session ID,
#           returns 400 when the sid query parameter is missing.
# Does NOT cover: Actual pitch generation (requires completing a full pipeline),
#                 Postgres read path, manual_quote flag.

# ── Happy path: unknown session returns { ready: false } ─────────────────────
GET {{base_url}}/api/pitch/result?sid=00000000-0000-0000-0000-000000000000
X-CI-Token: {{ci_token}}

HTTP 200
[Asserts]
jsonpath "$.ready" == false

# ── Error case: missing sid → 400 ─────────────────────────────────────────────
GET {{base_url}}/api/pitch/result
X-CI-Token: {{ci_token}}

HTTP 400
