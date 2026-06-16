use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use crate::pipeline;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct StartRequest {
    pub session_id: String,
    pub client_need: String,
}

#[derive(Serialize)]
pub struct StartResponse {
    pub pipeline_id: String,
}

/// POST /api/pipeline/start
/// Called by the qualifier once the prospect is deemed worth pursuing.
pub async fn start(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<StartRequest>,
) -> Json<StartResponse> {
    let id = state.pipelines.create(payload.session_id, payload.client_need, None).await;
    pipeline::spawn(id, state.pipelines.clone(), state.clone());
    Json(StartResponse { pipeline_id: id.to_string() })
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub pipeline_id: String,
    pub stage: serde_json::Value,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_quote: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_monthly: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_justification: Option<String>,
}

/// GET /api/pipeline/:id
/// Polls pipeline status. Frontend uses this to show progress to the operator.
pub async fn status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<StatusResponse>, StatusCode> {
    let guard = state.pipelines.0.read().await;
    let record = guard.get(&id).ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(StatusResponse {
        pipeline_id: id.to_string(),
        stage: serde_json::to_value(&record.stage).unwrap_or_default(),
        updated_at: record.updated_at.to_rfc3339(),
        price_quote: record.ctx.price_quote,
        price_monthly: record.ctx.price_monthly,
        price_justification: record.ctx.price_justification.clone(),
    }))
}

#[derive(Serialize)]
pub struct DeliveryItemDto {
    pub service: String,
    /// "native" | "http" | "managed"
    pub tier: String,
    /// "none" | "api_key" | "oauth2"
    pub auth: String,
    /// true when the client can self-serve provision it now (API-key + wired type).
    pub provisionable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prerequisite: Option<String>,
}

#[derive(Serialize)]
pub struct DeliveryResponse {
    pub pipeline_id: String,
    pub items: Vec<DeliveryItemDto>,
}

/// Maps a design blueprint to delivery-checklist DTOs via the capability catalog.
/// Shared by the per-pipeline endpoint and the client dashboard.
pub fn delivery_dtos(design_summary: &str) -> Vec<DeliveryItemDto> {
    use crate::capabilities::{Auth, Tier};
    crate::capabilities::delivery_plan(design_summary).into_iter().map(|d| {
        DeliveryItemDto {
            service: d.service,
            tier: match d.tier { Tier::Native => "native", Tier::Http => "http", Tier::Managed => "managed" }.to_string(),
            auth: match d.auth { Auth::None => "none", Auth::ApiKey => "api_key", Auth::OAuth2 => "oauth2" }.to_string(),
            provisionable: d.auth == Auth::ApiKey && d.cred_type.is_some(),
            prerequisite: d.auth.prerequisite().map(str::to_string),
        }
    }).collect()
}

/// GET /api/pipeline/:id/delivery
/// The post-payment delivery checklist (c.1): the design's integrations, each
/// classified by the capability catalog with its provisioning path.
pub async fn delivery(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<DeliveryResponse>, StatusCode> {
    let guard = state.pipelines.0.read().await;
    let record = guard.get(&id).ok_or(StatusCode::NOT_FOUND)?;
    let items = delivery_dtos(record.ctx.design_summary.as_deref().unwrap_or(""));
    Ok(Json(DeliveryResponse { pipeline_id: id.to_string(), items }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_dtos_classifies_and_flags_provisionable() {
        let design = "Blocs clés: Notion, Gmail, CRM maison\nPoints de vigilance: aucun";
        let dtos = delivery_dtos(design);
        let notion = dtos.iter().find(|d| d.service == "Notion").unwrap();
        assert_eq!(notion.tier, "native");
        assert_eq!(notion.auth, "api_key");
        assert!(notion.provisionable, "Notion (api_key + cred_type) is provisionable");

        let gmail = dtos.iter().find(|d| d.service == "Gmail").unwrap();
        assert_eq!(gmail.auth, "oauth2");
        assert!(!gmail.provisionable, "OAuth services are not self-serve-provisionable");

        let crm = dtos.iter().find(|d| d.service.contains("CRM")).unwrap();
        assert_eq!(crm.tier, "managed");
    }

    #[test]
    fn delivery_dtos_empty_without_design() {
        assert!(delivery_dtos("").is_empty());
    }
}

/// POST /api/pipeline/:id/resume
/// Called by the Stripe webhook after payment is confirmed.
pub async fn resume(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> StatusCode {
    if state.pipelines.resume_after_payment(id).await {
        pipeline::spawn(id, state.pipelines.clone(), state.clone());
        StatusCode::OK
    } else {
        StatusCode::CONFLICT
    }
}
