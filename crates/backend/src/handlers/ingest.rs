use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::qdrant::{TemplatePayload, TemplatePoint};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct IngestTemplate {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub workflow_json: serde_json::Value,
}

#[derive(Serialize)]
pub struct IngestResponse {
    pub ingested: usize,
}

/// POST /api/admin/ingest
/// Embeds and upserts a batch of n8n templates into Qdrant.
/// Protected in production — add auth middleware before shipping.
pub async fn ingest(
    State(state): State<Arc<AppState>>,
    Json(templates): Json<Vec<IngestTemplate>>,
) -> Result<Json<IngestResponse>, (StatusCode, String)> {
    let (Some(qdrant), Some(engine)) = (&state.qdrant, &state.embeddings) else {
        return Err((StatusCode::SERVICE_UNAVAILABLE, "Qdrant or embedding engine not configured".to_string()));
    };

    qdrant.ensure_collection().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let mut points = Vec::with_capacity(templates.len());
    for t in templates {
        let embed_text = format!("{} — {} — {}", t.name, t.description, t.tags.join(", "));
        let vector = engine.embed(embed_text).await.map_err(|e| {
            tracing::error!("[ingest] embed failed for '{}': {e}", t.name);
            (StatusCode::INTERNAL_SERVER_ERROR, e)
        })?;

        points.push(TemplatePoint {
            payload: TemplatePayload {
                name: t.name,
                description: t.description,
                tags: t.tags,
                workflow_json: serde_json::to_string(&t.workflow_json).unwrap_or_default(),
            },
            vector,
        });
    }

    let ingested = qdrant.upsert(points).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(IngestResponse { ingested }))
}
