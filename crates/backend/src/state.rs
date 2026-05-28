use crate::langfuse::LangfuseClient;
use crate::pipeline::PipelineStore;
use crate::qdrant::QdrantStore;
use crate::sessions::SessionStore;

pub struct AppState {
    pub anthropic_key: String,
    pub openai_key: Option<String>,
    pub http: reqwest::Client,
    pub system_prompt: String,
    pub langfuse: Option<LangfuseClient>,
    pub sessions: SessionStore,
    pub pipelines: PipelineStore,
    pub qdrant: Option<QdrantStore>,
}
