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
        workflow_name: &str,
        success_url: &str,
        cancel_url: &str,
    ) -> Result<CheckoutSession, String> {
        // Stripe amounts are in cents
        let amount_cents = price_eur * 100;

        // Stripe API uses form encoding with bracket-notation for nested fields
        let params = [
            ("mode", "payment"),
            ("currency", "eur"),
            ("line_items[0][quantity]", "1"),
            ("line_items[0][price_data][currency]", "eur"),
            ("line_items[0][price_data][product_data][name]", workflow_name),
            ("line_items[0][price_data][product_data][description]",
                "Workflow d'automatisation sur mesure — pointe.dev"),
            ("line_items[0][price_data][unit_amount]", &amount_cents.to_string()),
            ("metadata[pipeline_id]", &pipeline_id.to_string()),
            ("success_url", success_url),
            ("cancel_url", cancel_url),
            // Allow promo codes
            ("allow_promotion_codes", "true"),
            // Collect billing address for invoicing
            ("billing_address_collection", "auto"),
        ];

        #[derive(serde::Deserialize)]
        struct Resp { id: String, url: Option<String> }

        let resp = self.http
            .post("https://api.stripe.com/v1/checkout/sessions")
            .basic_auth(&self.secret_key, Option::<&str>::None)
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
