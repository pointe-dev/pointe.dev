use crate::embeddings::EmbeddingEngine;
use crate::langfuse::LangfuseClient;
use crate::pipeline::PipelineStore;
use crate::qdrant::QdrantStore;
use crate::sessions::SessionStore;

pub struct AppState {
    pub anthropic_key: String,
    pub http: reqwest::Client,
    pub system_prompt: String,
    pub langfuse: Option<LangfuseClient>,
    pub sessions: SessionStore,
    pub pipelines: PipelineStore,
    pub qdrant: Option<QdrantStore>,
    pub embeddings: Option<EmbeddingEngine>,
}
