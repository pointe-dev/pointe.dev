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
pub async fn run_research(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[research] session={}", ctx.session_id);

    let prompt = format!(
        "You are a senior automation consultant at pointe.dev, an automation agency.\n\
Analyze the following client need and produce a technical research report.\n\n\
Client need: {}\n\
Qualification summary: {}\n\n\
Respond with ONLY valid JSON matching this exact schema:\n\
{{\n\
  \"sector\": \"string (e.g. ecommerce, real-estate, logistics)\",\n\
  \"current_tools\": [\"list of tools/software the client likely uses\"],\n\
  \"pain_points\": [\"list of specific pain points to address\"],\n\
  \"integrations_required\": [\n\
    {{\n\
      \"name\": \"Tool name\",\n\
      \"n8n_node\": \"exact n8n node identifier or null if custom HTTP\",\n\
      \"auth_type\": \"oauth2 | api_key | webhook | none\",\n\
      \"notes\": \"brief setup note\"\n\
    }}\n\
  ],\n\
  \"api_keys_to_acquire\": [\"list of API credentials pointe.dev must set up\"],\n\
  \"feasibility_score\": 0-10,\n\
  \"complexity\": \"simple | medium | complex\",\n\
  \"estimated_build_hours\": \"range e.g. 2-4\",\n\
  \"approach\": \"one sentence describing the automation architecture\",\n\
  \"risks\": [\"up to 3 technical risks or edge cases to watch\"]\n\
}}",
        ctx.client_need,
        ctx.qualification_summary.as_deref().unwrap_or("not yet available"),
    );

    #[derive(serde::Serialize)]
    struct Req { model: &'static str, max_tokens: u32, messages: Vec<Msg> }
    #[derive(serde::Serialize)]
    struct Msg { role: &'static str, content: String }
    #[derive(serde::Deserialize)]
    struct Resp { content: Vec<Content> }
    #[derive(serde::Deserialize)]
    struct Content { #[serde(rename = "type")] kind: String, text: Option<String> }

    let resp = app.http
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &app.anthropic_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&Req {
            model: "claude-sonnet-4-6",
            max_tokens: 1024,
            messages: vec![Msg { role: "user", content: prompt }],
        })
        .send()
        .await
        .map_err(|e| AgentError(format!("research request: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AgentError(format!("research Sonnet {s}: {b}")));
    }

    let ant: Resp = resp.json().await.map_err(|e| AgentError(format!("research parse: {e}")))?;
    let raw = ant.content.into_iter()
        .find(|c| c.kind == "text")
        .and_then(|c| c.text)
        .unwrap_or_default();

    let json_str = raw.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let structured: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| AgentError(format!("research JSON parse: {e} — raw: {raw}")))?;

    // Human-readable summary for builder/critic prompts
    let summary = format!(
        "Sector: {sector}\n\
Complexity: {complexity} | Feasibility: {score}/10 | Est. build: {hours}h\n\
Integrations: {integrations}\n\
API keys needed: {keys}\n\
Approach: {approach}\n\
Risks: {risks}",
        sector    = structured["sector"].as_str().unwrap_or("unknown"),
        complexity = structured["complexity"].as_str().unwrap_or("medium"),
        score     = structured["feasibility_score"].as_f64().unwrap_or(7.0),
        hours     = structured["estimated_build_hours"].as_str().unwrap_or("?"),
        integrations = structured["integrations_required"]
            .as_array().unwrap_or(&vec![])
            .iter()
            .map(|i| i["name"].as_str().unwrap_or("?"))
            .collect::<Vec<_>>().join(", "),
        keys = structured["api_keys_to_acquire"]
            .as_array().unwrap_or(&vec![])
            .iter()
            .filter_map(|k| k.as_str())
            .collect::<Vec<_>>().join(", "),
        approach  = structured["approach"].as_str().unwrap_or(""),
        risks     = structured["risks"]
            .as_array().unwrap_or(&vec![])
            .iter()
            .filter_map(|r| r.as_str())
            .collect::<Vec<_>>().join("; "),
    );

    tracing::info!(
        "[research] session={} complexity={} feasibility={}/10",
        ctx.session_id,
        structured["complexity"].as_str().unwrap_or("?"),
        structured["feasibility_score"].as_f64().unwrap_or(0.0),
    );

    ctx.research_output = Some(summary);
    ctx.research_json = Some(structured);
    Ok(())
}

/// Builds an n8n workflow JSON using Qdrant RAG over n8n templates + Apify docs.
/// Runs up to MAX_BUILD_ATTEMPTS times, with critic feedback injected each retry.
pub async fn run_builder(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[builder] session={} attempt={}", ctx.session_id, ctx.build_attempts);

    // 1. Retrieve similar templates from Qdrant (if configured)
    let templates_context = match (&app.qdrant, &app.embeddings) {
        (Some(qdrant), Some(engine)) => {
            let query = format!(
                "{} {}",
                ctx.client_need,
                ctx.research_output.as_deref().unwrap_or_default()
            );
            match engine.embed(query).await {
                Ok(vector) => match qdrant.search(vector, 3).await {
                    Ok(hits) if !hits.is_empty() => {
                        let summaries: Vec<String> = hits.iter().map(|h| {
                            format!("Template: {}\nDescription: {}\nTags: {}",
                                h.name, h.description, h.tags.join(", "))
                        }).collect();
                        tracing::info!("[builder] retrieved {} RAG templates", hits.len());
                        summaries.join("\n\n---\n\n")
                    }
                    Ok(_) => { tracing::warn!("[builder] Qdrant returned no hits"); String::new() }
                    Err(e) => { tracing::warn!("[builder] Qdrant search failed: {e}"); String::new() }
                },
                Err(e) => { tracing::warn!("[builder] embed failed: {e}"); String::new() }
            }
        }
        _ => { tracing::warn!("[builder] RAG disabled (Qdrant or embeddings not configured)"); String::new() }
    };

    // 2. Build the Sonnet prompt
    let feedback_block = if ctx.critic_feedback.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nPrevious attempt was rejected. Critic feedback to address:\n{}",
            ctx.critic_feedback.iter().enumerate()
                .map(|(i, f)| format!("{}. {f}", i + 1))
                .collect::<Vec<_>>().join("\n")
        )
    };

    let rag_block = if templates_context.is_empty() {
        String::new()
    } else {
        format!("\n\nSimilar workflow templates for reference:\n{templates_context}")
    };

    let prompt = format!(
        "You are an n8n workflow architect. Generate a production-ready n8n workflow JSON \
for the following client need.\n\n\
Client need: {}\n\
Research findings: {}{}{}\n\n\
Output ONLY valid JSON that can be imported directly into n8n. \
Use realistic node types (e.g. n8n-nodes-base.webhook, n8n-nodes-base.gmail, \
@n8n/n8n-nodes-langchain.openAi). Include node positions, connections, and \
reasonable default parameters. No explanation, just the JSON.",
        ctx.client_need,
        ctx.research_output.as_deref().unwrap_or("none"),
        rag_block,
        feedback_block,
    );

    // 3. Call Anthropic Sonnet
    #[derive(serde::Serialize)]
    struct Req { model: &'static str, max_tokens: u32, messages: Vec<Msg> }
    #[derive(serde::Serialize)]
    struct Msg { role: &'static str, content: String }
    #[derive(serde::Deserialize)]
    struct Resp { content: Vec<Content> }
    #[derive(serde::Deserialize)]
    struct Content { #[serde(rename = "type")] kind: String, text: Option<String> }

    let resp = app.http
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &app.anthropic_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&Req {
            model: "claude-sonnet-4-6",
            max_tokens: 4096,
            messages: vec![Msg { role: "user", content: prompt }],
        })
        .send()
        .await
        .map_err(|e| AgentError(format!("Sonnet request: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AgentError(format!("Sonnet {s}: {b}")));
    }

    let ant: Resp = resp.json().await.map_err(|e| AgentError(format!("Sonnet parse: {e}")))?;
    let raw = ant.content.into_iter()
        .find(|c| c.kind == "text")
        .and_then(|c| c.text)
        .unwrap_or_default();

    // Strip markdown fences if Sonnet wraps the JSON
    let json_str = raw.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    ctx.workflow_json = Some(
        serde_json::from_str(json_str)
            .map_err(|e| AgentError(format!("workflow JSON parse: {e}")))?
    );
    Ok(())
}

/// Validates the workflow for correctness, completeness, and client fit.
/// Returns true if approved, false if revisions needed (feedback appended to ctx.critic_feedback).
pub async fn run_critic(app: &AppState, ctx: &mut PipelineContext) -> Result<bool, AgentError> {
    tracing::info!("[critic] session={} attempt={}", ctx.session_id, ctx.build_attempts);

    let workflow = ctx.workflow_json.as_ref()
        .ok_or_else(|| AgentError("critic called with no workflow_json".to_string()))?;

    let prompt = format!(
        "You are a senior n8n automation architect doing a quality review.\n\n\
Client need: {}\n\
Qualification summary: {}\n\
Research findings: {}\n\n\
Workflow to review:\n{}\n\n\
Evaluate the workflow on these criteria:\n\
1. Node types are valid n8n node identifiers (e.g. n8n-nodes-base.webhook)\n\
2. All connections reference nodes that exist in the workflow\n\
3. The workflow actually solves the client need end-to-end\n\
4. No missing critical steps (error handling, data mapping, authentication)\n\
5. Complexity is appropriate — not over-engineered, not under-built\n\n\
Respond with ONLY valid JSON in this exact format:\n\
{{\"approved\": true}} \
or \
{{\"approved\": false, \"feedback\": \"Concise list of specific issues to fix, max 3 bullet points.\"}}",
        ctx.client_need,
        ctx.qualification_summary.as_deref().unwrap_or("none"),
        ctx.research_output.as_deref().unwrap_or("none"),
        serde_json::to_string_pretty(workflow).unwrap_or_default(),
    );

    #[derive(serde::Serialize)]
    struct Req { model: &'static str, max_tokens: u32, messages: Vec<Msg> }
    #[derive(serde::Serialize)]
    struct Msg { role: &'static str, content: String }
    #[derive(serde::Deserialize)]
    struct Resp { content: Vec<Content> }
    #[derive(serde::Deserialize)]
    struct Content { #[serde(rename = "type")] kind: String, text: Option<String> }
    #[derive(serde::Deserialize)]
    struct CriticVerdict { approved: bool, feedback: Option<String> }

    let resp = app.http
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &app.anthropic_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&Req {
            model: "claude-sonnet-4-6",
            max_tokens: 512,
            messages: vec![Msg { role: "user", content: prompt }],
        })
        .send()
        .await
        .map_err(|e| AgentError(format!("critic request: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AgentError(format!("critic Sonnet {s}: {b}")));
    }

    let ant: Resp = resp.json().await.map_err(|e| AgentError(format!("critic parse: {e}")))?;
    let raw = ant.content.into_iter()
        .find(|c| c.kind == "text")
        .and_then(|c| c.text)
        .unwrap_or_default();

    let json_str = raw.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let verdict: CriticVerdict = serde_json::from_str(json_str)
        .map_err(|e| AgentError(format!("critic verdict parse: {e} — raw: {raw}")))?;

    if verdict.approved {
        tracing::info!("[critic] approved on attempt {}", ctx.build_attempts);
        Ok(true)
    } else {
        let feedback = verdict.feedback.unwrap_or_else(|| "unspecified issues".to_string());
        tracing::warn!("[critic] rejected attempt {}: {feedback}", ctx.build_attempts);
        ctx.critic_feedback.push(feedback);
        Ok(false)
    }
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
