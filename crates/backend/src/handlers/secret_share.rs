//! One-time secret sharing (outbound: team → client).
//!
//! Layer: HTTP handler, token-gated by the same admin secret as the other
//! `/api/admin/*` routes. When the team must hand a client a secret (a temporary
//! password, an admin link, a key), this mints a single-use OneTimeSecret link
//! instead of sending it in the clear. The secret is POSTed to OneTimeSecret from
//! the *backend* so the OTS credentials (if any) never reach the WASM frontend.
//!
//! Hosting is configurable via `ONETIMESECRET_URL` (default: the public SaaS
//! `https://onetimesecret.com`). If `ONETIMESECRET_USER` + `ONETIMESECRET_TOKEN`
//! are set, the call is authenticated (higher limits / longer TTL); otherwise it
//! falls back to OTS's anonymous share, which is enough for occasional outbound use.

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::state::AppState;

const DEFAULT_OTS_URL: &str = "https://onetimesecret.com";

/// Same admin-token check as `handlers::admin` (Bearer or `x-admin-token`).
fn extract_admin_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|v| !v.is_empty())
        .or_else(|| {
            headers
                .get("x-admin-token")
                .and_then(|v| v.to_str().ok())
                .filter(|v| !v.is_empty())
        })
}

fn check_admin(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected = state.admin_ingest_token.as_deref().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let provided = extract_admin_token(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if provided != expected {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct SecretShareRequest {
    /// The secret value to wrap in a one-time link.
    pub secret: String,
    /// Optional passphrase the recipient must enter to reveal the secret.
    #[serde(default)]
    pub passphrase: Option<String>,
    /// Optional time-to-live in seconds (OTS caps this per account tier).
    #[serde(default)]
    pub ttl: Option<u32>,
}

#[derive(Serialize)]
pub struct SecretShareResponse {
    /// The single-use link to send the client.
    pub link: String,
    /// The bare secret key (for reference / building a metadata link if needed).
    pub secret_key: String,
}

/// What OTS returns from `POST /api/v1/share`. We only need `secret_key`.
#[derive(Deserialize)]
struct OtsShareReply {
    secret_key: String,
}

/// POST /api/admin/secret-share — mint a one-time OneTimeSecret link for a secret.
pub async fn secret_share(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(payload): Json<SecretShareRequest>,
) -> Result<Json<SecretShareResponse>, StatusCode> {
    check_admin(&state, &headers)?;

    if payload.secret.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let base = std::env::var("ONETIMESECRET_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_OTS_URL.to_string());
    let base = base.trim_end_matches('/');

    // Build the form body. `secret` is required; passphrase/ttl are optional.
    let mut form: Vec<(&str, String)> = vec![("secret", payload.secret.clone())];
    if let Some(p) = payload.passphrase.as_ref().filter(|p| !p.is_empty()) {
        form.push(("passphrase", p.clone()));
    }
    if let Some(ttl) = payload.ttl {
        form.push(("ttl", ttl.to_string()));
    }

    let mut req = state
        .http
        .post(format!("{base}/api/v1/share"))
        .form(&form);

    // Authenticate if creds are configured; otherwise anonymous share.
    if let (Ok(user), Ok(tok)) = (std::env::var("ONETIMESECRET_USER"), std::env::var("ONETIMESECRET_TOKEN")) {
        if !user.is_empty() && !tok.is_empty() {
            req = req.basic_auth(user, Some(tok));
        }
    }

    let resp = req.send().await.map_err(|e| {
        tracing::warn!("[secret-share] OTS request failed: {e}");
        StatusCode::BAD_GATEWAY
    })?;

    if !resp.status().is_success() {
        let s = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::warn!("[secret-share] OTS → HTTP {s}: {}", body.chars().take(200).collect::<String>());
        return Err(StatusCode::BAD_GATEWAY);
    }

    let reply: OtsShareReply = resp.json().await.map_err(|e| {
        tracing::warn!("[secret-share] OTS parse failed: {e}");
        StatusCode::BAD_GATEWAY
    })?;

    let link = format!("{base}/secret/{}", reply.secret_key);
    tracing::info!("[secret-share] minted one-time link (key={})", reply.secret_key);
    Ok(Json(SecretShareResponse { link, secret_key: reply.secret_key }))
}
