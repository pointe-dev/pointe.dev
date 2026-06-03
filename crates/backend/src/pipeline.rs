use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::agents;
use crate::state::AppState;

pub const MAX_BUILD_ATTEMPTS: u8 = 3;
pub const MAX_PRICING_ATTEMPTS: u8 = 2;

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
    /// Pricing critic validating profitability and client fairness.
    PricingValidating,
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
    /// The owning pipeline's id. Set at create(); also the key under which this
    /// run's pitch is stored, so every qualification keeps its own pitch row.
    #[serde(default)]
    pub pipeline_id: Uuid,
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
    /// One-time setup fee in euros, set by run_pricing.
    pub price_quote: Option<u32>,
    /// Monthly recurring fee in euros (maintenance + monitoring + n8n hosting if deploy_target="own").
    pub price_monthly: Option<u32>,
    /// Client-facing justification covering both one-time and monthly fees.
    pub price_justification: Option<String>,
    /// Number of pricing attempts (incremented before each run_pricing call).
    pub pricing_attempts: u8,
    /// Critic feedback accumulated across pricing attempts.
    pub pricing_critic_feedback: Vec<String>,
    /// Complexity override set by the pricing critic (replaces research_json value).
    pub pricing_complexity_override: Option<String>,
    /// Feasibility score override set by the pricing critic.
    pub pricing_feasibility_override: Option<f32>,
    /// Slides JSON produced by run_pricing, carried to run_pricing_critic for publishing.
    pub pricing_slides_json: Option<serde_json::Value>,
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

/// Pipeline store: in-memory (L1) with optional Postgres write-through (L2).
/// Persistence matters mainly for the payment handoff — `/api/stripe/checkout`
/// and the webhook's `resume_after_payment` both look the pipeline up by id, so
/// without it a backend restart between pitch and payment would strand a paying
/// customer's deploy. `.0` is the live map; `.1` is the optional pool.
#[derive(Clone)]
pub struct PipelineStore(pub Arc<RwLock<HashMap<Uuid, PipelineRecord>>>, Option<sqlx::PgPool>);

impl PipelineStore {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(HashMap::new())), None)
    }

    /// Builds the store and hydrates the in-memory map from Postgres.
    pub async fn with_db(db: Option<sqlx::PgPool>) -> Self {
        let store = Self(Arc::new(RwLock::new(HashMap::new())), db);
        store.hydrate().await;
        store
    }

    async fn hydrate(&self) {
        let Some(pool) = &self.1 else { return };
        match sqlx::query_as::<_, (String, serde_json::Value, serde_json::Value)>(
            "SELECT id, stage, ctx FROM pipelines",
        )
        .fetch_all(pool)
        .await
        {
            Ok(rows) => {
                let mut w = self.0.write().await;
                for (id_str, stage_json, ctx_json) in rows {
                    let (Ok(id), Ok(stage), Ok(ctx)) = (
                        Uuid::parse_str(&id_str),
                        serde_json::from_value::<PipelineStage>(stage_json),
                        serde_json::from_value::<PipelineContext>(ctx_json),
                    ) else { continue };
                    w.insert(id, PipelineRecord { id, stage, ctx, updated_at: Utc::now() });
                }
                tracing::info!("[pipeline] hydrated {} pipelines from DB", w.len());
            }
            Err(e) => tracing::warn!("[pipeline] hydrate failed: {e}"),
        }
    }

    /// Write-through upsert, called after the in-memory mutation with the lock
    /// released, so a slow/failed DB write never blocks the pipeline.
    async fn persist(&self, id: Uuid, stage: &PipelineStage, ctx: &PipelineContext) {
        let Some(pool) = &self.1 else { return };
        let (stage_json, ctx_json) = match (serde_json::to_value(stage), serde_json::to_value(ctx)) {
            (Ok(s), Ok(c)) => (s, c),
            _ => { tracing::warn!("[pipeline] serialise failed for {id}"); return; }
        };
        if let Err(e) = sqlx::query(
            "INSERT INTO pipelines (id, stage, ctx, updated_at)
             VALUES ($1, $2, $3, NOW())
             ON CONFLICT (id) DO UPDATE SET
                 stage = EXCLUDED.stage,
                 ctx   = EXCLUDED.ctx,
                 updated_at = NOW()",
        )
        .bind(id.to_string())
        .bind(stage_json)
        .bind(ctx_json)
        .execute(pool)
        .await
        {
            tracing::warn!("[pipeline] DB write failed for {id}: {e}");
        }
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
        let stage = PipelineStage::Qualifying;
        let ctx = PipelineContext {
            pipeline_id: id,
            session_id,
            client_need,
            qualification_summary,
            ..Default::default()
        };
        self.0.write().await.insert(id, PipelineRecord {
            id,
            stage: stage.clone(),
            ctx: ctx.clone(),
            updated_at: Utc::now(),
        });
        self.persist(id, &stage, &ctx).await;
        id
    }

    pub async fn status(&self, id: Uuid) -> Option<(PipelineStage, DateTime<Utc>)> {
        self.0.read().await.get(&id).map(|r| (r.stage.clone(), r.updated_at))
    }

    pub async fn get_ctx(&self, id: Uuid) -> Option<PipelineContext> {
        self.0.read().await.get(&id).map(|r| r.ctx.clone())
    }

    pub async fn advance(&self, id: Uuid, stage: PipelineStage, ctx: PipelineContext) {
        let found = {
            let mut w = self.0.write().await;
            if let Some(r) = w.get_mut(&id) {
                r.stage = stage.clone();
                r.ctx = ctx.clone();
                r.updated_at = Utc::now();
                true
            } else {
                false
            }
        };
        if found {
            self.persist(id, &stage, &ctx).await;
        }
    }

    /// Resumes a pipeline that was paused at AwaitingPayment (Stripe webhook callback).
    pub async fn resume_after_payment(&self, id: Uuid) -> bool {
        let snapshot = {
            let mut guard = self.0.write().await;
            match guard.get_mut(&id) {
                Some(r) if r.stage == PipelineStage::AwaitingPayment => {
                    r.stage = PipelineStage::Deploying;
                    r.updated_at = Utc::now();
                    Some((r.stage.clone(), r.ctx.clone()))
                }
                _ => None,
            }
        };
        match snapshot {
            Some((stage, ctx)) => {
                self.persist(id, &stage, &ctx).await;
                true
            }
            None => false,
        }
    }
}

/// Creates the pipelines table if it doesn't exist.
pub async fn run_migrations(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS pipelines (
            id         TEXT PRIMARY KEY,
            stage      JSONB NOT NULL,
            ctx        JSONB NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"
    )
    .execute(pool).await?;
    tracing::info!("[pipeline] DB migration complete");
    Ok(())
}

// ── Unit tests ────────────────────────────────────────────────────────────────
//
// Layer   : pure unit — no I/O, no network, no agents called
// Covers  : PipelineStore CRUD, stage transitions, resume_after_payment,
//           PipelineStage serialisation, MAX_BUILD_ATTEMPTS / MAX_PRICING_ATTEMPTS
//           constant values
// Does NOT cover: the full run() state machine (requires real Anthropic calls),
//                 spawn() background task lifecycle, Postgres persistence
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_sets_qualifying_stage() {
        let store = PipelineStore::new();
        let id = store.create("sess-1".to_string(), "need something".to_string(), None).await;
        let (stage, _) = store.status(id).await.expect("pipeline must exist");
        assert_eq!(stage, PipelineStage::Qualifying);
    }

    #[tokio::test]
    async fn create_pre_fills_qualification_summary() {
        let store = PipelineStore::new();
        let id = store.create(
            "sess-2".to_string(),
            "client need".to_string(),
            Some("pre-filled summary".to_string()),
        ).await;
        let ctx = store.get_ctx(id).await.unwrap();
        assert_eq!(ctx.qualification_summary, Some("pre-filled summary".to_string()));
        assert_eq!(ctx.client_need, "client need");
    }

    #[tokio::test]
    async fn advance_updates_stage_and_ctx() {
        let store = PipelineStore::new();
        let id = store.create("sess-3".to_string(), "need".to_string(), None).await;
        let mut ctx = store.get_ctx(id).await.unwrap();
        ctx.research_output = Some("some research".to_string());
        store.advance(id, PipelineStage::Researching, ctx.clone()).await;
        let (stage, _) = store.status(id).await.unwrap();
        assert_eq!(stage, PipelineStage::Researching);
        let saved = store.get_ctx(id).await.unwrap();
        assert_eq!(saved.research_output, Some("some research".to_string()));
    }

    #[tokio::test]
    async fn status_returns_none_for_unknown_id() {
        let store = PipelineStore::new();
        assert!(store.status(Uuid::new_v4()).await.is_none());
    }

    #[tokio::test]
    async fn get_ctx_returns_none_for_unknown_id() {
        let store = PipelineStore::new();
        assert!(store.get_ctx(Uuid::new_v4()).await.is_none());
    }

    #[tokio::test]
    async fn resume_after_payment_transitions_awaiting_to_deploying() {
        let store = PipelineStore::new();
        let id = store.create("sess-pay".to_string(), "need".to_string(), None).await;
        // Manually advance to AwaitingPayment
        let ctx = store.get_ctx(id).await.unwrap();
        store.advance(id, PipelineStage::AwaitingPayment, ctx).await;

        let resumed = store.resume_after_payment(id).await;
        assert!(resumed, "should return true when in AwaitingPayment");

        let (stage, _) = store.status(id).await.unwrap();
        assert_eq!(stage, PipelineStage::Deploying);
    }

    #[tokio::test]
    async fn resume_after_payment_returns_false_when_not_awaiting() {
        let store = PipelineStore::new();
        let id = store.create("sess-noawait".to_string(), "need".to_string(), None).await;
        // Pipeline is in Qualifying stage, not AwaitingPayment
        assert!(!store.resume_after_payment(id).await);
    }

    #[tokio::test]
    async fn resume_after_payment_returns_false_for_unknown_id() {
        let store = PipelineStore::new();
        assert!(!store.resume_after_payment(Uuid::new_v4()).await);
    }

    #[test]
    fn max_build_attempts_is_3() {
        assert_eq!(MAX_BUILD_ATTEMPTS, 3);
    }

    #[test]
    fn max_pricing_attempts_is_2() {
        assert_eq!(MAX_PRICING_ATTEMPTS, 2);
    }

    #[test]
    fn pipeline_stage_serialises_to_snake_case_with_tag() {
        let stage = PipelineStage::Qualifying;
        let json = serde_json::to_value(&stage).unwrap();
        assert_eq!(json["stage"], "qualifying");

        let failed = PipelineStage::Failed { reason: "test error".to_string() };
        let json2 = serde_json::to_value(&failed).unwrap();
        assert_eq!(json2["stage"], "failed");
        assert_eq!(json2["reason"], "test error");
    }

    #[test]
    fn pipeline_stage_awaiting_payment_serialises() {
        let json = serde_json::to_value(&PipelineStage::AwaitingPayment).unwrap();
        assert_eq!(json["stage"], "awaiting_payment");
    }

    #[tokio::test]
    async fn advance_does_not_panic_for_unknown_id() {
        let store = PipelineStore::new();
        let ctx = PipelineContext::default();
        // Should silently do nothing
        store.advance(Uuid::new_v4(), PipelineStage::Researching, ctx).await;
    }

    // ── Postgres persistence (gated on TEST_DATABASE_URL, skipped otherwise) ──
    // Run locally with: TEST_DATABASE_URL=postgres://… cargo test -p backend
    async fn test_pool() -> Option<sqlx::PgPool> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("TEST_DATABASE_URL set but connection failed");
        run_migrations(&pool).await.unwrap();
        Some(pool)
    }

    #[tokio::test]
    async fn awaiting_payment_pipeline_survives_restart() {
        let Some(pool) = test_pool().await else { return };

        let store = PipelineStore::with_db(Some(pool.clone())).await;
        let id = store.create("sess-persist".into(), "need persist".into(), None).await;
        let mut ctx = store.get_ctx(id).await.unwrap();
        ctx.price_quote = Some(4200);
        store.advance(id, PipelineStage::AwaitingPayment, ctx).await;

        // Simulate a restart: brand-new store, same pool → must hydrate.
        let restarted = PipelineStore::with_db(Some(pool.clone())).await;
        let (stage, _) = restarted.status(id).await.expect("pipeline must survive restart");
        assert_eq!(stage, PipelineStage::AwaitingPayment);
        assert_eq!(restarted.get_ctx(id).await.unwrap().price_quote, Some(4200));
        // resume_after_payment must work on the hydrated record.
        assert!(restarted.resume_after_payment(id).await);

        sqlx::query("DELETE FROM pipelines WHERE id = $1")
            .bind(id.to_string()).execute(&pool).await.unwrap();
    }
}

/// Spawns the pipeline as a background Tokio task.
pub fn spawn(id: Uuid, store: PipelineStore, app: Arc<AppState>) {
    tokio::spawn(async move {
        tracing::info!("[pipeline {id}] started");
        if let Err(reason) = run(id, &store, &app).await {
            let ctx = store.get_ctx(id).await.unwrap_or_default();
            store.advance(id, PipelineStage::Failed { reason: reason.clone() }, ctx.clone()).await;
            tracing::error!("[pipeline {id}] failed: {reason}");
            notify_owner_failure(&app, id, &ctx.session_id, &reason).await;
        } else {
            tracing::info!("[pipeline {id}] reached terminal stage");
        }
    });
}

async fn notify_owner_failure(app: &AppState, id: Uuid, session_id: &str, reason: &str) {
    let (Some(api_key), Some(owner)) = (&app.resend_api_key, &app.owner_email) else { return };
    let html = format!(
        "<div style='font-family:sans-serif;padding:24px'>\
           <h2 style='color:#dc2626'>⚠️ Pipeline failed — pointe.dev</h2>\
           <p><b>Pipeline ID:</b> {id}</p>\
           <p><b>Session:</b> {session_id}</p>\
           <p><b>Reason:</b> {reason}</p>\
         </div>"
    );
    if let Err(e) = crate::email::resend_send(&app.http, api_key, owner,
        "⚠️ Pipeline failed — pointe.dev", &html).await {
        tracing::warn!("[pipeline] owner failure notify failed: {e}");
    }
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
                    agents::publish_manual_pitch(app, &ctx).await;
                    let reason = format!(
                        "critic rejected after {} attempts: {}",
                        ctx.build_attempts,
                        ctx.critic_feedback.last().cloned().unwrap_or_default()
                    );
                    notify_owner_failure(app, id, &ctx.session_id, &reason).await;
                    store.advance(id, PipelineStage::SavedForHuman { reason }, ctx).await;
                    break;
                } else {
                    store.advance(id, PipelineStage::Building, ctx).await;
                }
            }

            PipelineStage::Pricing => {
                ctx.pricing_attempts += 1;
                agents::run_pricing(app, &mut ctx).await.map_err(|e| e.to_string())?;
                store.advance(id, PipelineStage::PricingValidating, ctx).await;
            }

            PipelineStage::PricingValidating => {
                let approved = agents::run_pricing_critic(app, &mut ctx).await.map_err(|e| e.to_string())?;
                if approved {
                    // Pause: wait for Stripe webhook to call resume_after_payment
                    store.advance(id, PipelineStage::AwaitingPayment, ctx).await;
                    break;
                } else if ctx.pricing_attempts >= MAX_PRICING_ATTEMPTS {
                    agents::publish_manual_pitch(app, &ctx).await;
                    let reason = format!(
                        "pricing critic rejected after {} attempts: {}",
                        ctx.pricing_attempts,
                        ctx.pricing_critic_feedback.last().cloned().unwrap_or_default()
                    );
                    notify_owner_failure(app, id, &ctx.session_id, &reason).await;
                    store.advance(id, PipelineStage::SavedForHuman { reason }, ctx).await;
                    break;
                } else {
                    store.advance(id, PipelineStage::Pricing, ctx).await;
                }
            }

            PipelineStage::Deploying => {
                agents::run_deploy(app, &mut ctx).await.map_err(|e| e.to_string())?;
                store.advance(id, PipelineStage::Live, ctx).await;
                break;
            }

            // Terminal or externally-driven stages — stop the loop.
            // AwaitingPayment resumes via resume_after_payment() → Deploying.
            PipelineStage::AwaitingPayment
            | PipelineStage::Live
            | PipelineStage::SavedForHuman { .. }
            | PipelineStage::Failed { .. } => break,
        }
    }
    Ok(())
}
