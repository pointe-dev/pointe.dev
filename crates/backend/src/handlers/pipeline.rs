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
    let id = state.pipelines.create(payload.session_id, payload.client_need).await;
    pipeline::spawn(id, state.pipelines.clone(), state.clone());
    Json(StartResponse { pipeline_id: id.to_string() })
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub pipeline_id: String,
    pub stage: serde_json::Value,
    pub updated_at: String,
}

/// GET /api/pipeline/:id
/// Polls pipeline status. Frontend uses this to show progress to the operator.
pub async fn status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<StatusResponse>, StatusCode> {
    let (stage, updated_at) = state.pipelines
        .status(id)
        .await
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(StatusResponse {
        pipeline_id: id.to_string(),
        stage: serde_json::to_value(&stage).unwrap_or_default(),
        updated_at: updated_at.to_rfc3339(),
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
