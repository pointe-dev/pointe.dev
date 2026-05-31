//! Thin wrapper around the Resend API for transactional email.
//!
//! Layer: infrastructure helper — one HTTP call, no business logic
//! Does NOT cover: email template rendering, bounce/unsubscribe handling

/// Sends a single transactional email via the Resend API.
/// Returns Ok(()) on 2xx, Err(message) otherwise.
pub async fn resend_send(
    http: &reqwest::Client,
    api_key: &str,
    to: &str,
    subject: &str,
    html: &str,
) -> Result<(), String> {
    let payload = serde_json::json!({
        "from": "pointe.dev <noreply@pointe.dev>",
        "to": [to],
        "subject": subject,
        "html": html,
    });
    let resp = http
        .post("https://api.resend.com/emails")
        .header("Authorization", format!("Bearer {api_key}"))
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        Ok(())
    } else {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        Err(format!("Resend {s}: {b}"))
    }
}
