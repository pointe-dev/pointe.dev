use crate::embeddings::EmbeddingEngine;
use crate::langfuse::LangfuseClient;
use crate::pipeline::PipelineStore;
use crate::qdrant::QdrantStore;
use crate::sessions::SessionStore;
use crate::stripe::StripeClient;

pub struct AppState {
    pub anthropic_key: String,
    pub http: reqwest::Client,
    pub system_prompt: String,
    pub langfuse: Option<LangfuseClient>,
    pub sessions: SessionStore,
    pub pipelines: PipelineStore,
    pub qdrant: Option<QdrantStore>,
    pub embeddings: Option<EmbeddingEngine>,
    pub stripe: Option<StripeClient>,
    /// HMAC secret for signing persistent session tokens and confirm links.
    pub session_secret: Vec<u8>,
    /// Resend API key for transactional email. None → log link to console.
    pub resend_api_key: Option<String>,
    /// Public base URL used to build confirmation links (e.g. "https://pointe.dev").
    pub base_url: String,
    /// Owner email — receives notifications on new quote requests.
    pub owner_email: Option<String>,
}
