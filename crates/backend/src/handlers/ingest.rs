use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::cloudflare::TemplateDoc;
use crate::qdrant::{TemplatePayload, TemplatePoint};
use crate::state::AppState;

/// Stable, length-bounded id from a template name, so re-ingesting upserts in
/// place instead of accumulating duplicates.
fn slug_id(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    s.trim_matches('-').chars().take(64).collect()
}

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
/// Protected by the ADMIN_INGEST_TOKEN / ADMIN_INGEST_TOKEN_FILE secret.
fn extract_admin_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get("x-admin-token")
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
        })
}

pub async fn ingest(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(templates): Json<Vec<IngestTemplate>>,
) -> Result<Json<IngestResponse>, (StatusCode, String)> {
    let Some(expected_token) = state.admin_ingest_token.as_deref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "admin ingest token not configured".to_string(),
        ));
    };

    let Some(provided_token) = extract_admin_token(&headers) else {
        return Err((StatusCode::UNAUTHORIZED, "admin token required".to_string()));
    };

    if provided_token != expected_token {
        return Err((StatusCode::UNAUTHORIZED, "invalid admin token".to_string()));
    }

    // Cloudflare RAG takes precedence when configured. workflow_json is intentionally
    // not stored (the builder uses only name/description/tags; Vectorize caps metadata).
    if let Some(cf) = &state.cloudflare {
        let mut items = Vec::with_capacity(templates.len());
        for t in templates {
            let embed_text = format!("{} — {} — {}", t.name, t.description, t.tags.join(", "));
            let vector = cf.embed(embed_text).await.map_err(|e| {
                tracing::error!("[ingest] embed failed for '{}': {e}", t.name);
                (StatusCode::INTERNAL_SERVER_ERROR, e)
            })?;
            items.push((slug_id(&t.name), vector, TemplateDoc {
                name: t.name,
                description: t.description,
                tags: t.tags,
                lang: "fr".to_string(),
                source: "n8n".to_string(),
            }));
        }
        let ingested = cf.upsert(items).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
        return Ok(Json(IngestResponse { ingested }));
    }

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
