use crate::pipeline::PipelineContext;
use crate::state::AppState;

#[derive(Debug)]
pub struct AgentError(pub String);

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AgentError {}

impl From<reqwest::Error> for AgentError {
    fn from(e: reqwest::Error) -> Self {
        AgentError(e.to_string())
    }
}

/// Finalizes qualification: extracts a structured summary from the chat conversation.
/// Called once the qualifier chat judges the prospect worth pursuing.
pub async fn run_qualifier(_app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[qualifier] session={}", ctx.session_id);
    // TODO: POST to Anthropic Sonnet with conversation history →
    //   structured JSON: { sector, team_size, pain, current_tools, estimated_volume }
    ctx.qualification_summary = Some(format!("Besoin: {}", ctx.client_need));
    Ok(())
}

/// Researches the client's domain: required APIs, integration points, feasibility.
/// Determines which API keys pointe.dev must acquire to plug into the client's stack.
pub async fn run_research(_app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[research] session={}", ctx.session_id);
    // TODO: web search (SerpAPI/Brave) + Sonnet analysis →
    //   { apis_required: [], integration_notes: "", feasibility_score: 0-10 }
    ctx.research_output = Some("stub: aucune intégration complexe détectée".to_string());
    Ok(())
}

/// Builds an n8n workflow JSON using Qdrant RAG over n8n templates + Apify docs.
/// Runs up to MAX_BUILD_ATTEMPTS times, with critic feedback injected each retry.
pub async fn run_builder(_app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!(
        "[builder] session={} attempt={}",
        ctx.session_id,
        ctx.build_attempts
    );
    // TODO:
    //   1. Embed client_need + research_output → query Qdrant
    //   2. Reranker (Cohere) selects top-k n8n templates
    //   3. Sonnet generates workflow JSON with critic_feedback injected
    ctx.workflow_json = Some(serde_json::json!({
        "name": format!("workflow-{}", ctx.session_id),
        "nodes": [],
        "connections": {}
    }));
    Ok(())
}

/// Validates the workflow for correctness, completeness, and client fit.
/// Returns true if approved, false if revisions needed (feedback stored in ctx).
pub async fn run_critic(_app: &AppState, ctx: &mut PipelineContext) -> Result<bool, AgentError> {
    tracing::info!("[critic] session={}", ctx.session_id);
    // TODO: Sonnet reviews workflow_json against qualification_summary + research_output →
    //   { approved: bool, issues: [string], suggestions: [string] }
    // On rejection: push feedback into ctx.critic_feedback before returning false
    let _ = ctx; // suppress unused warning until implemented
    Ok(true)
}

/// Computes the price: workflow complexity score × token cost × margin target.
/// Stores a euro amount in ctx.price_quote.
pub async fn run_pricing(_app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[pricing] session={}", ctx.session_id);
    // TODO: rule-based complexity (node count, integrations, volume) + Haiku explanation →
    //   { base_cost_eur: u32, margin_multiplier: f32, final_price_eur: u32, justification: "" }
    ctx.price_quote = Some(500);
    Ok(())
}

/// Deploys the workflow to n8n via REST API and activates it.
pub async fn run_deploy(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[deploy] session={}", ctx.session_id);
    // TODO: POST /api/v1/workflows to n8n (our instance or client's)
    //       then POST /api/v1/workflows/:id/activate
    let _n8n_url = std::env::var("N8N_URL").unwrap_or_else(|_| "http://localhost:5678".to_string());
    let _n8n_key = std::env::var("N8N_API_KEY").unwrap_or_default();
    let _ = app;
    ctx.n8n_workflow_id = Some("stub-workflow-id".to_string());
    Ok(())
}
