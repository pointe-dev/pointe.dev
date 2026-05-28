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
