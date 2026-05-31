# Smoke test: GET /api/health
#
# Layer   : API smoke — runs against live production (https://go.pointe.dev)
# Covers  : Health check endpoint happy path and response shape.
# Does NOT cover: database connectivity reported in health, internal metrics.

# ── Happy path ─────────────────────────────────────────────────────────────────
GET {{base_url}}/api/health

HTTP 200
[Asserts]
jsonpath "$.status" == "healthy"
jsonpath "$.service" == "pointe.dev"
