use hmac::{Hmac, Mac};
use reqwest::Client;
use sha2::Sha256;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct StripeClient {
    http: Client,
    pub secret_key: String,
    pub webhook_secret: String,
}

#[derive(Debug)]
pub struct CheckoutSession {
    pub id: String,
    pub url: String,
}

impl StripeClient {
    pub fn new(http: Client, secret_key: String, webhook_secret: String) -> Self {
        Self { http, secret_key, webhook_secret }
    }

    /// Creates a Stripe Checkout session for one-time payment.
    /// Returns the hosted checkout URL to redirect the client to.
    pub async fn create_checkout(
        &self,
        pipeline_id: Uuid,
        price_eur: u32,
        price_monthly_eur: u32,
        workflow_name: &str,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<CheckoutSession, String> {
        let setup_cents = (price_eur * 100).to_string();
        let monthly_cents = (price_monthly_eur * 100).to_string();
        let monthly_desc = format!("Maintenance & monitoring mensuel — {workflow_name}");

        // Stripe API uses form encoding with bracket-notation for nested fields.
        // Two line items: one-time setup + first month's recurring fee (informational).
        // The recurring subscription is handled separately after payment confirmation.
        let params = [
            ("mode", "payment"),
            ("currency", "eur"),
            // Line 0: one-time setup fee
            ("line_items[0][quantity]", "1"),
            ("line_items[0][price_data][currency]", "eur"),
            ("line_items[0][price_data][product_data][name]", workflow_name),
            ("line_items[0][price_data][product_data][description]",
                "Workflow d'automatisation sur mesure — pointe.dev"),
            ("line_items[0][price_data][unit_amount]", &setup_cents),
            // Line 1: first month's recurring fee (transparent pricing)
            ("line_items[1][quantity]", "1"),
            ("line_items[1][price_data][currency]", "eur"),
            ("line_items[1][price_data][product_data][name]", "Abonnement mensuel (1er mois)"),
            ("line_items[1][price_data][product_data][description]", &monthly_desc),
            ("line_items[1][price_data][unit_amount]", &monthly_cents),
            ("metadata[pipeline_id]", &pipeline_id.to_string()),
            ("metadata[price_monthly]", &price_monthly_eur.to_string()),
            ("success_url", success_url),
            ("cancel_url", cancel_url),
            ("allow_promotion_codes", "true"),
            ("billing_address_collection", "auto"),
            ("invoice_creation[enabled]", "true"),
        ];

        #[derive(serde::Deserialize)]
        struct Resp { id: String, url: Option<String> }

        let resp = self.http
            .post("https://api.stripe.com/v1/checkout/sessions")
            .basic_auth(&self.secret_key, Option::<&str>::None)
            .header("Stripe-Version", "2024-12-18.acacia")
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Stripe checkout request: {e}"))?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(format!("Stripe checkout {s}: {b}"));
        }

        let data: Resp = resp.json().await
            .map_err(|e| format!("Stripe checkout parse: {e}"))?;

        let url = data.url.ok_or("Stripe returned no checkout URL")?;
        Ok(CheckoutSession { id: data.id, url })
    }

    /// Read-only account health. `GET /v1/account` returns the account this key
    /// belongs to; `charges_enabled` is the single flag that decides whether ANY
    /// checkout can succeed. When it is false (e.g. live account pending identity
    /// verification), every `create_checkout` call fails at Stripe's side with an
    /// `invalid_request_error` — the funnel then shows "Paiement momentanément
    /// indisponible" only AFTER the client clicks. Probing this at boot turns that
    /// silent, late failure into a loud, accurate startup signal. Returns
    /// `Ok(charges_enabled)`; `Err` only on transport/parse failure (treated as
    /// "unknown", non-fatal).
    pub async fn account_health(&self) -> Result<bool, String> {
        #[derive(serde::Deserialize)]
        struct Account { charges_enabled: bool }

        let resp = self.http
            .get("https://api.stripe.com/v1/account")
            .basic_auth(&self.secret_key, Option::<&str>::None)
            .header("Stripe-Version", "2024-12-18.acacia")
            .send()
            .await
            .map_err(|e| format!("Stripe account request: {e}"))?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(format!("Stripe account {s}: {b}"));
        }

        let acct: Account = resp.json().await
            .map_err(|e| format!("Stripe account parse: {e}"))?;
        Ok(acct.charges_enabled)
    }

    /// Creates a Checkout Session to buy chat credits (one-time payment). The paid
    /// webhook reads `metadata.kind=topup` + `session_id` + `credit_cents` to add the
    /// purchased credits. `credit_cents` is what the client receives; `price_cents`
    /// is what they pay (allow a markup if desired — here they're equal).
    pub async fn create_credit_topup_checkout(
        &self,
        session_key: &str,
        credit_cents: i64,
        price_cents: i64,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<CheckoutSession, String> {
        let amount = price_cents.to_string();
        let credit = credit_cents.to_string();
        let params = [
            ("mode", "payment"),
            ("currency", "eur"),
            ("line_items[0][quantity]", "1"),
            ("line_items[0][price_data][currency]", "eur"),
            ("line_items[0][price_data][product_data][name]", "Crédits de conversation — pointe.dev"),
            ("line_items[0][price_data][unit_amount]", &amount),
            ("metadata[kind]", "topup"),
            ("metadata[session_id]", session_key),
            ("metadata[credit_cents]", &credit),
            ("success_url", success_url),
            ("cancel_url", cancel_url),
            ("allow_promotion_codes", "true"),
            ("billing_address_collection", "auto"),
        ];

        #[derive(serde::Deserialize)]
        struct Resp { id: String, url: Option<String> }

        let resp = self.http
            .post("https://api.stripe.com/v1/checkout/sessions")
            .basic_auth(&self.secret_key, Option::<&str>::None)
            .header("Stripe-Version", "2024-12-18.acacia")
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Stripe topup request: {e}"))?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(format!("Stripe topup {s}: {b}"));
        }
        let data: Resp = resp.json().await
            .map_err(|e| format!("Stripe topup parse: {e}"))?;
        let url = data.url.ok_or("Stripe returned no checkout URL")?;
        Ok(CheckoutSession { id: data.id, url })
    }

    /// Creates a Checkout Session for a project: a one-time setup fee plus a real
    /// recurring monthly subscription (`mode=subscription`). The paid webhook reads
    /// `metadata.kind=project_sub` + `session_id` + `monthly_gift_cents` to activate
    /// the monthly chat-credit allocation, and `metadata.pipeline_id` to resume the
    /// pipeline. `invoice.paid` on each renewal re-applies the allocation.
    ///
    /// When `price_monthly_eur` is 0 there is nothing to subscribe to — the caller
    /// should fall back to a one-time `create_checkout` instead.
    pub async fn create_subscription_checkout(
        &self,
        pipeline_id: Uuid,
        session_key: &str,
        price_eur: u32,
        price_monthly_eur: u32,
        monthly_gift_cents: i64,
        workflow_name: &str,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<CheckoutSession, String> {
        let setup_cents = (price_eur * 100).to_string();
        let monthly_cents = (price_monthly_eur * 100).to_string();
        let monthly_desc = format!("Maintenance & monitoring mensuel — {workflow_name}");
        let pid = pipeline_id.to_string();
        let gift = monthly_gift_cents.to_string();

        // mode=subscription: line 0 = one-time setup, line 1 = recurring/month.
        // Stripe bills the setup once and starts the monthly subscription.
        let params = [
            ("mode", "subscription"),
            // Line 0: one-time setup fee
            ("line_items[0][quantity]", "1"),
            ("line_items[0][price_data][currency]", "eur"),
            ("line_items[0][price_data][product_data][name]", workflow_name),
            ("line_items[0][price_data][product_data][description]",
                "Workflow d'automatisation sur mesure — pointe.dev (frais de mise en place)"),
            ("line_items[0][price_data][unit_amount]", &setup_cents),
            // Line 1: recurring monthly fee
            ("line_items[1][quantity]", "1"),
            ("line_items[1][price_data][currency]", "eur"),
            ("line_items[1][price_data][product_data][name]", "Abonnement mensuel"),
            ("line_items[1][price_data][product_data][description]", &monthly_desc),
            ("line_items[1][price_data][unit_amount]", &monthly_cents),
            ("line_items[1][price_data][recurring][interval]", "month"),
            // Metadata on the Checkout Session (read by checkout.session.completed)…
            ("metadata[kind]", "project_sub"),
            ("metadata[session_id]", session_key),
            ("metadata[monthly_gift_cents]", &gift),
            ("metadata[pipeline_id]", &pid),
            // …and mirrored onto the subscription + its invoices (read by invoice.paid
            // on each renewal to re-apply the monthly allocation).
            ("subscription_data[metadata][kind]", "project_sub"),
            ("subscription_data[metadata][session_id]", session_key),
            ("subscription_data[metadata][monthly_gift_cents]", &gift),
            ("success_url", success_url),
            ("cancel_url", cancel_url),
            ("allow_promotion_codes", "true"),
            ("billing_address_collection", "auto"),
        ];

        #[derive(serde::Deserialize)]
        struct Resp { id: String, url: Option<String> }

        let resp = self.http
            .post("https://api.stripe.com/v1/checkout/sessions")
            .basic_auth(&self.secret_key, Option::<&str>::None)
            .header("Stripe-Version", "2024-12-18.acacia")
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Stripe subscription request: {e}"))?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(format!("Stripe subscription {s}: {b}"));
        }
        let data: Resp = resp.json().await
            .map_err(|e| format!("Stripe subscription parse: {e}"))?;
        let url = data.url.ok_or("Stripe returned no checkout URL")?;
        Ok(CheckoutSession { id: data.id, url })
    }

    /// Creates a Stripe Checkout session for a pitch quote (no pipeline needed).
    pub async fn create_direct_checkout(
        &self,
        montant_eur: u32,
        label: &str,
        note: &str,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<CheckoutSession, String> {
        let cents = (montant_eur * 100).to_string();

        let params = [
            ("mode", "payment"),
            ("currency", "eur"),
            ("line_items[0][quantity]", "1"),
            ("line_items[0][price_data][currency]", "eur"),
            ("line_items[0][price_data][product_data][name]", label),
            ("line_items[0][price_data][product_data][description]", note),
            ("line_items[0][price_data][unit_amount]", &cents),
            ("allow_promotion_codes", "true"),
            ("billing_address_collection", "auto"),
            ("invoice_creation[enabled]", "true"),
            ("success_url", success_url),
            ("cancel_url", cancel_url),
        ];

        #[derive(serde::Deserialize)]
        struct Resp { id: String, url: Option<String> }

        let resp = self.http
            .post("https://api.stripe.com/v1/checkout/sessions")
            .basic_auth(&self.secret_key, Option::<&str>::None)
            .header("Stripe-Version", "2024-12-18.acacia")
            .form(&params)
            .send()
            .await
            .map_err(|e| format!("Stripe direct checkout request: {e}"))?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(format!("Stripe direct checkout {s}: {b}"));
        }

        let data: Resp = resp.json().await
            .map_err(|e| format!("Stripe direct checkout parse: {e}"))?;

        let url = data.url.ok_or("Stripe returned no checkout URL")?;
        Ok(CheckoutSession { id: data.id, url })
    }

    /// Verifies the `Stripe-Signature` header and returns the parsed event.
    /// Rejects events older than 5 minutes to prevent replay attacks.
    pub fn verify_webhook(
        &self,
        payload: &[u8],
        signature_header: &str,
    ) -> Result<serde_json::Value, String> {
        // Header format: t=<timestamp>,v1=<sig>,v1=<sig2>,...
        let mut timestamp: Option<&str> = None;
        let mut signatures: Vec<&str> = Vec::new();

        for part in signature_header.split(',') {
            if let Some(ts) = part.strip_prefix("t=") {
                timestamp = Some(ts);
            } else if let Some(sig) = part.strip_prefix("v1=") {
                signatures.push(sig);
            }
        }

        let ts = timestamp.ok_or("Stripe-Signature missing timestamp")?;

        // Replay-attack guard: reject events older than 5 minutes
        let event_ts: i64 = ts.parse().map_err(|_| "invalid timestamp")?;
        let now = chrono::Utc::now().timestamp();
        if (now - event_ts).abs() > 300 {
            return Err(format!("webhook timestamp too old ({event_ts})"));
        }

        // Recompute HMAC-SHA256 of "{timestamp}.{raw_body}"
        let signed_payload = format!("{ts}.{}", String::from_utf8_lossy(payload));
        let mut mac = HmacSha256::new_from_slice(self.webhook_secret.as_bytes())
            .map_err(|e| format!("HMAC init: {e}"))?;
        mac.update(signed_payload.as_bytes());
        let computed = hex::encode(mac.finalize().into_bytes());

        if !signatures.iter().any(|s| constant_time_eq(s.as_bytes(), computed.as_bytes())) {
            return Err("webhook signature mismatch".to_string());
        }

        serde_json::from_slice(payload).map_err(|e| format!("webhook JSON parse: {e}"))
    }
}

/// Constant-time byte comparison to prevent timing attacks.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

// ── Unit tests ────────────────────────────────────────────────────────────────
//
// Layer   : pure unit — no network calls to Stripe
// Covers  : verify_webhook() HMAC verification, replay-attack timestamp guard,
//           missing signature header, malformed JSON body, constant_time_eq
// Does NOT cover: create_checkout() (requires live Stripe API), Stripe checkout
//                 redirect flow
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    const WEBHOOK_SECRET: &str = "whsec_test_secret_for_unit_tests";

    fn fresh_ts() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    }

    fn sign_payload(secret: &str, ts: i64, payload: &[u8]) -> String {
        let signed = format!("{ts}.{}", String::from_utf8_lossy(payload));
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signed.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    fn make_client() -> StripeClient {
        StripeClient::new(
            reqwest::Client::new(),
            "sk_test_fake".to_string(),
            WEBHOOK_SECRET.to_string(),
        )
    }

    // ── account_health response parsing ────────────────────────────────────
    // Mirrors the inline `Account` struct in account_health(): the boot probe
    // hinges on reading `charges_enabled` out of GET /v1/account. A live account
    // pending identity verification returns false (this is Bug A's root cause).

    #[derive(serde::Deserialize)]
    struct AccountProbe { charges_enabled: bool }

    #[test]
    fn account_health_parses_charges_disabled() {
        // Shape Stripe returns for an unverified live account (Bug A).
        let body = r#"{"id":"acct_1Tc4xtE9iwCUGFAq","object":"account","charges_enabled":false,"payouts_enabled":false}"#;
        let acct: AccountProbe = serde_json::from_str(body).unwrap();
        assert!(!acct.charges_enabled);
    }

    #[test]
    fn account_health_parses_charges_enabled() {
        let body = r#"{"id":"acct_x","object":"account","charges_enabled":true,"payouts_enabled":true}"#;
        let acct: AccountProbe = serde_json::from_str(body).unwrap();
        assert!(acct.charges_enabled);
    }

    // ── constant_time_eq ───────────────────────────────────────────────────

    #[test]
    fn constant_time_eq_equal_slices() {
        assert!(constant_time_eq(b"hello", b"hello"));
    }

    #[test]
    fn constant_time_eq_different_slices() {
        assert!(!constant_time_eq(b"hello", b"world"));
    }

    #[test]
    fn constant_time_eq_different_lengths() {
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }

    #[test]
    fn constant_time_eq_empty_equal() {
        assert!(constant_time_eq(b"", b""));
    }

    // ── verify_webhook — valid signature ──────────────────────────────────

    #[test]
    fn verify_webhook_valid_sig_returns_parsed_json() {
        let client = make_client();
        let payload = b"{\"type\":\"checkout.session.completed\"}";
        let ts = fresh_ts();
        let sig = sign_payload(WEBHOOK_SECRET, ts, payload);
        let header = format!("t={ts},v1={sig}");
        let result = client.verify_webhook(payload, &header);
        assert!(result.is_ok(), "valid signature must be accepted: {:?}", result);
        let event = result.unwrap();
        assert_eq!(event["type"], "checkout.session.completed");
    }

    // ── verify_webhook — wrong secret ─────────────────────────────────────

    #[test]
    fn verify_webhook_wrong_secret_returns_err() {
        let client = make_client();
        let payload = b"{\"type\":\"test\"}";
        let ts = fresh_ts();
        let sig = sign_payload("wrong_secret", ts, payload);
        let header = format!("t={ts},v1={sig}");
        assert!(client.verify_webhook(payload, &header).is_err());
    }

    // ── verify_webhook — missing Stripe-Signature header ──────────────────

    #[test]
    fn verify_webhook_missing_timestamp_returns_err() {
        let client = make_client();
        let payload = b"{\"type\":\"test\"}";
        let ts = fresh_ts();
        let sig = sign_payload(WEBHOOK_SECRET, ts, payload);
        // Header without "t=..." prefix
        let header = format!("v1={sig}");
        assert!(client.verify_webhook(payload, &header).is_err());
    }

    // ── verify_webhook — stale timestamp (replay attack) ──────────────────

    #[test]
    fn verify_webhook_stale_timestamp_returns_err() {
        let client = make_client();
        let payload = b"{\"type\":\"test\"}";
        let old_ts = fresh_ts() - 400; // 400 s ago — beyond the 300 s window
        let sig = sign_payload(WEBHOOK_SECRET, old_ts, payload);
        let header = format!("t={old_ts},v1={sig}");
        assert!(client.verify_webhook(payload, &header).is_err());
    }

    // ── verify_webhook — tampered payload ─────────────────────────────────

    #[test]
    fn verify_webhook_tampered_payload_returns_err() {
        let client = make_client();
        let original = b"{\"type\":\"test\"}";
        let ts = fresh_ts();
        let sig = sign_payload(WEBHOOK_SECRET, ts, original);
        let header = format!("t={ts},v1={sig}");
        let tampered = b"{\"type\":\"tampered\"}";
        assert!(client.verify_webhook(tampered, &header).is_err());
    }

    // ── verify_webhook — invalid JSON body ────────────────────────────────

    #[test]
    fn verify_webhook_invalid_json_body_returns_err() {
        let client = make_client();
        let payload = b"not-json";
        let ts = fresh_ts();
        let sig = sign_payload(WEBHOOK_SECRET, ts, payload);
        let header = format!("t={ts},v1={sig}");
        // Signature is valid but body is not JSON
        assert!(client.verify_webhook(payload, &header).is_err());
    }
}
