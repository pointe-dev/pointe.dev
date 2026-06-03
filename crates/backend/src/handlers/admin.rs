//! Admin dossier overview — read-only listing of every prospect pipeline with
//! its published pitch and the visitor's confirmed email.
//!
//! Layer: HTTP handler. Gated by the same admin secret as `/api/admin/ingest`
//! (`admin_ingest_token`, sent as `Authorization: Bearer …` or `x-admin-token`).
//! Does NOT cover: any mutation/validation action (read-only by design for v1).

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use crate::state::AppState;

/// Reads the admin secret from `Authorization: Bearer …` or the `x-admin-token`
/// header. Mirrors `handlers::ingest` so both admin routes accept the same auth.
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

fn check_admin(state: &AppState, headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
    let Some(expected) = state.admin_ingest_token.as_deref() else {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "admin token not configured".to_string()));
    };
    let Some(provided) = extract_admin_token(headers) else {
        return Err((StatusCode::UNAUTHORIZED, "admin token required".to_string()));
    };
    if provided != expected {
        return Err((StatusCode::UNAUTHORIZED, "invalid admin token".to_string()));
    }
    Ok(())
}

#[derive(Serialize)]
pub struct DossierPitch {
    pub price_eur_cents: u32,
    pub manual_quote: bool,
    pub solution_desc: String,
}

#[derive(Serialize)]
pub struct Dossier {
    pub pipeline_id: String,
    pub session_id: String,
    pub email: Option<String>,
    pub client_need: String,
    pub summary: Option<String>,
    /// snake_case stage variant (e.g. "building", "awaiting_payment", "failed").
    pub stage: String,
    /// Set only for stages that carry one (`failed`, `saved_for_human`).
    pub stage_reason: Option<String>,
    pub updated_at: String,
    pub pitch: Option<DossierPitch>,
}

/// GET /api/admin/dossiers — every pipeline, newest first, with its pitch + email.
pub async fn dossiers(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<Vec<Dossier>>, (StatusCode, String)> {
    check_admin(&state, &headers)?;

    let records = state.pipelines.list_records().await;
    let mut out = Vec::with_capacity(records.len());
    for (id, stage, updated_at, ctx) in records {
        let pitch = state.pitches.get(&id.to_string()).await.map(|p| DossierPitch {
            price_eur_cents: p.price_eur_cents,
            manual_quote: p.manual_quote,
            solution_desc: p.solution_desc,
        });
        let email = state.sessions.get_email(&ctx.session_id).await;

        // PipelineStage is internally-tagged (`{"stage":"failed","reason":"…"}`),
        // so pull the label + optional reason straight from its serialized form.
        let sv = serde_json::to_value(&stage).unwrap_or_default();
        let stage_label = sv.get("stage").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let stage_reason = sv.get("reason").and_then(|v| v.as_str()).map(String::from);

        out.push(Dossier {
            pipeline_id: id.to_string(),
            session_id: ctx.session_id,
            email,
            client_need: ctx.client_need,
            summary: ctx.qualification_summary,
            stage: stage_label,
            stage_reason,
            updated_at: updated_at.to_rfc3339(),
            pitch,
        });
    }
    Ok(Json(out))
}
