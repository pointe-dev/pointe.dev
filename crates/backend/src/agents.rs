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

/// Finalizes qualification: enriches the summary from the chat qualify block.
/// If a summary already exists (from the qualify block), validates and normalises it.
/// Otherwise, asks Sonnet to infer a summary from client_need alone.
pub async fn run_qualifier(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[qualifier] session={}", ctx.session_id);

    // Already qualified by the chat — just normalise the format
    if ctx.qualification_summary.is_some() {
        tracing::info!("[qualifier] summary pre-filled from chat qualify block — skipping LLM");
        return Ok(());
    }

    // Fallback: infer from client_need (pipeline started via /api/pipeline/start directly)
    let prompt = format!(
        "Output a single line: \"sector | main pain | current tools | volume\"\n\
Input: {}",
        ctx.client_need
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
            model: "claude-haiku-4-5-20251001",
            max_tokens: 120,
            messages: vec![Msg { role: "user", content: prompt }],
        })
        .send()
        .await
        .map_err(|e| AgentError(format!("qualifier request: {e}")))?;

    if resp.status().is_success() {
        let ant: Resp = resp.json().await
            .map_err(|e| AgentError(format!("qualifier parse: {e}")))?;
        let summary = ant.content.into_iter()
            .find(|c| c.kind == "text")
            .and_then(|c| c.text)
            .unwrap_or_else(|| ctx.client_need.clone());
        ctx.qualification_summary = Some(summary);
    } else {
        // Non-fatal: research agent can work with client_need alone
        tracing::warn!("[qualifier] Haiku call failed: {} — using client_need as summary", resp.status());
        ctx.qualification_summary = Some(ctx.client_need.clone());
    }

    Ok(())
}

/// Researches the client's domain: required APIs, integration points, feasibility.
/// Determines which API keys pointe.dev must acquire to plug into the client's stack.
pub async fn run_research(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[research] session={}", ctx.session_id);

    let prompt = format!(
        "Output ONLY valid JSON. No prose, no fences.\n\
Client: {}\nSummary: {}\n\
Schema: {{sector,current_tools[],pain_points[],\
integrations_required[{{name,n8n_node,auth_type,notes}}],\
api_keys_to_acquire[],feasibility_score(0-10),\
complexity(simple|medium|complex),estimated_build_hours,\
approach(1 sentence),risks[{{description,severity(low|medium|high)}}](max 3)}}",
        ctx.client_need,
        ctx.qualification_summary.as_deref().unwrap_or(""),
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
            max_tokens: 2048,
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
        "Output ONLY valid n8n workflow JSON. No prose, no fences, no position fields.\n\
Keep parameters minimal: only type, name, typeVersion, and 2-3 essential params per node.\n\
Use {{}} for all credential/auth fields. Max 8 nodes.\n\
Required top-level: name, nodes[], connections{{}}.\n\
Client: {}\nResearch: {}{}{}",
        ctx.client_need,
        ctx.research_output.as_deref().unwrap_or(""),
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
            max_tokens: 8192,
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
        serde_json::from_str(json_str).map_err(|e| {
            let preview = &json_str[..json_str.len().min(500)];
            AgentError(format!("workflow JSON parse: {e}\nRaw (first 500): {preview}"))
        })?
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
        "Output ONLY: {{\"approved\":true}} or {{\"approved\":false,\"feedback\":\"max 3 issues\"}}\n\
Check: valid n8n node types, no broken connections, solves client need end-to-end, \
no missing auth/error-handling, appropriate complexity.\n\
Client: {}\nResearch: {}\nWorkflow:\n{}",
        ctx.client_need,
        ctx.research_output.as_deref().unwrap_or(""),
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

/// Computes the price from research_json using deterministic rules, then asks Haiku
/// to write a client-facing justification. Numbers never come from the LLM.
pub async fn run_pricing(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[pricing] session={}", ctx.session_id);

    // --- 1. Rule-based price calculation ---

    let research = ctx.research_json.as_ref();

    let complexity = research
        .and_then(|r| r["complexity"].as_str())
        .unwrap_or("medium");

    let integration_count = research
        .and_then(|r| r["integrations_required"].as_array())
        .map(|v| v.len())
        .unwrap_or(2);

    // Risk premium: low=75, medium=150, high=300 per risk
    let risk_premium: u32 = research
        .and_then(|r| r["risks"].as_array())
        .map(|risks| risks.iter().map(|r| match r["severity"].as_str() {
            Some("high")   => 300,
            Some("medium") => 150,
            _              => 75,  // low or missing
        }).sum())
        .unwrap_or(0);

    let feasibility: f32 = research
        .and_then(|r| r["feasibility_score"].as_f64())
        .map(|f| f as f32)
        .unwrap_or(7.0);

    // Base price by complexity tier (euros)
    let base: u32 = match complexity {
        "simple"  => 900,
        "complex" => 6000,
        _         => 2500, // medium
    };

    // +€200 per integration beyond the first two
    let integration_premium = (integration_count.saturating_sub(2) as u32) * 200;

    // Low feasibility (<6) adds a complexity buffer
    let feasibility_buffer: u32 = if feasibility < 6.0 { 600 } else { 0 };

    // Node count from workflow (if already built — critic approved it)
    let node_count = ctx.workflow_json.as_ref()
        .and_then(|w| w["nodes"].as_array())
        .map(|n| n.len())
        .unwrap_or(0);
    let node_premium = (node_count.saturating_sub(5) as u32) * 60;

    let subtotal = base + integration_premium + risk_premium + feasibility_buffer + node_premium;

    // Round up to nearest €50 for clean invoice numbers
    let setup_price = ((subtotal + 49) / 50) * 50;

    // Monthly recurring fee: maintenance + monitoring + n8n hosting (if our instance)
    // Base: 12% of setup, min per tier, +€30/integration beyond 2, rounded to €25
    let monthly_base: u32 = match complexity {
        "simple"  => 100,
        "complex" => 500,
        _         => 250, // medium
    };
    let monthly_integration_fee = (integration_count.saturating_sub(2) as u32) * 50;
    let monthly_subtotal = monthly_base + monthly_integration_fee;
    let monthly_price = ((monthly_subtotal + 24) / 25) * 25;

    tracing::info!(
        "[pricing] setup={setup_price}€ (base={base} int={integration_premium} risks={risk_premium} \
feas={feasibility_buffer} nodes={node_premium}) | monthly={monthly_price}€"
    );

    let integrations_str = research
        .and_then(|r| r["integrations_required"].as_array())
        .map(|v| v.iter().filter_map(|i| i["name"].as_str()).collect::<Vec<_>>().join(", "))
        .unwrap_or_else(|| "standard integrations".to_string());

    // --- 2. Haiku writes the client-facing justification ---

    let justification_prompt = format!(
        "2-3 sentences. Professional, no filler. Mention integrations. Focus on time saved / errors eliminated.\n\
Include both fees naturally in the last sentence.\n\
Project: {need}\nComplexity: {complexity}\nIntegrations: {integrations}\n\
Setup fee: {setup}€ (one-time) | Monthly: {monthly}€/mo (maintenance + monitoring + hosting)",
        need        = ctx.client_need,
        complexity  = complexity,
        integrations = integrations_str,
        setup       = setup_price,
        monthly     = monthly_price,
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
            model: "claude-haiku-4-5-20251001",
            max_tokens: 200,
            messages: vec![Msg { role: "user", content: justification_prompt }],
        })
        .send()
        .await
        .map_err(|e| AgentError(format!("pricing justification request: {e}")))?;

    let justification = if resp.status().is_success() {
        let ant: Resp = resp.json().await
            .map_err(|e| AgentError(format!("pricing justification parse: {e}")))?;
        ant.content.into_iter()
            .find(|c| c.kind == "text")
            .and_then(|c| c.text)
            .unwrap_or_default()
    } else {
        tracing::warn!("[pricing] justification call failed: {}", resp.status());
        format!("Automatisation {complexity} — {setup_price}€ (setup) + {monthly_price}€/mois.")
    };

    ctx.price_quote        = Some(setup_price);
    ctx.price_monthly      = Some(monthly_price);
    ctx.price_justification = Some(justification);
    Ok(())
}

/// Deploys the workflow to n8n (our instance or client's) and activates it.
pub async fn run_deploy(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[deploy] session={} target={}", ctx.session_id,
        ctx.deploy_target.as_deref().unwrap_or("own"));

    let workflow = ctx.workflow_json.as_ref()
        .ok_or_else(|| AgentError("deploy called with no workflow_json".to_string()))?;

    // Resolve n8n endpoint and API key based on deploy target
    let (n8n_url, n8n_key) = match ctx.deploy_target.as_deref().unwrap_or("own") {
        "client" => {
            let url = ctx.client_n8n_url.clone()
                .ok_or_else(|| AgentError("deploy_target=client but client_n8n_url is unset".to_string()))?;
            let key = ctx.client_n8n_key.clone()
                .ok_or_else(|| AgentError("deploy_target=client but client_n8n_key is unset".to_string()))?;
            (url, key)
        }
        _ => {
            let url = std::env::var("N8N_URL")
                .unwrap_or_else(|_| "http://localhost:5678".to_string());
            let key = std::env::var("N8N_API_KEY")
                .map_err(|_| AgentError("N8N_API_KEY not set".to_string()))?;
            (url, key)
        }
    };

    // n8n expects this top-level shape when creating a workflow
    let mut body = workflow.clone();
    let obj = body.as_object_mut()
        .ok_or_else(|| AgentError("workflow_json is not a JSON object".to_string()))?;

    // Ensure required top-level fields are present
    obj.entry("settings").or_insert_with(|| serde_json::json!({ "executionOrder": "v1" }));
    obj.entry("staticData").or_insert(serde_json::Value::Null);

    // n8n requires position {x, y} on every node — inject a simple top-down layout if missing
    if let Some(nodes) = obj.get_mut("nodes").and_then(|n| n.as_array_mut()) {
        for (i, node) in nodes.iter_mut().enumerate() {
            if let Some(obj) = node.as_object_mut() {
                obj.entry("position").or_insert_with(|| {
                    serde_json::json!({ "x": 250, "y": i as i64 * 200 })
                });
            }
        }
    }

    // Use client need as workflow name if the builder didn't set one
    obj.entry("name").or_insert_with(|| {
        serde_json::Value::String(format!(
            "pointe.dev — {}",
            ctx.client_need.chars().take(60).collect::<String>()
        ))
    });

    // --- 1. Create the workflow ---
    #[derive(serde::Deserialize)]
    struct CreateResp { id: String }

    let create_resp = app.http
        .post(format!("{n8n_url}/api/v1/workflows"))
        .header("X-N8N-API-KEY", &n8n_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| AgentError(format!("n8n create request: {e}")))?;

    if !create_resp.status().is_success() {
        let s = create_resp.status();
        let b = create_resp.text().await.unwrap_or_default();
        return Err(AgentError(format!("n8n create {s}: {b}")));
    }

    let created: CreateResp = create_resp.json().await
        .map_err(|e| AgentError(format!("n8n create parse: {e}")))?;

    tracing::info!("[deploy] workflow created id={}", created.id);

    // --- 2. Activate the workflow ---
    let activate_resp = app.http
        .post(format!("{n8n_url}/api/v1/workflows/{}/activate", created.id))
        .header("X-N8N-API-KEY", &n8n_key)
        .send()
        .await
        .map_err(|e| AgentError(format!("n8n activate request: {e}")))?;

    if !activate_resp.status().is_success() {
        // Activation failure is non-fatal — workflow exists, client can activate manually
        tracing::warn!(
            "[deploy] workflow {} created but activation failed: {}",
            created.id,
            activate_resp.status()
        );
    } else {
        tracing::info!("[deploy] workflow {} activated", created.id);
    }

    ctx.n8n_workflow_id  = Some(created.id.clone());
    ctx.n8n_workflow_url = Some(format!("{n8n_url}/workflow/{}", created.id));
    Ok(())
}
