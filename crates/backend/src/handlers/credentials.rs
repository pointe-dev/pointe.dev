//! POST /api/credentials/provision — in-app credential provisioning (offer tier
//! "Assisté"). Given a service the design needs and the client's secret field(s),
//! create the matching n8n credential so `create_from_code` auto-wires it at deploy.
//!
//! Catalog-driven (see [`crate::capabilities`]): API-key services are provisioned
//! now; OAuth2 returns a guided-handoff status (deferred); bespoke/unknown services
//! are reported as manual. Session-gated: only an unlocked (email-confirmed) session
//! may create credentials, so this is never an open relay into n8n.

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;

use crate::capabilities::{self, Auth};
use crate::credentials::N8nRestConfig;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ProvisionRequest {
    pub session_id: String,
    /// Service name as it appears in the design (e.g. "Notion", "OpenAI").
    pub service: String,
    /// The credential's secret field(s), e.g. {"apiKey": "..."} — shape per n8n schema.
    #[serde(default)]
    pub secrets: Map<String, Value>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProvisionResponse {
    /// Credential created and ready — `create_from_code` will auto-wire it.
    Created { service: String, credential_id: String },
    /// OAuth2 app keys saved (shell created); consent still to be finalized.
    OauthPending { service: String, credential_id: String, message: String },
    /// OAuth2 service — needs the app keys / a consent flow (guided handoff).
    OauthRequired { service: String, message: String },
    /// No credential needed (webhook/schedule/RSS/etc.).
    NoCredentialNeeded { service: String },
    /// Bespoke / not-yet-wired — handled by our team (Managed tier).
    Manual { service: String, message: String },
}

/// POST /api/credentials/provision
pub async fn provision(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ProvisionRequest>,
) -> Result<Json<ProvisionResponse>, StatusCode> {
    // Gate: only an unlocked session may provision credentials into n8n.
    if !state.sessions.is_unlocked(&payload.session_id).await {
        return Err(StatusCode::FORBIDDEN);
    }

    let service = payload.service.clone();
    let cap = capabilities::classify(&service);

    let resp = match cap {
        Some(c) if c.auth == Auth::None => ProvisionResponse::NoCredentialNeeded { service },
        Some(c) if c.auth == Auth::ApiKey => match c.cred_type {
            Some(cred_type) => {
                let rest = N8nRestConfig::from_env()
                    .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
                let name = format!("{} — pointe.dev", c.service);
                match rest.create_credential(&state.http, cred_type, &name, &payload.secrets).await {
                    Ok(id) => {
                        tracing::info!("[credentials] provisioned {cred_type} (id={id}) for session={}", payload.session_id);
                        ProvisionResponse::Created { service, credential_id: id }
                    }
                    Err(e) => {
                        tracing::warn!("[credentials] provision {cred_type} failed: {e}");
                        return Err(StatusCode::BAD_GATEWAY);
                    }
                }
            }
            None => ProvisionResponse::Manual {
                service,
                message: "Provisioning automatique pas encore disponible pour ce service — notre équipe le câble.".into(),
            },
        },
        // OAuth2: if the OAuth app keys (clientId+clientSecret) are supplied and we
        // know the credential type, pre-create the credential shell so only the
        // consent click remains. The consent redirect itself is owner/infra-side
        // (deferred) — see the product-strategy memory. Without keys → guided handoff.
        Some(c) => {
            let has_keys = payload.secrets.contains_key("clientId")
                && payload.secrets.contains_key("clientSecret");
            match (c.cred_type, has_keys) {
                (Some(cred_type), true) => {
                    let rest = N8nRestConfig::from_env().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
                    let name = format!("{} OAuth — pointe.dev", c.service);
                    match rest.create_credential(&state.http, cred_type, &name, &payload.secrets).await {
                        Ok(id) => {
                            tracing::info!("[credentials] OAuth shell {cred_type} (id={id}) for session={}", payload.session_id);
                            ProvisionResponse::OauthPending {
                                service, credential_id: id,
                                message: "Identifiants OAuth enregistrés — la connexion (consentement) se finalise avec notre équipe.".into(),
                            }
                        }
                        Err(e) => { tracing::warn!("[credentials] OAuth shell {cred_type} failed: {e}"); return Err(StatusCode::BAD_GATEWAY); }
                    }
                }
                _ => ProvisionResponse::OauthRequired {
                    service,
                    message: "Ce service nécessite une connexion OAuth. Notre équipe vous guide pour l'autoriser en quelques clics.".into(),
                },
            }
        }
        None => ProvisionResponse::Manual {
            service,
            message: "Service sur mesure — intégration réalisée par notre équipe (niveau Managé).".into(),
        },
    };

    Ok(Json(resp))
}
