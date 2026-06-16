//! Admin dossier overview — read-only listing of every prospect pipeline with
//! its published pitch and the visitor's confirmed email.
//!
//! Layer: HTTP handler. Gated by the same admin secret as `/api/admin/ingest`
//! (`admin_ingest_token`, sent as `Authorization: Bearer …` or `x-admin-token`).
//! Does NOT cover: any mutation/validation action (read-only by design for v1).

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    Json,
};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;
use crate::state::AppState;
use crate::handlers::pipeline::{delivery_dtos, DeliveryItemDto};

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

/// At-a-glance count of what stands between a dossier and a live workflow — the
/// admin's ops scan. `managed` is the human-integration worklist.
#[derive(Serialize, Default, PartialEq, Debug)]
pub struct DeliverySummary {
    pub total: usize,
    /// Bespoke / no-API integrations needing our hands (the Managed worklist).
    pub managed: usize,
    /// OAuth services awaiting consent (guided).
    pub oauth: usize,
    /// API-key services the client can self-serve.
    pub api_key: usize,
}

/// Tallies a delivery plan into the ops summary.
pub fn summarize_delivery(items: &[DeliveryItemDto]) -> DeliverySummary {
    let mut s = DeliverySummary { total: items.len(), ..Default::default() };
    for it in items {
        if it.tier == "managed" { s.managed += 1; }
        else if it.auth == "oauth2" { s.oauth += 1; }
        else if it.provisionable { s.api_key += 1; }
    }
    s
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
    /// Live n8n workflow link, once deployed.
    pub workflow_url: Option<String>,
    /// Per-integration delivery/credential status (the c.1 checklist).
    pub delivery: Vec<DeliveryItemDto>,
    pub delivery_summary: DeliverySummary,
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

        let delivery = delivery_dtos(ctx.design_summary.as_deref().unwrap_or(""));
        let delivery_summary = summarize_delivery(&delivery);

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
            workflow_url: ctx.n8n_workflow_url,
            delivery,
            delivery_summary,
        });
    }
    Ok(Json(out))
}

/// POST /api/admin/dossiers/:id/respawn — re-run a dossier's pipeline from its
/// stored need + qualification summary (e.g. to recover a SavedForHuman/Failed
/// one). Spawns a fresh pipeline keyed by a new id; on publish it also re-emails
/// the client the proposal. Returns the new pipeline id.
pub async fn respawn(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    check_admin(&state, &headers)?;

    let pid = Uuid::parse_str(&id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid pipeline id".to_string()))?;
    let ctx = state.pipelines.get_ctx(pid).await
        .ok_or((StatusCode::NOT_FOUND, "dossier not found".to_string()))?;

    let new_id = state.pipelines.create(
        ctx.session_id.clone(),
        ctx.client_need.clone(),
        ctx.qualification_summary.clone(),
    ).await;
    crate::pipeline::spawn(new_id, state.pipelines.clone(), state.clone());

    tracing::info!("[admin] respawned dossier {pid} → new pipeline {new_id}");
    Ok(Json(serde_json::json!({ "pipeline_id": new_id.to_string() })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_delivery_tallies_each_bucket() {
        // Notion (api_key), Gmail (oauth2), bespoke CRM (managed) + Webhook (none)
        let items = delivery_dtos("Blocs clés: Notion, Gmail, CRM maison, Webhook");
        let s = summarize_delivery(&items);
        assert_eq!(s.total, 4);
        assert_eq!(s.managed, 1, "the bespoke CRM is the Managed worklist item");
        assert_eq!(s.oauth, 1, "Gmail needs consent");
        assert_eq!(s.api_key, 1, "Notion is self-serve");
    }

    #[test]
    fn summarize_delivery_empty_when_no_design() {
        assert_eq!(summarize_delivery(&delivery_dtos("")), DeliverySummary::default());
    }
}
