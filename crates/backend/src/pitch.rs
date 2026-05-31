use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct PitchSlide {
    pub title: String,
    pub body: String,
    pub points: Vec<String>,
}

/// Full pitch result produced by the pipeline.
#[derive(Clone, Serialize, Deserialize)]
pub struct PitchResult {
    /// One-paragraph plain-language description of the proposed solution.
    pub solution_desc: String,
    /// Price in EUR cents (e.g. 240000 = €2 400). Zero when manual_quote=true.
    pub price_eur_cents: u32,
    /// Human-readable label for price validity (e.g. "valable 48h").
    #[serde(default)]
    pub price_validity: String,
    /// External assets the client must provision before build can start.
    pub externals_needed: Vec<String>,
    pub slides: Vec<PitchSlide>,
    /// true = pricing could not be computed; human will follow up within 24h.
    #[serde(default)]
    pub manual_quote: bool,
}

/// In-memory store with optional Postgres write-through.
/// Memory is L1 (survives the request). DB is L2 (survives restarts).
/// If DATABASE_URL is not set the store works purely in-memory.
#[derive(Clone)]
pub struct PitchStore {
    cache: Arc<RwLock<HashMap<String, PitchResult>>>,
    db:    Option<sqlx::PgPool>,
}

impl PitchStore {
    pub fn new(db: Option<sqlx::PgPool>) -> Self {
        Self { cache: Arc::new(RwLock::new(HashMap::new())), db }
    }

    pub async fn set(&self, session_id: &str, result: PitchResult) {
        self.cache.write().await.insert(session_id.to_string(), result.clone());
        tracing::info!("[pitch] result stored for session={session_id}");

        if let Some(pool) = &self.db {
            let json = match serde_json::to_value(&result) {
                Ok(v) => v,
                Err(e) => { tracing::warn!("[pitch] serialise failed: {e}"); return; }
            };
            if let Err(e) = sqlx::query(
                "INSERT INTO pitches (session_id, result_json)
                 VALUES ($1, $2)
                 ON CONFLICT (session_id) DO UPDATE SET result_json = EXCLUDED.result_json"
            )
            .bind(session_id)
            .bind(json)
            .execute(pool).await {
                tracing::warn!("[pitch] DB write failed: {e}");
            }
        }
    }

    pub async fn get(&self, session_id: &str) -> Option<PitchResult> {
        // L1 — memory
        if let Some(r) = self.cache.read().await.get(session_id).cloned() {
            return Some(r);
        }
        // L2 — database (cache miss after restart)
        if let Some(pool) = &self.db {
            let row: Option<(serde_json::Value,)> = sqlx::query_as(
                "SELECT result_json FROM pitches WHERE session_id = $1"
            )
            .bind(session_id)
            .fetch_optional(pool).await
            .unwrap_or(None);

            if let Some((json,)) = row {
                if let Ok(result) = serde_json::from_value::<PitchResult>(json) {
                    self.cache.write().await.insert(session_id.to_string(), result.clone());
                    return Some(result);
                }
            }
        }
        None
    }
}

/// Creates the pitches table if it doesn't exist.
pub async fn run_migrations(pool: &sqlx::PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS pitches (
            session_id  TEXT PRIMARY KEY,
            result_json JSONB NOT NULL,
            created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )"
    )
    .execute(pool).await?;
    tracing::info!("[pitch] DB migration complete");
    Ok(())
}
