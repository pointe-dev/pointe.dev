use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::agents;
use crate::state::AppState;

pub const MAX_BUILD_ATTEMPTS: u8 = 3;

/// The stage an automation pipeline is currently in.
/// Serialized as `{ "stage": "building", ... }` for the status API.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum PipelineStage {
    /// Chat qualification in progress (driven by /api/ai/chat).
    Qualifying,
    /// Research agent running.
    Researching,
    /// Workflow builder running (attempt tracked in ctx.build_attempts).
    Building,
    /// Critic agent validating the latest draft.
    Validating,
    /// Pricing agent computing the quote.
    Pricing,
    /// Waiting for Stripe payment — pipeline is paused.
    AwaitingPayment,
    /// Deploying to n8n after payment confirmed.
    Deploying,
    /// Workflow live in production.
    Live,
    /// Critic could not approve after MAX_BUILD_ATTEMPTS — needs human review.
    SavedForHuman { reason: String },
    /// Unrecoverable error.
    Failed { reason: String },
}

/// Accumulated context flowing through all pipeline stages.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineContext {
    pub session_id: String,
    /// Raw description from the qualifier chat.
    pub client_need: String,
    /// Structured summary produced by run_qualifier.
    pub qualification_summary: Option<String>,
    /// Human-readable findings from run_research (fed into builder/critic prompts).
    pub research_output: Option<String>,
    /// Structured research data (integrations, complexity, risks) — used by pricing.
    pub research_json: Option<serde_json::Value>,
    /// n8n workflow JSON produced by run_builder.
    pub workflow_json: Option<serde_json::Value>,
    /// Critic feedback accumulated across build attempts.
    pub critic_feedback: Vec<String>,
    /// Number of build attempts so far.
    pub build_attempts: u8,
    /// Price in euros, set by run_pricing.
    pub price_quote: Option<u32>,
    /// Client-facing justification for the price.
    pub price_justification: Option<String>,
    /// n8n workflow ID after deployment.
    pub n8n_workflow_id: Option<String>,
    /// Direct URL to the workflow in the n8n editor.
    pub n8n_workflow_url: Option<String>,
    /// "own" (our instance) or "client" (client's own n8n). Defaults to "own".
    pub deploy_target: Option<String>,
    /// Client's n8n URL (only set when deploy_target = "client").
    pub client_n8n_url: Option<String>,
    /// Client's n8n API key (only set when deploy_target = "client").
    pub client_n8n_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PipelineRecord {
    pub id: Uuid,
    pub stage: PipelineStage,
    pub ctx: PipelineContext,
    pub updated_at: DateTime<Utc>,
}

/// In-memory pipeline store. Will be backed by a DB once stable.
#[derive(Clone)]
pub struct PipelineStore(pub Arc<RwLock<HashMap<Uuid, PipelineRecord>>>);

impl PipelineStore {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(HashMap::new())))
    }

    /// Creates a new pipeline, returns its ID.
    /// `qualification_summary` is pre-filled from the chat qualify block when available.
    pub async fn create(
        &self,
        session_id: String,
        client_need: String,
        qualification_summary: Option<String>,
    ) -> Uuid {
        let id = Uuid::new_v4();
        self.0.write().await.insert(id, PipelineRecord {
            id,
            stage: PipelineStage::Qualifying,
            ctx: PipelineContext {
                session_id,
                client_need,
                qualification_summary,
                ..Default::default()
            },
            updated_at: Utc::now(),
        });
        id
    }

    pub async fn status(&self, id: Uuid) -> Option<(PipelineStage, DateTime<Utc>)> {
        self.0.read().await.get(&id).map(|r| (r.stage.clone(), r.updated_at))
    }

    pub async fn get_ctx(&self, id: Uuid) -> Option<PipelineContext> {
        self.0.read().await.get(&id).map(|r| r.ctx.clone())
    }

    pub async fn advance(&self, id: Uuid, stage: PipelineStage, ctx: PipelineContext) {
        if let Some(r) = self.0.write().await.get_mut(&id) {
            r.stage = stage;
            r.ctx = ctx;
            r.updated_at = Utc::now();
        }
    }

    /// Resumes a pipeline that was paused at AwaitingPayment (Stripe webhook callback).
    pub async fn resume_after_payment(&self, id: Uuid) -> bool {
        let mut guard = self.0.write().await;
        if let Some(r) = guard.get_mut(&id) {
            if r.stage == PipelineStage::AwaitingPayment {
                r.stage = PipelineStage::Deploying;
                r.updated_at = Utc::now();
                return true;
            }
        }
        false
    }
}

/// Spawns the pipeline as a background Tokio task.
pub fn spawn(id: Uuid, store: PipelineStore, app: Arc<AppState>) {
    tokio::spawn(async move {
        tracing::info!("[pipeline {id}] started");
        if let Err(reason) = run(id, &store, &app).await {
            let ctx = store.get_ctx(id).await.unwrap_or_default();
            store.advance(id, PipelineStage::Failed { reason: reason.clone() }, ctx).await;
            tracing::error!("[pipeline {id}] failed: {reason}");
        } else {
            tracing::info!("[pipeline {id}] reached terminal stage");
        }
    });
}

async fn run(id: Uuid, store: &PipelineStore, app: &Arc<AppState>) -> Result<(), String> {
    loop {
        let (stage, mut ctx) = {
            let guard = store.0.read().await;
            let r = guard.get(&id).ok_or_else(|| "pipeline record not found".to_string())?;
            (r.stage.clone(), r.ctx.clone())
        };

        match stage {
            PipelineStage::Qualifying => {
                agents::run_qualifier(app, &mut ctx).await.map_err(|e| e.to_string())?;
                store.advance(id, PipelineStage::Researching, ctx).await;
            }

            PipelineStage::Researching => {
                agents::run_research(app, &mut ctx).await.map_err(|e| e.to_string())?;
                store.advance(id, PipelineStage::Building, ctx).await;
            }

            PipelineStage::Building => {
                ctx.build_attempts += 1;
                agents::run_builder(app, &mut ctx).await.map_err(|e| e.to_string())?;
                store.advance(id, PipelineStage::Validating, ctx).await;
            }

            PipelineStage::Validating => {
                let approved = agents::run_critic(app, &mut ctx).await.map_err(|e| e.to_string())?;
                if approved {
                    store.advance(id, PipelineStage::Pricing, ctx).await;
                } else if ctx.build_attempts >= MAX_BUILD_ATTEMPTS {
                    store.advance(
                        id,
                        PipelineStage::SavedForHuman {
                            reason: format!(
                                "critic rejected after {} attempts: {}",
                                ctx.build_attempts,
                                ctx.critic_feedback.last().cloned().unwrap_or_default()
                            ),
                        },
                        ctx,
                    ).await;
                    break;
                } else {
                    store.advance(id, PipelineStage::Building, ctx).await;
                }
            }

            PipelineStage::Pricing => {
                agents::run_pricing(app, &mut ctx).await.map_err(|e| e.to_string())?;
                // Pause: wait for Stripe webhook to call resume_after_payment
                store.advance(id, PipelineStage::AwaitingPayment, ctx).await;
                break;
            }

            PipelineStage::Deploying => {
                agents::run_deploy(app, &mut ctx).await.map_err(|e| e.to_string())?;
                store.advance(id, PipelineStage::Live, ctx).await;
                break;
            }

            // Terminal or externally-driven stages — stop the loop
            PipelineStage::AwaitingPayment
            | PipelineStage::Live
            | PipelineStage::SavedForHuman { .. }
            | PipelineStage::Failed { .. } => break,
        }
    }
    Ok(())
}
