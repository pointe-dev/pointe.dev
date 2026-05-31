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

// ── Unit tests ────────────────────────────────────────────────────────────────
//
// Layer   : pure unit — no I/O, no Postgres
// Covers  : PitchStore in-memory set/get, PitchResult serialisation round-trip,
//           missing-key returns None, second write overwrites first
// Does NOT cover: Postgres write-through, DB cache-miss path after restart
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pitch() -> PitchResult {
        PitchResult {
            solution_desc: "Automatic Shopify → accounting sync".to_string(),
            price_eur_cents: 120_000,
            price_validity: "valable 48h".to_string(),
            externals_needed: vec!["Shopify Admin API key".to_string()],
            slides: vec![
                PitchSlide {
                    title: "Ce que nous avons compris".to_string(),
                    body: "Vous saisissez chaque commande à la main.".to_string(),
                    points: vec!["~80 commandes/jour".to_string()],
                },
            ],
            manual_quote: false,
        }
    }

    #[tokio::test]
    async fn set_then_get_returns_same_result() {
        let store = PitchStore::new(None);
        let pitch = sample_pitch();
        store.set("sess-1", pitch.clone()).await;
        let got = store.get("sess-1").await.expect("should be present");
        assert_eq!(got.solution_desc, pitch.solution_desc);
        assert_eq!(got.price_eur_cents, 120_000);
        assert_eq!(got.price_validity, "valable 48h");
        assert!(!got.manual_quote);
    }

    #[tokio::test]
    async fn get_missing_key_returns_none() {
        let store = PitchStore::new(None);
        assert!(store.get("does-not-exist").await.is_none());
    }

    #[tokio::test]
    async fn set_overwrites_previous_value() {
        let store = PitchStore::new(None);
        store.set("sess", sample_pitch()).await;
        let updated = PitchResult {
            solution_desc: "Updated description".to_string(),
            price_eur_cents: 999,
            price_validity: String::new(),
            externals_needed: vec![],
            slides: vec![],
            manual_quote: true,
        };
        store.set("sess", updated.clone()).await;
        let got = store.get("sess").await.unwrap();
        assert_eq!(got.solution_desc, "Updated description");
        assert_eq!(got.price_eur_cents, 999);
        assert!(got.manual_quote);
    }

    #[tokio::test]
    async fn multiple_sessions_stored_independently() {
        let store = PitchStore::new(None);
        let mut p1 = sample_pitch();
        p1.price_eur_cents = 1000;
        let mut p2 = sample_pitch();
        p2.price_eur_cents = 2000;
        store.set("session-1", p1).await;
        store.set("session-2", p2).await;
        assert_eq!(store.get("session-1").await.unwrap().price_eur_cents, 1000);
        assert_eq!(store.get("session-2").await.unwrap().price_eur_cents, 2000);
    }

    #[test]
    fn pitch_result_serialises_to_expected_json_shape() {
        let pitch = sample_pitch();
        let json = serde_json::to_value(&pitch).unwrap();
        assert_eq!(json["price_eur_cents"], 120_000);
        assert_eq!(json["manual_quote"], false);
        assert!(json["slides"].is_array());
        assert_eq!(json["slides"][0]["title"], "Ce que nous avons compris");
    }

    #[test]
    fn pitch_result_deserialises_with_defaults() {
        // price_validity and manual_quote have #[serde(default)]
        let json = r#"{
            "solution_desc": "test",
            "price_eur_cents": 0,
            "externals_needed": [],
            "slides": []
        }"#;
        let pitch: PitchResult = serde_json::from_str(json).unwrap();
        assert_eq!(pitch.price_validity, "");
        assert!(!pitch.manual_quote);
    }
}
