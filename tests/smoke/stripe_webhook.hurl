# Smoke test: POST /api/stripe/webhook
#
# Layer   : API smoke — runs against live production (https://go.pointe.dev)
# Covers  : Webhook rejects requests with missing Stripe-Signature header (400),
#           webhook rejects requests with an invalid signature (400).
# Does NOT cover: Valid Stripe webhook events (requires real HMAC secret),
#                 checkout.session.completed pipeline resume flow.

# ── Error case: missing Stripe-Signature header → 400 ─────────────────────────
POST {{base_url}}/api/stripe/webhook
Content-Type: application/json
{"type": "checkout.session.completed"}

HTTP 400

# ── Error case: invalid signature → 400 ───────────────────────────────────────
POST {{base_url}}/api/stripe/webhook
Content-Type: application/json
Stripe-Signature: t=1234567890,v1=invalid_signature_that_will_not_match
{"type": "checkout.session.completed"}

HTTP 400
