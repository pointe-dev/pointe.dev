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

#[derive(Clone)]
pub struct PitchStore {
    results: Arc<RwLock<HashMap<String, PitchResult>>>,
}

impl PitchStore {
    pub fn new() -> Self {
        Self { results: Arc::new(RwLock::new(HashMap::new())) }
    }

    pub async fn set(&self, session_id: &str, result: PitchResult) {
        self.results.write().await.insert(session_id.to_string(), result);
        tracing::info!("[pitch] result stored for session={session_id}");
    }

    pub async fn get(&self, session_id: &str) -> Option<PitchResult> {
        self.results.read().await.get(session_id).cloned()
    }
}
