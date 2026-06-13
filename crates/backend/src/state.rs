use crate::cloudflare::CloudflareRag;
use crate::embeddings::EmbeddingEngine;
use crate::langfuse::LangfuseClient;
use crate::mcp::N8nMcpConfig;
use crate::pending::PendingStore;
use crate::pipeline::PipelineStore;
use crate::pitch::PitchStore;
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
    /// Qualifications stashed between the chat gate and email confirmation,
    /// then the pipeline id spawned after confirmation. Keyed by session_id.
    pub pending: PendingStore,
    pub pitches: PitchStore,
    pub qdrant: Option<QdrantStore>,
    pub embeddings: Option<EmbeddingEngine>,
    /// Managed RAG on Cloudflare (Workers AI + Vectorize). When set, the builder
    /// and ingest use it instead of the local qdrant+embeddings pair.
    pub cloudflare: Option<CloudflareRag>,
    /// n8n MCP connector for the build pipeline. When set, the builder/critic/
    /// designer ground themselves on the real n8n node catalogue via the
    /// Anthropic Messages API MCP connector instead of a hardcoded node list.
    pub n8n_mcp: Option<N8nMcpConfig>,
    pub stripe: Option<StripeClient>,
    /// HMAC secret for signing persistent session tokens and confirm links.
    pub session_secret: Vec<u8>,
    /// Admin token required to authorize template ingestion.
    pub admin_ingest_token: Option<String>,
    /// Resend API key for transactional email. None → log link to console.
    pub resend_api_key: Option<String>,
    /// Public base URL used to build confirmation links (e.g. "https://pointe.dev").
    pub base_url: String,
    /// Owner email — receives notifications on new quote requests and failures.
    pub owner_email: Option<String>,
    /// Postgres pool for pitch persistence. None → in-memory only.
    pub db: Option<sqlx::PgPool>,
}
