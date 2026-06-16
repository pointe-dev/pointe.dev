//! Paid-client dashboard data (c.2 of the dashboards plan). Returns every workflow
//! belonging to the authenticated client — resolved by the email behind their
//! unlocked session — with status, price, the live workflow link, and the delivery
//! checklist. Reuses the existing email-unlock auth and the persisted pipelines.

use axum::{extract::{Query, State}, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::handlers::pipeline::{delivery_dtos, DeliveryItemDto};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ClientQuery {
    /// The visitor's session id (localStorage `_sid`); must be email-unlocked.
    pub sid: String,
}

#[derive(Serialize)]
pub struct ClientWorkflow {
    pub pipeline_id: String,
    pub stage: String,
    pub client_need: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_eur: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_url: Option<String>,
    pub delivery: Vec<DeliveryItemDto>,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct ClientWorkflowsResponse {
    pub email: String,
    pub workflows: Vec<ClientWorkflow>,
}

/// GET /api/client/workflows?sid=...
pub async fn workflows(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ClientQuery>,
) -> Result<Json<ClientWorkflowsResponse>, StatusCode> {
    client_workflows_for(&state.sessions, &state.pipelines, &state.pitches, &q.sid)
        .await
        .map(Json)
        .ok_or(StatusCode::FORBIDDEN) // None = not an email-unlocked session
}

/// Resolves a client's workflows from their session id, independent of HTTP/AppState
/// so it is unit-testable with in-memory stores. Returns `None` when the session is
/// not email-unlocked (→ 403); otherwise every pipeline whose session shares the
/// caller's email, projected for the dashboard.
pub async fn client_workflows_for(
    sessions: &crate::sessions::SessionStore,
    pipelines: &crate::pipeline::PipelineStore,
    pitches: &crate::pitch::PitchStore,
    sid: &str,
) -> Option<ClientWorkflowsResponse> {
    if !sessions.is_unlocked(sid).await {
        return None;
    }
    let email = sessions.get_email(sid).await?;

    let mut workflows = Vec::new();
    for (id, stage, updated_at, ctx) in pipelines.list_records().await {
        // Belongs to this client? (the email behind the pipeline's session)
        if sessions.get_email(&ctx.session_id).await.as_deref() != Some(email.as_str()) {
            continue;
        }
        let sv = serde_json::to_value(&stage).unwrap_or_default();
        let stage_label = sv.get("stage").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let price_eur = pitches.get(&id.to_string()).await
            .map(|p| p.price_eur_cents / 100)
            .filter(|c| *c > 0);

        workflows.push(ClientWorkflow {
            pipeline_id: id.to_string(),
            stage: stage_label,
            client_need: ctx.client_need.clone(),
            price_eur,
            workflow_url: ctx.n8n_workflow_url.clone(),
            delivery: delivery_dtos(ctx.design_summary.as_deref().unwrap_or("")),
            updated_at: updated_at.to_rfc3339(),
        });
    }

    Some(ClientWorkflowsResponse { email, workflows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::PipelineStore;
    use crate::pitch::PitchStore;
    use crate::sessions::SessionStore;

    #[tokio::test]
    async fn returns_only_the_callers_workflows() {
        let sessions = SessionStore::new();
        sessions.unlock_with_email("sidA", "a@client.com".into(), "tokA").await;
        sessions.unlock_with_email("sidB", "b@client.com".into(), "tokB").await;

        let pipelines = PipelineStore::new();
        let id_a = pipelines.create("sidA".into(), "Router mes commandes".into(), None).await;
        let _id_b = pipelines.create("sidB".into(), "Autre client".into(), None).await;
        let pitches = PitchStore::new(None);

        let resp = client_workflows_for(&sessions, &pipelines, &pitches, "sidA").await
            .expect("unlocked session must resolve");
        assert_eq!(resp.email, "a@client.com");
        assert_eq!(resp.workflows.len(), 1, "must NOT see client B's workflow");
        assert_eq!(resp.workflows[0].pipeline_id, id_a.to_string());
        assert_eq!(resp.workflows[0].client_need, "Router mes commandes");
    }

    #[tokio::test]
    async fn locked_or_unknown_session_is_forbidden() {
        let sessions = SessionStore::new();
        let pipelines = PipelineStore::new();
        let pitches = PitchStore::new(None);
        assert!(client_workflows_for(&sessions, &pipelines, &pitches, "never-seen").await.is_none());
    }
}
