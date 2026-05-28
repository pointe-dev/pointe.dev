use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::sync::Arc;

/// multilingual-e5-large: FR/EN/DE/... trained specifically for retrieval, 1024 dims.
const MODEL: EmbeddingModel = EmbeddingModel::MultilingualE5Large;
pub const VECTOR_DIM: usize = 1024;

/// Wraps `TextEmbedding` in an Arc so it's cheaply cloneable and shareable across tasks.
#[derive(Clone)]
pub struct EmbeddingEngine(Arc<TextEmbedding>);

impl EmbeddingEngine {
    /// Downloads the model on first call (~300 MB, cached afterwards).
    /// Run this via `spawn_blocking` at startup to avoid blocking the runtime.
    pub fn new() -> Result<Self, String> {
        let model = TextEmbedding::try_new(
            InitOptions::new(MODEL).with_show_download_progress(true),
        )
        .map_err(|e| format!("embedding model init failed: {e}"))?;
        Ok(Self(Arc::new(model)))
    }

    /// Embeds a single text. Offloads CPU work to a blocking thread.
    pub async fn embed(&self, text: String) -> Result<Vec<f32>, String> {
        let model = self.0.clone();
        tokio::task::spawn_blocking(move || {
            model
                .embed(vec![text.as_str()], None)
                .map(|mut v| v.remove(0))
                .map_err(|e| format!("embed error: {e}"))
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }

    /// Embeds a batch of texts. More efficient than calling `embed` in a loop.
    pub async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        let model = self.0.clone();
        tokio::task::spawn_blocking(move || {
            let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
            model
                .embed(refs, None)
                .map_err(|e| format!("embed_batch error: {e}"))
        })
        .await
        .map_err(|e| format!("spawn_blocking: {e}"))?
    }
}
