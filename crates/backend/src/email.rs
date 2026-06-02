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

/// Renders the pitch slides into the client-facing proposal email and sends it.
/// Best-effort owner notification when `owner_email` is set. Returns the result
/// of the client send. Used by the pipeline to auto-deliver the proposal the
/// moment the pitch is published (the visitor's email is already confirmed by
/// the unlock gate, so no manual re-entry is needed).
pub async fn send_proposal(
    http: &reqwest::Client,
    api_key: &str,
    client_email: &str,
    owner_email: Option<&str>,
    slides: &[crate::pitch::PitchSlide],
) -> Result<(), String> {
    let client_html = render_proposal_html(slides);

    if let Some(owner) = owner_email {
        let owner_html = format!(
            "<div style='font-family:sans-serif;padding:24px'>\
               <h2>Proposition envoyée à un client</h2>\
               <p><b>Client :</b> {client_email}</p>\
             </div>"
        );
        if let Err(e) = resend_send(http, api_key, owner,
            "🎯 Proposition envoyée — pointe.dev", &owner_html).await {
            tracing::warn!("[quote] owner notify failed: {e}");
        }
    }

    resend_send(http, api_key, client_email, "Votre proposition pointe.dev", &client_html).await
}

/// Pure renderer for the client-facing proposal email body. Split out so the
/// HTML can be unit-tested without an HTTP call.
fn render_proposal_html(slides: &[crate::pitch::PitchSlide]) -> String {
    let slides_html = slides.iter().map(|s| {
        let points = if s.points.is_empty() {
            String::new()
        } else {
            let items = s.points.iter()
                .map(|p| format!("<li style='margin:4px 0;color:#d1d5db'>{p}</li>"))
                .collect::<String>();
            format!("<ul style='margin:0;padding-left:20px'>{items}</ul>")
        };
        format!(
            "<div style='margin-bottom:24px'>\
               <h3 style='color:#f3f4f6;font-size:15px;margin:0 0 6px'>{}</h3>\
               <p style='color:#9ca3af;font-size:13px;margin:0 0 8px'>{}</p>\
               {points}</div>",
            s.title, s.body
        )
    }).collect::<String>();

    format!(
        r#"<!DOCTYPE html><html><body style="font-family:sans-serif;background:#0a0a0a;margin:0;padding:40px 20px">
<div style="max-width:560px;margin:auto;background:#111;border-radius:16px;padding:40px;border:1px solid #222">
  <p style="color:#dc2626;font-size:20px;font-weight:700;margin:0 0 24px">pointe.dev</p>
  <h1 style="color:#f3f4f6;font-size:20px;font-weight:600;margin:0 0 24px">Votre proposition</h1>
  {slides_html}
  <p style="color:#6b7280;font-size:12px;margin-top:28px">Notre équipe vous contactera prochainement avec une estimation détaillée.</p>
</div></body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::PitchSlide;

    #[test]
    fn proposal_html_renders_titles_bodies_and_points() {
        let slides = vec![
            PitchSlide {
                title: "Ce que nous avons compris".into(),
                body:  "Automatiser la relance client.".into(),
                points: vec![],
            },
            PitchSlide {
                title: "Prochaines étapes".into(),
                body:  "Mise en place sous 2 semaines.".into(),
                points: vec!["Spec & setup".into(), "Build & test".into()],
            },
        ];
        let html = render_proposal_html(&slides);

        assert!(html.contains("Votre proposition"));
        assert!(html.contains("Ce que nous avons compris"));
        assert!(html.contains("Automatiser la relance client."));
        assert!(html.contains("Prochaines étapes"));
        // points render as <li> only for the slide that has them
        assert!(html.contains("<li style='margin:4px 0;color:#d1d5db'>Spec & setup</li>"));
        assert_eq!(html.matches("<ul ").count(), 1);
    }

    #[test]
    fn proposal_html_handles_no_slides() {
        let html = render_proposal_html(&[]);
        assert!(html.contains("Votre proposition"));
        assert_eq!(html.matches("<ul ").count(), 0);
    }
}
