# Smoke test: GET /
#
# Layer   : API smoke — runs against live production (https://go.pointe.dev)
# Covers  : Root page serves an HTML document (200 OK, content-type html).
# Does NOT cover: WASM hydration, JavaScript execution, visual correctness.

# ── Happy path: root serves HTML ──────────────────────────────────────────────
GET {{base_url}}/

HTTP 200
[Asserts]
header "content-type" contains "text/html"
body contains "pointe"
