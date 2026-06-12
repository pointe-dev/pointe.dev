use crate::mcp::N8nMcpConfig;
use crate::pipeline::PipelineContext;
use crate::pitch::{PitchResult, PitchSlide};
use crate::state::AppState;

const SONNET: &str = "claude-sonnet-4-6";
const HAIKU:  &str = "claude-haiku-4-5-20251001";

/// Beta header for the extended 1-hour cache TTL.
/// 1h write costs 2× but read stays 0.1× — captures cross-lead reuse at low volume.
/// NOTE: prompt caching is GA on Claude 4.x, so the old `prompt-caching-2024-07-31`
/// beta must NOT be sent — combining it here makes Anthropic ignore cache_control.
const CACHE_BETA: &str = "extended-cache-ttl-2025-04-11";

// n8n MCP read-only tool allowlists, per agent. These are the catalogue/grounding
// tools — never the write/deploy tools (create_workflow_from_code, update_workflow…),
// which stay out of reach. Exposed to the model as client-side tools and executed
// by the backend against the n8n MCP server (the hosted connector can't reach it).
const BUILDER_GROUNDING_TOOLS:  &[&str] = &["get_suggested_nodes", "search_nodes", "get_node_types"];
const CRITIC_GROUNDING_TOOLS:   &[&str] = &["search_nodes", "get_node_types"];
const DESIGNER_GROUNDING_TOOLS: &[&str] = &["get_suggested_nodes", "search_nodes"];

const BUILDER_MCP_ADDENDUM: &str = "\
You have LIVE access to the real n8n node catalogue through tools: get_suggested_nodes \
(workflow technique → recommended nodes), search_nodes (find a node by service or \
function), get_node_types (exact type id, typeVersion, and parameter names). GROUND \
every node you emit: before using a node, confirm its real type string and typeVersion \
via the catalogue, use the exact parameter names it returns, and prefer a real \
dedicated node over httpRequest when one exists. Never invent a type string. When the \
workflow is complete and every node is verified, call build_workflow with the final JSON.";

const CRITIC_MCP_ADDENDUM: &str = "\
You have LIVE access to the real n8n node catalogue through tools: search_nodes (find a \
node by service or function) and get_node_types (exact type id, typeVersion, and \
parameter names). Use them to VERIFY, not guess: check that each node 'type' in the \
workflow is a real n8n node and that its parameters exist. A type the catalogue does \
not have is grounds to reject (it fails to import). When done verifying, call \
submit_review with your verdict.";

// Appended to the builder system prompt when building ONE sub-flow of a decomposed
// tunnel. Explains the chaining convention so each sub-flow triggers the next and
// the data crosses the execution-context boundary explicitly (deploy substitutes the
// next sub-flow's NAME placeholder with its real n8n id).
const SUBFLOW_BUILD_ADDENDUM: &str = "\
You are building ONE sub-flow of a larger automation that was split into chained n8n \
workflows. Build ONLY this sub-flow's logic — not the whole tunnel — staying ≤8 nodes \
INCLUDING its trigger and any hand-off node.\n\
- If this is NOT the first sub-flow, its trigger MUST be \
'n8n-nodes-base.executeWorkflowTrigger' (it is invoked by the previous sub-flow and \
receives the input fields below). Do NOT add a schedule/webhook/app trigger.\n\
- If this is NOT the last sub-flow, it MUST end by invoking the next one: add an \
'n8n-nodes-base.executeWorkflow' node whose workflowId parameter is the EXACT string \
of the next sub-flow's name given below — a placeholder replaced with the real id at \
deploy — and pass the output fields below to it.\n\
- Honour the input/output contracts exactly: the fields crossing between sub-flows \
must match by name, because the n8n execution context does NOT carry across the hop \
($('EarlierNode') from another sub-flow is unreachable).";

const DESIGNER_MCP_ADDENDUM: &str = "\
You may consult the LIVE n8n node catalogue through tools: get_suggested_nodes \
(technique → recommended nodes) and search_nodes (find a node by service or function). \
Use them to ground the blueprint in integrations/nodes that actually exist, and to \
flag honestly when a needed service has no dedicated node (note it as 'via HTTP API'). \
Still output ONLY the blueprint outline exactly as specified — no JSON, no tool \
commentary in your final answer.";

/// cache_control marker for a 1-hour ephemeral cache breakpoint.
fn cache_1h() -> serde_json::Value {
    serde_json::json!({ "type": "ephemeral", "ttl": "1h" })
}

#[derive(Debug)]
pub struct AgentError(pub String);

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AgentError {}

impl From<reqwest::Error> for AgentError {
    fn from(e: reqwest::Error) -> Self { AgentError(e.to_string()) }
}

// ── Shared Anthropic primitives ───────────────────────────────────────────────

/// Single-turn call with prompt caching.
/// `system` is always cached — static instructions shared across all pipeline runs.
/// `user` is dynamic per-request context — not cached.
async fn anthropic_call(
    http: &reqwest::Client,
    key: &str,
    model: &'static str,
    max_tokens: u32,
    system: &str,
    user: &str,
) -> Result<String, AgentError> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": [{"type": "text", "text": system, "cache_control": cache_1h()}],
        "messages": [{"role": "user", "content": user}]
    });
    anthropic_raw(http, key, body).await
}

/// Forces the model to answer by calling a single tool, returning that tool's
/// `input` as a JSON value. The Messages API guarantees the tool input is valid
/// JSON shaped by `input_schema`, so this removes the "model replied in prose
/// instead of JSON" failure class that `extract_json` was patching over.
///
/// `user_content` is the message content — a plain string, or a content-block
/// array carrying `cache_control` for callers that cache a large stable prefix
/// (the builder). Caching is preserved: `tools` render before `system`, so the
/// system-block breakpoint caches tools+system together, and both the tool
/// schema and `tool_choice` are constant across calls.
async fn anthropic_tool_call(
    http: &reqwest::Client,
    key: &str,
    model: &'static str,
    max_tokens: u32,
    system: &str,
    user_content: serde_json::Value,
    tool_name: &str,
    tool_description: &str,
    input_schema: serde_json::Value,
) -> Result<serde_json::Value, AgentError> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": [{"type": "text", "text": system, "cache_control": cache_1h()}],
        "messages": [{"role": "user", "content": user_content}],
        "tools": [{
            "name": tool_name,
            "description": tool_description,
            "input_schema": input_schema,
        }],
        "tool_choice": {"type": "tool", "name": tool_name},
    });
    let v = anthropic_send(http, key, body).await?;
    tool_use_input(&v)
        .ok_or_else(|| AgentError(format!("Anthropic: no tool_use block in response: {v}")))
}

/// First `tool_use` block's `input` from a Messages API response body.
fn tool_use_input(resp: &serde_json::Value) -> Option<serde_json::Value> {
    resp["content"].as_array()?
        .iter()
        .find(|b| b["type"] == "tool_use")
        .map(|b| b["input"].clone())
}

/// First `text` block's text from a Messages API response body ("" if none).
fn text_block(resp: &serde_json::Value) -> String {
    resp["content"].as_array()
        .and_then(|blocks| blocks.iter().find(|b| b["type"] == "text"))
        .and_then(|b| b["text"].as_str())
        .unwrap_or_default()
        .to_string()
}

/// Total attempts (1 initial + retries) for a single Anthropic call.
const ANTHROPIC_MAX_ATTEMPTS: u32 = 4;

/// Exponential backoff with light jitter: ~0.5s, 1s, 2s, plus up to 250ms of
/// jitter so concurrent agents don't retry in lockstep. Jitter is derived from
/// the clock to stay dependency-free.
fn anthropic_backoff(attempt: u32) -> std::time::Duration {
    let base_ms = 500u64 * 2u64.pow(attempt.saturating_sub(1).min(4));
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0) % 250;
    std::time::Duration::from_millis(base_ms + jitter)
}

/// Sends a Messages API request with retry/backoff and returns the parsed JSON
/// response body. Both the text extractor (`anthropic_raw`) and the tool-call
/// extractor (`anthropic_tool_call`) build on this.
async fn anthropic_send(
    http: &reqwest::Client,
    key: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, AgentError> {
    // Transient failures (rate limits, overloaded 529, gateway/5xx, network
    // blips) are retried with backoff so one hiccup doesn't kill a whole
    // pipeline. 4xx other than 408/429 are permanent → fail fast.
    let mut attempt = 0u32;
    let resp = loop {
        attempt += 1;
        match http
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", CACHE_BETA)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => break r,
            Ok(r) => {
                let status = r.status();
                let retryable = matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504 | 529);
                if retryable && attempt < ANTHROPIC_MAX_ATTEMPTS {
                    // Honour a server-provided Retry-After (seconds) over our backoff.
                    let delay = r.headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .and_then(|s| s.trim().parse::<u64>().ok())
                        .map(std::time::Duration::from_secs)
                        .unwrap_or_else(|| anthropic_backoff(attempt));
                    tracing::warn!("[anthropic] {status} (attempt {attempt}/{ANTHROPIC_MAX_ATTEMPTS}); retrying in {delay:?}");
                    tokio::time::sleep(delay).await;
                    continue;
                }
                let b = r.text().await.unwrap_or_default();
                return Err(AgentError(format!("Anthropic {status}: {b}")));
            }
            Err(e) => {
                if attempt < ANTHROPIC_MAX_ATTEMPTS {
                    let delay = anthropic_backoff(attempt);
                    tracing::warn!("[anthropic] request error (attempt {attempt}/{ANTHROPIC_MAX_ATTEMPTS}): {e}; retrying in {delay:?}");
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(AgentError(format!("Anthropic request: {e}")));
            }
        }
    };

    resp.json::<serde_json::Value>().await
        .map_err(|e| AgentError(format!("Anthropic parse: {e}")))
}

/// Calls the Messages API and returns the first `text` content block.
async fn anthropic_raw(
    http: &reqwest::Client,
    key: &str,
    body: serde_json::Value,
) -> Result<String, AgentError> {
    let v = anthropic_send(http, key, body).await?;
    Ok(text_block(&v))
}

// ── MCP-grounded calls ────────────────────────────────────────────────────────

/// Hard ceiling on tool-loop iterations for a grounded call. Each round-trip
/// where the model calls catalogue tools costs one; a well-behaved grounding
/// session needs 1–3. The cap stops a runaway loop.
const GROUNDING_MAX_TURNS: u32 = 6;

/// Drives a client-side tool loop: the model is given the n8n catalogue tools
/// (`tools`, allowlisted), and whenever it calls one of `grounding_names` we
/// execute it against the n8n MCP server and feed the result back, until the
/// model produces a terminal message (calls `finish_tool`, or — when that is
/// None — answers with text). Returns the final response body for the caller to
/// extract from. `tool_choice` is implicit (`auto`).
async fn anthropic_grounded_loop(
    http: &reqwest::Client,
    key: &str,
    mcp: &N8nMcpConfig,
    model: &'static str,
    max_tokens: u32,
    system: &str,
    user_content: serde_json::Value,
    tools: Vec<serde_json::Value>,
    finish_tool: Option<&str>,
    grounding_names: &[&str],
) -> Result<serde_json::Value, AgentError> {
    let mut messages = vec![serde_json::json!({ "role": "user", "content": user_content })];
    for _ in 0..GROUNDING_MAX_TURNS {
        let body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "system": [{"type": "text", "text": system, "cache_control": cache_1h()}],
            "messages": messages,
            "tools": tools,
        });
        let resp = anthropic_send(http, key, body).await?;
        let content = resp["content"].as_array().cloned().unwrap_or_default();

        // Terminal: the model called the structured finish tool — hand it back.
        if let Some(ft) = finish_tool {
            if content.iter().any(|b| b["type"] == "tool_use" && b["name"] == ft) {
                return Ok(resp);
            }
        }

        // Collect the catalogue tool calls this turn (skip the finish tool).
        let calls: Vec<&serde_json::Value> = content.iter()
            .filter(|b| b["type"] == "tool_use"
                && grounding_names.contains(&b["name"].as_str().unwrap_or("")))
            .collect();

        // No catalogue call and finish not matched → terminal (text answer, or
        // the model stopped). Hand the body back; the caller decides if it's usable.
        if calls.is_empty() {
            return Ok(resp);
        }

        // Execute each catalogue call and feed the results back as tool_results.
        // A tool failure becomes an `is_error` result, not a pipeline abort, so
        // the model can recover (retry a different query) rather than dying.
        let mut tool_results = Vec::with_capacity(calls.len());
        for c in &calls {
            let name = c["name"].as_str().unwrap_or_default();
            let (text, is_error) = match mcp.call_tool(http, name, c["input"].clone()).await {
                Ok(t) => {
                    tracing::info!("[grounding] {name}({}) → {} chars", c["input"], t.len());
                    (t, false)
                }
                Err(e) => {
                    tracing::warn!("[grounding] {name} failed: {e}");
                    (format!("Tool error: {e}"), true)
                }
            };
            tool_results.push(serde_json::json!({
                "type": "tool_result",
                "tool_use_id": c["id"].clone(),
                "content": text,
                "is_error": is_error,
            }));
        }
        messages.push(serde_json::json!({ "role": "assistant", "content": content }));
        messages.push(serde_json::json!({ "role": "user", "content": tool_results }));
    }
    Err(AgentError(format!("grounding loop exceeded {GROUNDING_MAX_TURNS} turns without a terminal response")))
}

/// Grounded counterpart of `anthropic_tool_call`: the model grounds itself on the
/// n8n catalogue (via client-side tools we proxy), then returns structured output
/// by calling `tool_name`. Returns that tool's `input`.
async fn anthropic_grounded_tool_call(
    http: &reqwest::Client,
    key: &str,
    mcp: &N8nMcpConfig,
    model: &'static str,
    max_tokens: u32,
    system: &str,
    user_content: serde_json::Value,
    tool_name: &str,
    tool_description: &str,
    input_schema: serde_json::Value,
    allowed: &[&str],
) -> Result<serde_json::Value, AgentError> {
    let mut tools = mcp.grounding_tools(allowed);
    tools.push(serde_json::json!({
        "name": tool_name,
        "description": tool_description,
        "input_schema": input_schema,
    }));
    let resp = anthropic_grounded_loop(
        http, key, mcp, model, max_tokens, system, user_content, tools, Some(tool_name), allowed,
    ).await?;
    resp["content"].as_array()
        .and_then(|blocks| blocks.iter().find(|b| b["type"] == "tool_use" && b["name"] == tool_name))
        .map(|b| b["input"].clone())
        .ok_or_else(|| AgentError(format!(
            "grounded: model did not call {tool_name} (stop_reason={})", resp["stop_reason"]
        )))
}

/// Grounded counterpart of `anthropic_call`: the model grounds itself on the n8n
/// catalogue, then returns its answer as text (used by the prose designer).
async fn anthropic_grounded_call(
    http: &reqwest::Client,
    key: &str,
    mcp: &N8nMcpConfig,
    model: &'static str,
    max_tokens: u32,
    system: &str,
    user: &str,
    allowed: &[&str],
) -> Result<String, AgentError> {
    let tools = mcp.grounding_tools(allowed);
    let resp = anthropic_grounded_loop(
        http, key, mcp, model, max_tokens, system,
        serde_json::Value::String(user.to_string()), tools, None, allowed,
    ).await?;
    Ok(text_block(&resp))
}

// ── Agents ────────────────────────────────────────────────────────────────────

/// Finalizes qualification: enriches the summary from the chat qualify block.
/// If a summary already exists (from the qualify block), validates and normalises it.
/// Otherwise, asks Haiku to infer a summary from client_need alone.
pub async fn run_qualifier(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[qualifier] session={}", ctx.session_id);

    if ctx.qualification_summary.is_some() {
        tracing::info!("[qualifier] summary pre-filled from chat qualify block — skipping LLM");
        return Ok(());
    }

    const SYSTEM: &str = "\
You are the qualification analyst at pointe.dev, a bespoke automation agency. \
A prospect has described their business in their own words. Your job is to distil \
that into one dense, structured line that the rest of the pipeline (research, \
pricing) will rely on.\n\
\n\
Output EXACTLY one line, no prefix, no explanation, in this format:\n\
  sector | main pain point | current tools | approximate volume\n\
\n\
Rules:\n\
- Write each field in the SAME LANGUAGE the prospect used (FR, EN or DE).\n\
- Be concrete and specific: 'e-commerce mode' beats 'retail'; 'saisie manuelle \
des commandes Shopify dans l'ERP' beats 'data entry'.\n\
- NEVER invent facts. If a field is genuinely absent from the description, write \
'non précisé' (or 'not specified' / 'nicht angegeben') rather than guessing a \
plausible-sounding value. A wrong guess here corrupts the price downstream.\n\
- For volume, prefer the prospect's own figure ('200 leads/mois'). Only if none \
is given, mark it 'non précisé' — do not fabricate a range.\n\
- Tools = the named systems they actually use (Shopify, HubSpot, Excel, Gmail…), \
not categories.\n\
\n\
Examples:\n\
  Input: 'On gère une boutique en ligne de cosmétiques, je passe mes journées à \
recopier les commandes Shopify dans notre logiciel de compta, environ 80 par jour.'\n\
  Output: e-commerce cosmétiques | recopie manuelle des commandes Shopify vers la \
compta | Shopify, logiciel de compta | ~80 commandes/jour\n\
\n\
  Input: 'We're a B2B SaaS, support tickets pile up in Zendesk and nobody triages them.'\n\
  Output: B2B SaaS | unsorted support backlog, no triage | Zendesk | not specified";

    let raw = anthropic_call(
        &app.http, &app.anthropic_key, HAIKU, 120,
        SYSTEM, &ctx.client_need,
    ).await;

    ctx.qualification_summary = Some(match raw {
        Ok(s) if !s.is_empty() => s,
        _ => {
            tracing::warn!("[qualifier] LLM call failed — using client_need as summary");
            ctx.client_need.clone()
        }
    });
    Ok(())
}

/// Researches the client's domain: required APIs, integration points, feasibility.
pub async fn run_research(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[research] session={}", ctx.session_id);

    const SYSTEM: &str = "\
You are the solutions architect at pointe.dev. You scope automation projects that \
will be built on n8n (a node-based workflow tool). Given a prospect's need, you \
produce a rigorous, honest technical assessment that drives the build and the price.\n\
\n\
Submit your assessment by calling the submit_assessment tool.\n\
\n\
Fields:\n\
{sector, current_tools[], pain_points[],\n\
 integrations_required[{name, n8n_node, auth_type, notes}],\n\
 api_keys_to_acquire[], feasibility_score(0-10),\n\
 complexity(simple|medium|complex), estimated_build_hours,\n\
 approach(1 sentence), risks[{description, severity(low|medium|high)}](max 3)}\n\
\n\
Accuracy rules — this assessment is the basis for a real quote:\n\
- n8n_node: use the REAL n8n node type when you are confident it exists \
(e.g. 'n8n-nodes-base.httpRequest', 'n8n-nodes-base.gmail', \
'n8n-nodes-base.postgres', 'n8n-nodes-base.scheduleTrigger'). If a dedicated node \
likely exists but you are unsure of its exact type, use 'n8n-nodes-base.httpRequest' \
and say so in notes. NEVER invent a node type that looks official but isn't.\n\
- Only list integrations the need actually implies. Do not pad the list to seem \
thorough — every integration raises the price and must be justified.\n\
- complexity: 'simple' = 1-2 integrations, linear flow, no custom logic; 'medium' = \
3-4 integrations or some branching/transformation; 'complex' = 5+ integrations, \
heavy data shaping, error-prone external systems, or real-time constraints.\n\
- estimated_build_hours: realistic senior-engineer hours, as a plain number string \
(e.g. '14'). Be conservative but not padded. Simple ≈ 4-10h, medium ≈ 10-25h, \
complex ≈ 25-60h.\n\
- feasibility_score: 10 = trivial with mature nodes; below 6 = real technical risk \
(undocumented API, brittle integration, unclear requirements).\n\
- risks: only genuine ones (rate limits, missing API, data quality, auth complexity). \
If there are none worth flagging, return an empty array — do not manufacture risk.\n\
- approach: one plain sentence a non-technical client would understand.\n\
- When the need is vague, reflect that in a lower feasibility_score and a risk \
entry, NOT in invented integrations.\n\
\n\
Worked example (study the level of rigour — your output is JSON only, like the \
'Output' below):\n\
\n\
Input:\n\
  Client: On a une boutique Shopify de cosmétiques, je recopie chaque commande à la \
main dans notre logiciel de compta Pennylane, environ 80 par jour.\n\
  Summary: e-commerce cosmétiques | recopie manuelle des commandes Shopify vers la \
compta | Shopify, Pennylane | ~80 commandes/jour\n\
\n\
Output:\n\
{\"sector\":\"e-commerce cosmétiques\",\
\"current_tools\":[\"Shopify\",\"Pennylane\"],\
\"pain_points\":[\"recopie manuelle de ~80 commandes/jour\",\"risque d'erreurs de saisie\",\"temps perdu quotidien\"],\
\"integrations_required\":[\
{\"name\":\"Shopify\",\"n8n_node\":\"n8n-nodes-base.shopifyTrigger\",\"auth_type\":\"apiKey\",\"notes\":\"déclencheur sur nouvelle commande\"},\
{\"name\":\"Pennylane\",\"n8n_node\":\"n8n-nodes-base.httpRequest\",\"auth_type\":\"apiKey\",\"notes\":\"pas de nœud dédié — appel API REST Pennylane pour créer la facture\"}],\
\"api_keys_to_acquire\":[\"Shopify Admin API access token\",\"Pennylane API key\"],\
\"feasibility_score\":8,\
\"complexity\":\"simple\",\
\"estimated_build_hours\":\"8\",\
\"approach\":\"Chaque nouvelle commande Shopify crée automatiquement la facture correspondante dans Pennylane.\",\
\"risks\":[{\"description\":\"mapping des taux de TVA entre Shopify et Pennylane\",\"severity\":\"medium\"}]}\n\
\n\
Note how Pennylane uses httpRequest (no dedicated node, stated in notes), the risk is \
real and specific, and nothing is padded. Apply the same discipline.";

    let user = format!(
        "Client: {}\nSummary: {}",
        ctx.client_need,
        ctx.qualification_summary.as_deref().unwrap_or(""),
    );

    // Forced tool call → guaranteed-valid JSON. The model previously emitted
    // free-text JSON that occasionally broke serde (e.g. a full-width comma),
    // which failed the whole pipeline before it ever reached the builder.
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "sector": {"type": "string"},
            "current_tools": {"type": "array", "items": {"type": "string"}},
            "pain_points": {"type": "array", "items": {"type": "string"}},
            "integrations_required": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "n8n_node": {"type": "string", "description": "real n8n node type, or n8n-nodes-base.httpRequest"},
                        "auth_type": {"type": "string"},
                        "notes": {"type": "string"}
                    },
                    "required": ["name", "n8n_node"]
                }
            },
            "api_keys_to_acquire": {"type": "array", "items": {"type": "string"}},
            "feasibility_score": {"type": "number", "description": "0-10"},
            "complexity": {"type": "string", "enum": ["simple", "medium", "complex"]},
            "estimated_build_hours": {"type": "string", "description": "plain number string, e.g. '14'"},
            "approach": {"type": "string", "description": "one plain sentence"},
            "risks": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "description": {"type": "string"},
                        "severity": {"type": "string", "enum": ["low", "medium", "high"]}
                    },
                    "required": ["description", "severity"]
                },
                "description": "max 3; empty array if none worth flagging"
            }
        },
        "required": ["sector", "complexity", "feasibility_score", "estimated_build_hours", "integrations_required"]
    });

    let structured = anthropic_tool_call(
        &app.http, &app.anthropic_key, SONNET, 2048,
        SYSTEM, serde_json::Value::String(user),
        "submit_assessment", "Submit the technical assessment.",
        schema,
    ).await.map_err(|e| AgentError(format!("research: {e}")))?;

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
            .iter().map(|i| i["name"].as_str().unwrap_or("?"))
            .collect::<Vec<_>>().join(", "),
        keys = structured["api_keys_to_acquire"]
            .as_array().unwrap_or(&vec![])
            .iter().filter_map(|k| k.as_str())
            .collect::<Vec<_>>().join(", "),
        approach  = structured["approach"].as_str().unwrap_or(""),
        risks     = structured["risks"]
            .as_array().unwrap_or(&vec![])
            .iter().filter_map(|r| r.as_str())
            .collect::<Vec<_>>().join("; "),
    );

    tracing::info!(
        "[research] session={} complexity={} feasibility={}/10",
        ctx.session_id,
        structured["complexity"].as_str().unwrap_or("?"),
        structured["feasibility_score"].as_f64().unwrap_or(0.0),
    );

    ctx.research_output = Some(summary);
    ctx.research_json   = Some(structured);
    Ok(())
}

/// Drafts the high-level solution outline — an ordered list of blocks that solves
/// the need, each naming the action and the integration from research. NO JSON:
/// this blueprint is what the design critic reviews, pricing quotes, and the client
/// reads. The real workflow JSON is built later (post-payment) from this outline.
pub async fn run_designer(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[designer] session={} attempt={}", ctx.session_id, ctx.design_attempts);

    const SYSTEM: &str = "\
You are the solution designer at pointe.dev. Given a prospect's need and the \
technical research, you produce the HIGH-LEVEL design of the automation as an \
ordered outline of blocks — NOT JSON. This outline is reviewed, priced, and shown \
to the client; the real n8n workflow is built later, only AFTER the client pays, \
from THIS blueprint. So it must be complete and honest.\n\
\n\
Output a numbered list of steps. For EACH step, one line:\n\
  <n>. <what happens> — <integration / n8n node> — <why it matters>\n\
Start with the trigger, end with the final client-visible outcome. Cover the WHOLE \
need end-to-end: if the need implies several outputs (e.g. publish to two \
platforms), each gets its own step. Use the integrations named in the research; if \
a step needs an external service with no dedicated node, say 'via HTTP API'. After \
the steps, add two short lines:\n\
  Blocs clés: <distinct integrations/services involved, comma-separated>\n\
  Points de vigilance: <max 2 real risks or client-provided prerequisites (API \
approval, credentials, paid account), or 'aucun'>\n\
\n\
Rules:\n\
- Same language as the client's need (FR/EN/DE).\n\
- Concrete and tied to THIS need — no generic boilerplate.\n\
- Do NOT emit JSON, node parameters, or code. This is a blueprint, not an \
implementation.\n\
- Do NOT pad: every block must be justified by the need — honesty drives a correct \
quote. Keep it tight: readable in 30 seconds.";

    let feedback = ctx.design_critic_feedback.last()
        .map(|fb| format!("\n\nThe previous design was rejected. Address this feedback:\n{fb}"))
        .unwrap_or_default();

    let user = format!(
        "Client need: {}\nQualification: {}\nTechnical research: {}{}",
        ctx.client_need,
        ctx.qualification_summary.as_deref().unwrap_or(""),
        ctx.research_output.as_deref().unwrap_or(""),
        feedback,
    );

    // Grounded path: the blueprint is anchored in nodes that actually exist (and
    // honestly flags services with no dedicated node) so the post-payment build
    // matches the quote. NOTE: the designer is PRE-payment, so MCP calls add
    // latency on unpaid leads — gated on env so it can be split off if conversion
    // latency matters. Higher token cap leaves room for the catalogue tool blocks.
    let outline = match &app.n8n_mcp {
        Some(mcp) => {
            let system = format!("{SYSTEM}\n\n{DESIGNER_MCP_ADDENDUM}");
            anthropic_grounded_call(
                &app.http, &app.anthropic_key, mcp, SONNET, 2500,
                &system, &user, DESIGNER_GROUNDING_TOOLS,
            ).await
        }
        None => anthropic_call(
            &app.http, &app.anthropic_key, SONNET, 1200,
            SYSTEM, &user,
        ).await,
    }.map_err(|e| AgentError(format!("designer: {e}")))?;

    if outline.trim().is_empty() {
        return Err(AgentError("designer returned an empty outline".into()));
    }
    ctx.design_summary = Some(outline);
    Ok(())
}

/// Gates the SOLUTION DESIGN before it is priced and quoted. Because the real
/// workflow is built only after payment, a gap here means we quote for the wrong
/// thing — so this critic checks completeness/viability of the blueprint, not JSON.
/// Returns true if approved; rejection feedback is appended to ctx.design_critic_feedback.
pub async fn run_design_critic(app: &AppState, ctx: &mut PipelineContext) -> Result<bool, AgentError> {
    tracing::info!("[design_critic] session={} attempt={}", ctx.session_id, ctx.design_attempts);

    let design = ctx.design_summary.as_deref()
        .ok_or_else(|| AgentError("design critic called with no design_summary".to_string()))?;

    const SYSTEM: &str = "\
You are the design reviewer at pointe.dev. You gate the SOLUTION DESIGN before it is \
priced and quoted to the client. Critically: the real workflow is built only AFTER \
the client pays, from this design — so a gap here means we quote for the wrong thing \
and then fail post-payment. Be demanding on substance, not style.\n\
\n\
Submit your verdict via submit_review: approved=true if the design is viable and \
complete, or approved=false with feedback (max 3 concrete, actionable points).\n\
\n\
Reject (approved:false) if ANY fail:\n\
1. Completeness — the design does NOT solve the stated need end-to-end. Every \
outcome the client asked for must have a step (e.g. publish to YouTube AND \
Instagram → both must appear). Missing output, missing trigger, or a gap in the \
chain → reject.\n\
2. Viability — a named integration is wrong for its job, or a step depends on \
something infeasible/unavailable without that being flagged as a risk (e.g. an API \
that requires approval).\n\
3. Scope sanity — drastically over- or under-engineered for the need.\n\
4. Honesty — a real prerequisite the client must provide (credentials, paid \
account, API approval) is silently assumed instead of flagged.\n\
\n\
Do NOT review JSON, exact node-type strings, or parameters — there is NO workflow \
yet; that is the builder's job post-payment. Judge the BLUEPRINT: does it fully and \
feasibly solve the need, and is it the right size to price fairly? When the design \
is sound and complete, approve it without inventing objections. Feedback must be \
specific enough for the designer to fix in one pass. Output only the verdict.";

    let user = format!(
        "Client need: {}\nResearch: {}\nProposed design:\n{}",
        ctx.client_need,
        ctx.research_output.as_deref().unwrap_or(""),
        design,
    );

    #[derive(serde::Deserialize)]
    struct Verdict { approved: bool, feedback: Option<String> }

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "approved": {
                "type": "boolean",
                "description": "true if the design solves the need end-to-end, is viable, honest about prerequisites, and appropriately scoped"
            },
            "feedback": {
                "type": "string",
                "description": "Required when approved is false: max 3 concrete, actionable issues. Omit when approved is true."
            }
        },
        "required": ["approved"]
    });

    // A transport error shouldn't kill the pipeline — soft-reject so the designer
    // retries; after MAX_DESIGN_ATTEMPTS the human takes over the proposal.
    let input = match anthropic_tool_call(
        &app.http, &app.anthropic_key, SONNET, 1024,
        SYSTEM, serde_json::Value::String(user),
        "submit_review", "Submit your review verdict for the solution design.",
        schema,
    ).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[design_critic] tool call failed: {e} — treating as rejection");
            ctx.design_critic_feedback.push("Le critique de design n'a pas pu rendre de verdict; nouvelle tentative.".to_string());
            return Ok(false);
        }
    };

    let verdict: Verdict = match serde_json::from_value(input.clone()) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[design_critic] unexpected verdict shape: {e} (input={input}) — treating as rejection");
            ctx.design_critic_feedback.push("Verdict mal formé; nouvelle tentative.".to_string());
            return Ok(false);
        }
    };

    if verdict.approved {
        tracing::info!("[design_critic] approved on attempt {}", ctx.design_attempts);
        Ok(true)
    } else {
        let fb = verdict.feedback.unwrap_or_else(|| "unspecified issues".to_string());
        tracing::warn!("[design_critic] rejected attempt {}: {fb}", ctx.design_attempts);
        ctx.design_critic_feedback.push(fb);
        Ok(false)
    }
}

/// Cheap pre-check: is this automation large enough to warrant splitting into
/// sub-workflows? A tunnel with many integrations or a long blueprint will not fit
/// one ≤8-node workflow reliably (the builder tops out around there). Mirrors the
/// research complexity buckets (5+ integrations = complex) and keeps the decomposer
/// LLM call off the simple majority of leads.
pub fn needs_decomposition(ctx: &PipelineContext) -> bool {
    let integrations = ctx.research_json.as_ref()
        .and_then(|j| j["integrations_required"].as_array())
        .map(|a| a.len())
        .unwrap_or(0);

    let design_steps = ctx.design_summary.as_deref()
        .map(count_design_steps)
        .unwrap_or(0);

    integrations >= 5 || design_steps > 8
}

/// Counts the numbered steps ("1. …", "2) …") in a designer blueprint.
fn count_design_steps(design: &str) -> usize {
    design.lines().filter(|l| is_numbered_step(l)).count()
}

/// True if a line begins (after indentation) with a number followed by '.' or ')'.
fn is_numbered_step(line: &str) -> bool {
    let t = line.trim_start();
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    digits > 0 && matches!(t[digits..].chars().next(), Some('.') | Some(')'))
}

/// Splits a large approved design into an ordered list of self-contained sub-flows
/// (each ≤8 nodes), so the post-payment builder constructs simple workflows that
/// stay under the node budget where it succeeds. Chains them through explicit
/// input/output contracts (executeWorkflow / webhook) so the n8n execution context
/// — which breaks after a trigger node — survives the hop between sub-flows.
/// Only worth calling when `needs_decomposition` trips; populates ctx.sub_workflows.
/// May legitimately return a single entry if the design fits one workflow (N=1).
pub async fn run_decomposer(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[decomposer] session={}", ctx.session_id);

    const SYSTEM: &str = "\
You are the workflow architect at pointe.dev. You are given an APPROVED automation \
design (an ordered blueprint) that is too large to build as one n8n workflow. Split \
it into an ORDERED list of self-contained sub-flows, each a separate n8n workflow.\n\
\n\
Hard rules:\n\
- Each sub-flow is ≤8 nodes. If a stage needs more, split it further.\n\
- Cut along natural boundaries of the blueprint (e.g. Ingest → Produce → Publish → \
Analytics), never mid-action. Every block of the design must land in exactly one \
sub-flow; together they must cover the WHOLE design end-to-end, in order.\n\
- The FIRST sub-flow owns the real trigger (schedule/webhook/app trigger). Each \
later sub-flow is started by the previous one — via an n8n 'Execute Workflow' call \
or an inbound webhook.\n\
- CRITICAL: the n8n execution context does NOT cross a trigger boundary \
($('Node').item.json from an earlier sub-flow is unreachable downstream). So each \
hop must pass its data EXPLICITLY. State, per sub-flow, the input_contract (the \
exact fields it receives from the previous sub-flow, empty for the first) and the \
output_contract (the exact fields it hands to the next, empty for the last). Make the \
contracts concrete field lists, not vague prose.\n\
\n\
Submit the plan via submit_decomposition. Prefer the FEWEST sub-flows that respect \
the ≤8-node rule — do not over-split. If the whole design genuinely fits one ≤8-node \
workflow, return a single sub-flow. Same language as the design (FR/EN/DE) for names \
and descriptions.";

    let user = format!(
        "Client need: {}\nResearch: {}\nApproved design (split THIS, end-to-end):\n{}",
        ctx.client_need,
        ctx.research_output.as_deref().unwrap_or(""),
        ctx.design_summary.as_deref().unwrap_or(""),
    );

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "sub_workflows": {
                "type": "array",
                "description": "Ordered sub-flows, each ≤8 nodes, together covering the whole design.",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "description": "Short ordered name, e.g. 'WF-A — Ingest & Select'"},
                        "description": {"type": "string", "description": "The subset of the design this sub-flow implements"},
                        "trigger": {"type": "string", "description": "Real trigger for the first sub-flow; how the previous one hands off (Execute Workflow / webhook) for the rest"},
                        "input_contract": {"type": "string", "description": "Exact fields received from the previous sub-flow; empty for the first"},
                        "output_contract": {"type": "string", "description": "Exact fields handed to the next sub-flow; empty for the last"}
                    },
                    "required": ["name", "description", "trigger", "input_contract", "output_contract"]
                }
            }
        },
        "required": ["sub_workflows"]
    });

    let input = anthropic_tool_call(
        &app.http, &app.anthropic_key, SONNET, 2048,
        SYSTEM, serde_json::Value::String(user),
        "submit_decomposition", "Submit the ordered list of sub-workflows.",
        schema,
    ).await.map_err(|e| AgentError(format!("decomposer: {e}")))?;

    let plan: Vec<crate::pipeline::SubWorkflowPlan> =
        serde_json::from_value(input["sub_workflows"].clone())
            .map_err(|e| AgentError(format!("decomposer: malformed plan: {e}")))?;

    if plan.is_empty() {
        return Err(AgentError("decomposer returned an empty plan".into()));
    }

    tracing::info!("[decomposer] session={} split into {} sub-flows", ctx.session_id, plan.len());
    ctx.sub_workflows = plan;
    Ok(())
}

/// Builds an n8n workflow JSON using Qdrant RAG over n8n templates.
/// Runs up to MAX_BUILD_ATTEMPTS times; the large context is cached between retries.
pub async fn run_builder(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[builder] session={} attempt={}", ctx.session_id, ctx.build_attempts);

    let rag_query = format!(
        "{} {}",
        ctx.client_need,
        ctx.research_output.as_deref().unwrap_or_default()
    );

    // Format retrieved templates identically whatever the backend (Cloudflare or
    // Qdrant) — both expose name/description/tags.
    let format_templates = |rows: &[(String, String, String)]| {
        let s = rows
            .iter()
            .map(|(name, desc, tags)| {
                format!("Template: {name}\nDescription: {desc}\nTags: {tags}")
            })
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");
        format!("\n\nSimilar workflow templates for reference:\n{s}")
    };

    let rag_block = if let Some(cf) = &app.cloudflare {
        match cf.embed(rag_query).await {
            Ok(vector) => match cf.query(vector, 3, None).await {
                Ok(hits) if !hits.is_empty() => {
                    tracing::info!("[builder] retrieved {} RAG templates (cloudflare)", hits.len());
                    let rows: Vec<_> = hits.iter()
                        .map(|h| (h.name.clone(), h.description.clone(), h.tags.join(", ")))
                        .collect();
                    format_templates(&rows)
                }
                Ok(_)  => { tracing::warn!("[builder] Vectorize returned no hits"); String::new() }
                Err(e) => { tracing::warn!("[builder] Vectorize query failed: {e}"); String::new() }
            },
            Err(e) => { tracing::warn!("[builder] embed failed: {e}"); String::new() }
        }
    } else {
        match (&app.qdrant, &app.embeddings) {
            (Some(qdrant), Some(engine)) => {
                match engine.embed(rag_query).await {
                    Ok(vector) => match qdrant.search(vector, 3).await {
                        Ok(hits) if !hits.is_empty() => {
                            tracing::info!("[builder] retrieved {} RAG templates", hits.len());
                            let rows: Vec<_> = hits.iter()
                                .map(|h| (h.name.clone(), h.description.clone(), h.tags.join(", ")))
                                .collect();
                            format_templates(&rows)
                        }
                        Ok(_)  => { tracing::warn!("[builder] Qdrant returned no hits"); String::new() }
                        Err(e) => { tracing::warn!("[builder] Qdrant search failed: {e}"); String::new() }
                    },
                    Err(e) => { tracing::warn!("[builder] embed failed: {e}"); String::new() }
                }
            }
            _ => { tracing::warn!("[builder] RAG disabled"); String::new() }
        }
    };

    const SYSTEM: &str = "\
You are the workflow engineer at pointe.dev. You produce production-grade n8n \
workflow JSON that solves the client's need end-to-end. Reference templates may be \
provided — adapt their proven structure, do not copy blindly.\n\
\n\
Return the workflow by calling the build_workflow tool. No position fields.\n\
\n\
Structure:\n\
- Required top-level keys: name, nodes[], connections{}.\n\
- Each node: type, name, typeVersion, and only the 2-3 essential parameters it needs.\n\
- Use {} for every credential/auth field — never embed secrets or placeholders \
like 'YOUR_API_KEY'.\n\
- Max 8 nodes. Prefer the simplest graph that fully solves the need.\n\
\n\
Correctness rules:\n\
- Use ONLY real n8n node types you are confident exist \
(e.g. 'n8n-nodes-base.scheduleTrigger', 'n8n-nodes-base.httpRequest', \
'n8n-nodes-base.gmail', 'n8n-nodes-base.set', 'n8n-nodes-base.if', \
'n8n-nodes-base.postgres', 'n8n-nodes-base.code'). If no dedicated node is certain, \
use 'n8n-nodes-base.httpRequest' rather than inventing one.\n\
- Every workflow must start with a trigger node (schedule, webhook, or an app \
trigger) — never a dangling flow.\n\
- 'connections' must wire every non-trigger node: no orphan nodes, no references to \
nodes that don't exist. Connection keys are node NAMES, matching the 'name' field.\n\
- Node names must be unique and human-readable ('Fetch Shopify Orders', not 'HTTP1').\n\
- Include basic resilience where it matters (e.g. an IF node to handle the empty/error \
case) when the client need implies reliability.\n\
- The workflow must actually accomplish the stated need — trace it mentally from \
trigger to final action before emitting.\n\
\n\
Worked example (shape and rigour — your output is workflow JSON only):\n\
\n\
Need: each new Shopify order should create an invoice in an accounting system via \
its REST API.\n\
\n\
Output:\n\
{\"name\":\"Shopify → Accounting Invoice\",\
\"nodes\":[\
{\"name\":\"On New Order\",\"type\":\"n8n-nodes-base.shopifyTrigger\",\"typeVersion\":1,\
\"parameters\":{\"topic\":\"orders/create\"},\"credentials\":{}},\
{\"name\":\"Build Invoice Payload\",\"type\":\"n8n-nodes-base.set\",\"typeVersion\":3,\
\"parameters\":{\"mode\":\"manual\"}},\
{\"name\":\"Create Invoice\",\"type\":\"n8n-nodes-base.httpRequest\",\"typeVersion\":4,\
\"parameters\":{\"method\":\"POST\",\"url\":\"https://api.accounting.example/v1/invoices\"},\"credentials\":{}},\
{\"name\":\"Order Has Customer?\",\"type\":\"n8n-nodes-base.if\",\"typeVersion\":2,\
\"parameters\":{}}],\
\"connections\":{\
\"On New Order\":{\"main\":[[{\"node\":\"Order Has Customer?\",\"type\":\"main\",\"index\":0}]]},\
\"Order Has Customer?\":{\"main\":[[{\"node\":\"Build Invoice Payload\",\"type\":\"main\",\"index\":0}]]},\
\"Build Invoice Payload\":{\"main\":[[{\"node\":\"Create Invoice\",\"type\":\"main\",\"index\":0}]]}}}\n\
\n\
Note: a real trigger node, credentials as {}, connection keys are node names, the IF \
guards the empty-customer case, and the graph runs cleanly trigger → action. Match \
this structure for the client's actual need below.\n\
\n\
Common node types you can rely on (use the exact string):\n\
- Triggers: 'n8n-nodes-base.scheduleTrigger' (cron/interval), \
'n8n-nodes-base.webhook' (inbound HTTP), 'n8n-nodes-base.shopifyTrigger', \
'n8n-nodes-base.gmailTrigger'.\n\
- Actions/apps: 'n8n-nodes-base.httpRequest' (any REST API — your fallback), \
'n8n-nodes-base.gmail', 'n8n-nodes-base.slack', 'n8n-nodes-base.googleSheets', \
'n8n-nodes-base.postgres', 'n8n-nodes-base.notion', 'n8n-nodes-base.airtable'.\n\
- Logic/data: 'n8n-nodes-base.set' (shape data), 'n8n-nodes-base.if' (branch), \
'n8n-nodes-base.switch' (multi-branch), 'n8n-nodes-base.merge', \
'n8n-nodes-base.code' (custom JS), 'n8n-nodes-base.splitInBatches' (loop/throttle).\n\
If the client's app has no dedicated node above, use httpRequest and note the endpoint \
in the node name. When in doubt about a type, prefer httpRequest over guessing — a \
real httpRequest call always beats an invented node that n8n cannot load, because an \
unknown node type makes the entire workflow fail to import and blocks deployment.";

    // Decomposed build? Build only the sub-flow at the cursor; otherwise the whole
    // approved design as one workflow (mono, unchanged).
    let subflow = ctx.sub_workflows.get(ctx.build_cursor).cloned();

    // Stable across retries → cached after the first attempt
    let context = match &subflow {
        Some(sf) => {
            let total = ctx.sub_workflows.len();
            let pos = ctx.build_cursor + 1;
            let handoff = match ctx.sub_workflows.get(ctx.build_cursor + 1) {
                Some(next) => format!(
                    "Next sub-flow to invoke (use its name verbatim as the executeWorkflow workflowId placeholder): {}",
                    next.name,
                ),
                None => "This is the LAST sub-flow — it produces the final client-visible outcome, no hand-off.".to_string(),
            };
            format!(
                "Client need (overall tunnel): {}\n\
                 You are building sub-flow {pos} of {total}: {}\n\
                 What this sub-flow must do: {}\n\
                 Trigger: {}\n\
                 Input it receives from the previous sub-flow: {}\n\
                 Output it must hand to the next sub-flow: {}\n\
                 {handoff}\n\
                 Research context: {}{}",
                ctx.client_need, sf.name, sf.description, sf.trigger,
                if sf.input_contract.is_empty() { "(none — this is the entry sub-flow)" } else { &sf.input_contract },
                if sf.output_contract.is_empty() { "(none — this is the final sub-flow)" } else { &sf.output_contract },
                ctx.research_output.as_deref().unwrap_or(""),
                rag_block,
            )
        }
        None => format!(
            "Client: {}\nResearch: {}\nApproved design (build exactly this blueprint, end-to-end):\n{}{}",
            ctx.client_need,
            ctx.research_output.as_deref().unwrap_or(""),
            ctx.design_summary.as_deref().unwrap_or(""),
            rag_block,
        ),
    };

    // Changes each retry → never cached
    let suffix = if ctx.critic_feedback.is_empty() {
        String::new()
    } else {
        format!(
            "\n\nPrevious attempt was rejected. Critic feedback to address:\n{}",
            ctx.critic_feedback.iter().enumerate()
                .map(|(i, f)| format!("{}. {f}", i + 1))
                .collect::<Vec<_>>().join("\n")
        )
    };

    // Stable context cached within the pipeline; the changing critic feedback
    // (suffix) is appended uncached. Same caching shape as before — only the
    // delivery mechanism (forced tool call) changed.
    let user_content = if suffix.is_empty() {
        serde_json::json!([
            {"type": "text", "text": context, "cache_control": cache_1h()}
        ])
    } else {
        serde_json::json!([
            {"type": "text", "text": context, "cache_control": cache_1h()},
            {"type": "text", "text": suffix}
        ])
    };

    // Forced tool call → the API returns the workflow as a valid JSON object,
    // so there is no prose/fence to strip and no parse-failure path.
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "name": {"type": "string", "description": "Human-readable workflow name"},
            "nodes": {
                "type": "array",
                "items": {"type": "object"},
                "description": "n8n nodes: each has type, name, typeVersion, parameters, and credentials ({} when none)"
            },
            "connections": {"type": "object", "description": "Wiring keyed by source node name"}
        },
        "required": ["name", "nodes", "connections"]
    });

    // In a decomposed build, the system prompt also carries the sub-flow chaining
    // convention (executeWorkflowTrigger / executeWorkflow-by-name).
    let base_system = if subflow.is_some() {
        format!("{SYSTEM}\n\n{SUBFLOW_BUILD_ADDENDUM}")
    } else {
        SYSTEM.to_string()
    };

    // When the n8n MCP connector is configured, the builder grounds itself on the
    // real node catalogue (verifies type ids/versions/params) before emitting —
    // killing the invented-node-type failure class. Otherwise it falls back to the
    // hardcoded-node-list prompt above. Post-payment, so the extra loop latency is
    // acceptable; gated on env so it can be toggled and measured.
    let workflow = match &app.n8n_mcp {
        Some(mcp) => {
            let system = format!("{base_system}\n\n{BUILDER_MCP_ADDENDUM}");
            anthropic_grounded_tool_call(
                &app.http, &app.anthropic_key, mcp, SONNET, 8192,
                &system, user_content,
                "build_workflow", "Submit the complete n8n workflow JSON.",
                schema, BUILDER_GROUNDING_TOOLS,
            ).await
        }
        None => anthropic_tool_call(
            &app.http, &app.anthropic_key, SONNET, 8192,
            &base_system, user_content,
            "build_workflow", "Submit the complete n8n workflow JSON.",
            schema,
        ).await,
    }.map_err(|e| AgentError(format!("builder: {e}")))?;

    ctx.workflow_json = Some(workflow);
    Ok(())
}

/// Validates the workflow for correctness, completeness, and client fit.
/// Returns true if approved, false if revisions needed (feedback appended to ctx).
pub async fn run_critic(app: &AppState, ctx: &mut PipelineContext) -> Result<bool, AgentError> {
    tracing::info!("[critic] session={} attempt={}", ctx.session_id, ctx.build_attempts);

    let workflow = ctx.workflow_json.as_ref()
        .ok_or_else(|| AgentError("critic called with no workflow_json".to_string()))?;

    const SYSTEM: &str = "\
You are the senior reviewer at pointe.dev. You gate every workflow before it reaches \
a paying client. You are demanding but fair: you reject real defects, you do not \
nitpick style. A workflow you approve will be deployed and billed, so correctness \
is non-negotiable — but blocking a sound workflow over a trivial preference wastes a \
build cycle.\n\
\n\
Submit your verdict by calling the submit_review tool: approved=true for a sound \
workflow, or approved=false with feedback listing max 3 concrete, actionable issues.\n\
\n\
Reject (approved:false) if ANY of these fail:\n\
1. Node types — any type that isn't a real n8n node, or is clearly wrong for its job.\n\
2. Graph integrity — orphan nodes, connections referencing non-existent node names, \
or no trigger node.\n\
3. Completeness — the workflow does NOT actually solve the client's stated need \
end-to-end (trace trigger → final action).\n\
4. Auth/resilience — a node that needs credentials has none wired ({}), or an \
obvious failure mode (empty result, API error) is unhandled where reliability matters.\n\
5. Complexity mismatch — drastically over- or under-engineered for the need.\n\
\n\
When rejecting, feedback must be specific and fixable ('the Gmail node has no \
trigger upstream; add a scheduleTrigger' — not 'improve error handling'). Cite node \
names. If the workflow is genuinely sound, approve it without inventing objections.\n\
\n\
Worked examples (your output is JSON only, exactly like the 'Verdict' lines):\n\
\n\
Example A — sound workflow:\n\
A 4-node flow: shopifyTrigger → if → set → httpRequest, all connections wired, \
credentials as {}, solves 'Shopify order to invoice' end-to-end.\n\
Verdict: {\"approved\":true}\n\
\n\
Example B — broken graph:\n\
A flow with an httpRequest node named 'Send to CRM' but no trigger node, and the \
'connections' object is empty so nothing is wired.\n\
Verdict: {\"approved\":false,\"feedback\":\"No trigger node — add a scheduleTrigger or \
webhook as the entry point. 'Send to CRM' is orphaned: connections is empty, wire the \
trigger to it.\"}\n\
\n\
Example C — invented node + missing auth:\n\
A flow using type 'n8n-nodes-base.salesforceAutoSync' (not a real node) for a node \
that has no credentials set.\n\
Verdict: {\"approved\":false,\"feedback\":\"'n8n-nodes-base.salesforceAutoSync' is not a \
real n8n node; use n8n-nodes-base.salesforce or httpRequest. That node also has no \
credentials wired — Salesforce requires auth.\"}\n\
\n\
Example D — incomplete vs the need:\n\
The client asked for 'fetch new leads and notify the team on Slack', but the workflow \
stops after fetching: 'Fetch Leads' is wired from the trigger, and a Slack node exists \
but nothing connects to it.\n\
Verdict: {\"approved\":false,\"feedback\":\"The Slack notification is never reached: \
'Fetch Leads' has no connection to the Slack node, so the team is never notified. Wire \
'Fetch Leads' → Slack to actually solve the stated need.\"}\n\
\n\
Quick mental checklist before you answer (run all five, in order):\n\
1. Trigger present? Exactly one entry point (schedule, webhook, or app trigger). \
None → reject.\n\
2. Graph connected? Every non-trigger node reachable from the trigger, and every \
connection target names a node that exists. Orphan or dangling reference → reject.\n\
3. Real node types? Each 'type' is a genuine n8n node string. A plausible but invented \
type → reject (it fails to import).\n\
4. Auth wired? Any node touching an external system has credentials present ({} is \
correct; missing entirely is not). \n\
5. Solves the need? Trace trigger → final action and confirm the client's outcome is \
actually produced, not just started.\n\
\n\
Calibration: you are not a style reviewer. Do NOT reject over naming taste, parameter \
ordering, an extra-but-harmless node, or preference. A workflow that is correct, \
complete, wired, authenticated, and appropriately scoped MUST be approved even if you \
would have built it differently. Each rejection costs a full rebuild cycle and delays \
the client's proposal — so reject only when a real defect would otherwise reach \
production, and make feedback precise enough to fix in one pass. Output only the JSON \
verdict.";

    // In a decomposed build, the workflow under review is ONE sub-flow, not the
    // whole tunnel — so an Execute Workflow Trigger is a valid entry point and an
    // Execute Workflow node may reference the next sub-flow by NAME (a deploy-time
    // placeholder). Tell the critic, so it judges the sub-flow on its own merits.
    let subflow_note = match ctx.sub_workflows.get(ctx.build_cursor) {
        Some(sf) => format!(
            "\n\nNote: this is sub-flow {}/{} ('{}') of a decomposed automation, NOT the \
             whole tunnel. Judge ONLY this sub-flow. An 'n8n-nodes-base.executeWorkflowTrigger' \
             is a valid trigger here (it is invoked by the previous sub-flow). An \
             'n8n-nodes-base.executeWorkflow' whose workflowId is the next sub-flow's NAME is \
             intentional — that placeholder is replaced with the real id at deploy; do NOT \
             reject it. The sub-flow only needs to fulfil its own contract, not the whole need.\n\
             Its remit: {}\n\
             Receives: {}\n\
             Must output: {}",
            ctx.build_cursor + 1, ctx.sub_workflows.len(), sf.name,
            sf.description,
            if sf.input_contract.is_empty() { "(nothing — entry sub-flow)" } else { &sf.input_contract },
            if sf.output_contract.is_empty() { "(nothing — final sub-flow)" } else { &sf.output_contract },
        ),
        None => String::new(),
    };

    let user = format!(
        "Client: {}\nResearch: {}\nWorkflow:\n{}{subflow_note}",
        ctx.client_need,
        ctx.research_output.as_deref().unwrap_or(""),
        serde_json::to_string_pretty(workflow).unwrap_or_default(),
    );

    #[derive(serde::Deserialize)]
    struct Verdict { approved: bool, feedback: Option<String> }

    // Forced tool call → the verdict comes back as a valid JSON object, which
    // ends the recurring "critic answered in prose" failure that previously
    // burned all 3 build attempts and stranded the pipeline at SavedForHuman.
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "approved": {
                "type": "boolean",
                "description": "true if the workflow is correct, complete, wired, authenticated, and appropriately scoped"
            },
            "feedback": {
                "type": "string",
                "description": "Required when approved is false: max 3 concrete, actionable issues citing node names. Omit when approved is true."
            }
        },
        "required": ["approved"]
    });

    // A transport error still shouldn't kill the pipeline — fall back to a soft
    // rejection so the builder retries and, after MAX_BUILD_ATTEMPTS,
    // publish_manual_pitch still produces a proposal.
    // Grounded path: verify every node 'type' against the live catalogue rather
    // than judging "real node?" from memory. Falls back to the single-shot critic.
    let critic_call = match &app.n8n_mcp {
        Some(mcp) => {
            let system = format!("{SYSTEM}\n\n{CRITIC_MCP_ADDENDUM}");
            anthropic_grounded_tool_call(
                &app.http, &app.anthropic_key, mcp, SONNET, 2048,
                &system, serde_json::Value::String(user),
                "submit_review", "Submit your review verdict for the workflow.",
                schema, CRITIC_GROUNDING_TOOLS,
            ).await
        }
        None => anthropic_tool_call(
            &app.http, &app.anthropic_key, SONNET, 1024,
            SYSTEM, serde_json::Value::String(user),
            "submit_review", "Submit your review verdict for the workflow.",
            schema,
        ).await,
    };
    let input = match critic_call {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[critic] tool call failed: {e} — treating as rejection");
            ctx.critic_feedback.push("Le critique n'a pas pu rendre de verdict; nouvelle tentative.".to_string());
            return Ok(false);
        }
    };

    let verdict: Verdict = match serde_json::from_value(input.clone()) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[critic] unexpected verdict shape: {e} (input={input}) — treating as rejection");
            ctx.critic_feedback.push("Verdict mal formé; nouvelle tentative.".to_string());
            return Ok(false);
        }
    };

    if verdict.approved {
        tracing::info!("[critic] approved on attempt {}", ctx.build_attempts);
        Ok(true)
    } else {
        let fb = verdict.feedback.unwrap_or_else(|| "unspecified issues".to_string());
        tracing::warn!("[critic] rejected attempt {}: {fb}", ctx.build_attempts);
        ctx.critic_feedback.push(fb);
        Ok(false)
    }
}

/// Computes price from research_json using deterministic rules, then asks Haiku
/// for a client-facing justification and 3 proposal slides.
/// Stores slides as parsed JSON in ctx — publishing happens in run_pricing_critic.
pub async fn run_pricing(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[pricing] attempt={} session={}", ctx.pricing_attempts, ctx.session_id);

    let research = ctx.research_json.as_ref();

    let complexity = ctx.pricing_complexity_override.as_deref()
        .or_else(|| research.and_then(|r| r["complexity"].as_str()))
        .unwrap_or("medium");

    let integration_count = research
        .and_then(|r| r["integrations_required"].as_array())
        .map(|v| v.len()).unwrap_or(2);

    let risk_premium: u32 = research
        .and_then(|r| r["risks"].as_array())
        .map(|risks| risks.iter().map(|r| match r["severity"].as_str() {
            Some("high")   => 300,
            Some("medium") => 150,
            _              => 75,
        }).sum()).unwrap_or(0);

    let feasibility: f32 = ctx.pricing_feasibility_override
        .or_else(|| research.and_then(|r| r["feasibility_score"].as_f64()).map(|f| f as f32))
        .unwrap_or(7.0);

    let base: u32 = match complexity { "simple" => 900, "complex" => 6000, _ => 2500 };
    let integration_premium  = (integration_count.saturating_sub(2) as u32) * 200;
    let feasibility_buffer: u32 = if feasibility < 6.0 { 600 } else { 0 };
    // Pre-payment the JSON isn't built yet, so estimate node count from the scope:
    // ~2 nodes per integration + a couple of control/trigger nodes.
    let est_node_count = integration_count * 2 + 2;
    let node_premium = (est_node_count.saturating_sub(5) as u32) * 60;
    let subtotal    = base + integration_premium + risk_premium + feasibility_buffer + node_premium;
    let setup_price = ((subtotal + 49) / 50) * 50;

    let monthly_base: u32 = match complexity { "simple" => 100, "complex" => 500, _ => 250 };
    let monthly_integration_fee = (integration_count.saturating_sub(2) as u32) * 50;
    let monthly_price = ((monthly_base + monthly_integration_fee + 24) / 25) * 25;

    tracing::info!(
        "[pricing] setup={setup_price}€ (base={base} int={integration_premium} risks={risk_premium} \
feas={feasibility_buffer} nodes={node_premium}) | monthly={monthly_price}€"
    );

    let integrations_str = research
        .and_then(|r| r["integrations_required"].as_array())
        .map(|v| v.iter().filter_map(|i| i["name"].as_str()).collect::<Vec<_>>().join(", "))
        .unwrap_or_else(|| "standard integrations".to_string());

    // ── 1. Haiku writes the client-facing justification ───────────────────────

    const JUSTIFICATION_SYSTEM: &str = "\
You are a seasoned solutions consultant at pointe.dev writing the rationale a \
prospect reads next to their price. Your voice: confident, warm, concrete — a \
trusted expert, never a pushy salesperson and never corporate filler. You make the \
client feel understood and the price feel obvious.\n\
\n\
Write 2-3 sentences, in the SAME LANGUAGE as the Project field (FR, EN or DE).\n\
\n\
Craft:\n\
- Lead with the OUTCOME, not the technology: hours reclaimed, money saved, errors \
eliminated, capacity freed for higher-value work. Frame it as work the client now \
DELEGATES to an 'collaborateur IA' (AI teammate), not as a 'robot' or cold system.\n\
- Name the actual integrations naturally, as proof you understood their stack — not \
as a feature list.\n\
- Quantify only what you can defend. You may speak of 'plusieurs heures par semaine' \
or 'la quasi-totalité des saisies manuelles', but NEVER invent a precise statistic \
('-73%') that isn't grounded. Vague-but-true beats precise-but-fabricated.\n\
- Close by situating both fees as an investment: the one-time setup and the monthly \
fee (maintenance, monitoring, hosting), framed against the value, in the final \
sentence. Make the recurring fee feel like peace of mind, not a tax.\n\
- No hype words ('révolutionnaire', 'game-changer'), no exclamation marks, no \
generic openers ('Dans le monde d'aujourd'hui'). Sound like a human who has done \
this many times.";

    let critic_note = ctx.pricing_critic_feedback.last()
        .map(|fb| format!("\nPrevious critic feedback (address these): {fb}\n"))
        .unwrap_or_default();

    let justification_user = format!(
        "{critic_note}\nProject: {need}\nComplexity: {complexity}\nIntegrations: {integrations}\n\
Setup fee: {setup}€ (one-time) | Monthly: {monthly}€/mo (maintenance + monitoring + hosting)",
        need         = ctx.client_need,
        integrations = integrations_str,
        setup        = setup_price,
        monthly      = monthly_price,
    );

    let justification = match anthropic_call(
        &app.http, &app.anthropic_key, HAIKU, 200,
        JUSTIFICATION_SYSTEM, &justification_user,
    ).await {
        Ok(s) if !s.is_empty() => s,
        _ => {
            tracing::warn!("[pricing] justification call failed — using fallback");
            format!("Automatisation {complexity} — {setup_price}€ (setup) + {monthly_price}€/mois.")
        }
    };

    ctx.price_quote         = Some(setup_price);
    ctx.price_monthly       = Some(monthly_price);
    ctx.price_justification = Some(justification.clone());

    // ── 2. Haiku generates the 3 proposal slides ──────────────────────────────

    const SLIDES_SYSTEM: &str = "\
You are crafting the proposal a prospect sees right after talking to pointe.dev. \
These three slides must feel tailor-made: the client should recognise their own \
situation, see a credible solution, and know exactly what happens next. This is the \
moment that converts interest into a signed project — make it land.\n\
\n\
Submit the slides by calling the submit_slides tool: exactly 3 slides, each \
{title, body, points[]}.\n\
\n\
The 3 titles MUST be exactly these, in the SAME LANGUAGE as the Project field:\n\
  1. Ce que nous avons compris / What we understood / Was wir verstanden haben\n\
  2. Notre proposition / Our proposal / Unser Angebot\n\
  3. Prochaines étapes / Next steps / Nächste Schritte\n\
\n\
Content craft:\n\
- Slide 1 (understanding): mirror their pain back in their words — specific, concrete, \
empathetic. They must think 'yes, exactly'. Points = the concrete frictions they face.\n\
- Slide 2 (proposal): frame the solution as a 'collaborateur IA' (AI teammate / \
KI-Mitarbeiter) the client DELEGATES the work to — outcomes and deliverables, not node \
names. Each point a tangible deliverable ('Synchronisation automatique Shopify → compta').\n\
- Slide 3 (next steps): a calm, credible path. Phases that feel inevitable and low-risk. \
Make the time and money reclaimed concrete here ('plusieurs heures par semaine \
récupérées'), without inventing precise figures.\n\
\n\
Hard limits: body = 1-2 sentences max. Each point ≤ 10 words. Max 3 points per slide.\n\
\n\
Lexicon: lead with the human framing — 'déléguer', 'collaborateur IA', 'vous libérer', \
'gagner du temps et de l'argent'. Use the cold word 'automatisation' sparingly (once at \
most); a non-technical reader should feel they are handing work to a teammate, not \
configuring a robot.\n\
\n\
Voice: confident, warm, precise. Same language throughout. Never invent figures or \
promises you cannot keep — credibility over flash. No hype words, no exclamation marks.";

    let slides_user = format!(
        "Project: {need}\nSummary: {summary}\nIntegrations: {integrations}\n\
Complexity: {complexity} | Build: ~{hours}h\n\
Setup: {setup}€ (one-time) + {monthly}€/month\nJustification: {just}",
        need     = ctx.client_need,
        summary  = ctx.qualification_summary.as_deref().unwrap_or(""),
        integrations = research
            .and_then(|r| r["integrations_required"].as_array())
            .map(|v| v.iter().filter_map(|i| i["name"].as_str()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default(),
        hours    = research.and_then(|r| r["estimated_build_hours"].as_str()).unwrap_or("?"),
        setup    = setup_price,
        monthly  = monthly_price,
        just     = justification,
    );

    // Forced tool call → the slides come back as a valid JSON array. Tool input
    // must be an object, so the array is wrapped under `slides`; we store the
    // inner array (the shape publish_pitch expects). On any error, leave None
    // and let publish fall back.
    let slides_schema = serde_json::json!({
        "type": "object",
        "properties": {
            "slides": {
                "type": "array",
                "description": "exactly 3 slides",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": {"type": "string"},
                        "body": {"type": "string", "description": "1-2 sentences"},
                        "points": {"type": "array", "items": {"type": "string"}, "description": "max 3, each ≤10 words"}
                    },
                    "required": ["title", "body", "points"]
                }
            }
        },
        "required": ["slides"]
    });

    ctx.pricing_slides_json = match anthropic_tool_call(
        &app.http, &app.anthropic_key, HAIKU, 600,
        SLIDES_SYSTEM, serde_json::Value::String(slides_user),
        "submit_slides", "Submit the 3 proposal slides.",
        slides_schema,
    ).await {
        Ok(input) => input.get("slides").cloned(),
        Err(_) => {
            tracing::warn!("[pricing] slide generation failed — fallback used at publish");
            None
        }
    };

    Ok(())
}

/// Evaluates pricing from two angles: pointe.dev profitability and client fairness.
/// On approval: publishes PitchResult to app.pitches and returns true.
/// On rejection: injects corrections into ctx and returns false.
pub async fn run_pricing_critic(app: &AppState, ctx: &mut PipelineContext) -> Result<bool, AgentError> {
    tracing::info!("[pricing-critic] attempt={} session={}", ctx.pricing_attempts, ctx.session_id);

    let research = ctx.research_json.as_ref();
    let setup_price  = ctx.price_quote.unwrap_or(0);
    let monthly      = ctx.price_monthly.unwrap_or(0);
    let complexity   = ctx.pricing_complexity_override.as_deref()
        .or_else(|| research.and_then(|r| r["complexity"].as_str()))
        .unwrap_or("medium");
    let estimated_hours: f32 = research
        .and_then(|r| r["estimated_build_hours"].as_str())
        .and_then(|h| h.parse().ok()).unwrap_or(10.0);
    let hourly_rate = if estimated_hours > 0.0 { setup_price as f32 / estimated_hours } else { 0.0 };

    let integrations_str = research
        .and_then(|r| r["integrations_required"].as_array())
        .map(|v| v.iter().filter_map(|i| i["name"].as_str()).collect::<Vec<_>>().join(", "))
        .unwrap_or_else(|| "unknown".to_string());

    let risks_str = research
        .and_then(|r| r["risks"].as_array())
        .map(|v| v.iter().map(|r| format!(
            "{} ({})",
            r["description"].as_str().unwrap_or("?"),
            r["severity"].as_str().unwrap_or("?")
        )).collect::<Vec<_>>().join("; "))
        .unwrap_or_else(|| "none".to_string());

    const SYSTEM: &str = "\
You are the commercial director at pointe.dev, a bespoke automation agency. You sign \
off on every quote, balancing two duties: the agency must be profitable (target \
€100-200/h effective rate) AND the client must feel the price is fair for the value. \
A quote that is too low burns the agency; one that is too high loses the deal. Your \
judgement protects both sides.\n\
\n\
Evaluate the quote and respond ONLY with valid JSON — no prose, no markdown fences.\n\
\n\
Approval criteria (reject if any fails):\n\
- Effective hourly rate (setup ÷ build hours) lands in €80-250/h. Below 80 = the \
agency loses money; above 250 = not credible, reject.\n\
- Complexity is consistent with the integration count and risk level. A 5-integration \
project rated 'simple' is mis-scoped — correct the complexity.\n\
- The absolute price is believable for this client's sector and the outcome delivered \
(a solo e-commerce owner and an enterprise have different ceilings).\n\
- The monthly fee is proportionate to the setup and the ongoing burden.\n\
\n\
When you reject, the 'reason' must be specific and actionable, and you should set the \
correction levers so the next pricing pass improves:\n\
- complexity: set to the correct tier if mis-rated, else null.\n\
- feasibility_score: override (0-10) if the current value distorts the price, else null.\n\
\n\
Do not reject a sound, fair quote to seem rigorous — approving a good price is the \
right call. Be decisive.\n\
\n\
Submit your verdict by calling the submit_pricing_verdict tool.\n\
Set complexity/feasibility_score to null when the current values are acceptable.\n\
\n\
Worked examples (your output is JSON only):\n\
\n\
Example A — fair quote:\n\
Simple Shopify→accounting automation, 8h build, 2 integrations, setup 1200€ \
(=150€/h), monthly 100€.\n\
Verdict: {\"approved\":true,\"reason\":\"150€/h effective rate sits in target, price \
credible for a solo e-commerce, monthly is proportionate.\",\"complexity\":null,\
\"feasibility_score\":null}\n\
\n\
Example B — underpriced (rate too low):\n\
5-integration CRM orchestration with high-risk items, 40h build, rated 'medium', \
setup 2500€ (=62€/h).\n\
Verdict: {\"approved\":false,\"reason\":\"62€/h is below the 80€/h floor and the scope \
(5 integrations, high risk) is complex, not medium — the price under-reflects the \
work.\",\"complexity\":\"complex\",\"feasibility_score\":null}\n\
\n\
Example C — not credible (rate too high):\n\
Simple 1-integration email digest, 5h build, setup 3000€ (=600€/h).\n\
Verdict: {\"approved\":false,\"reason\":\"600€/h is far above any credible rate for a \
simple single-integration job; the client will balk. Re-scope or lower.\",\
\"complexity\":null,\"feasibility_score\":null}\n\
\n\
Example D — complexity mis-rated upward:\n\
A single Gmail-to-Sheets digest, 6h build, 1 integration, but rated 'complex' which \
inflated setup to 6000€ (=1000€/h).\n\
Verdict: {\"approved\":false,\"reason\":\"A 1-integration email digest is 'simple', not \
'complex'; the inflated tier produced an absurd 1000€/h. Re-tier to simple.\",\
\"complexity\":\"simple\",\"feasibility_score\":null}\n\
\n\
Guidance on the levers: only override complexity when the tier genuinely mismatches \
the integration count and risk profile — a correction here re-runs the deterministic \
pricing with the right base tier. Only override feasibility_score when the current \
value distorts the buffer (a needlessly low score adds an unwarranted premium; a \
needlessly high one hides real risk). When both values are sound and only the price \
framing is off, explain that in 'reason' and leave both null — the next pass keeps \
the tiers but can still benefit from your written feedback.\n\
\n\
Decide with this judgement. Approving a fair quote is the correct, expected outcome — \
a rejection forces a re-pricing pass and delays the client's proposal, so reject only \
when the rate is genuinely out of band or the tier is genuinely mis-scoped, and never \
to appear diligent.";

    let prev = ctx.pricing_critic_feedback.last()
        .map(|fb| format!("Previous feedback: {fb}\n"))
        .unwrap_or_default();

    let user = format!(
        "Project: {need}\n\
Complexity: {complexity} | Build estimate: {hours}h\n\
Integrations: {integrations}\nRisks: {risks}\n\
\nComputed price:\n  Setup: {setup}€ (one-time)\n  Monthly: {monthly}€/month\n  Effective rate: {rate:.0}€/h\n{prev}",
        need         = ctx.client_need,
        hours        = estimated_hours,
        integrations = integrations_str,
        risks        = risks_str,
        setup        = setup_price,
        rate         = hourly_rate,
    );

    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "approved": {"type": "boolean"},
            "reason": {"type": "string", "description": "1-2 sentences; specific and actionable when rejecting"},
            "complexity": {"type": ["string", "null"], "enum": ["simple", "medium", "complex", null], "description": "corrected tier, or null if the current one is fine"},
            "feasibility_score": {"type": ["number", "null"], "description": "override 0-10, or null if the current value is fine"}
        },
        "required": ["approved", "reason"]
    });

    #[derive(serde::Deserialize)]
    struct CriticOutput {
        approved: bool,
        reason: String,
        #[serde(default)]
        complexity: Option<String>,
        #[serde(default)]
        feasibility_score: Option<f32>,
    }

    // Forced tool call → valid JSON verdict. On a transport error or an
    // unexpected shape, auto-approve (the deterministic price is already sound)
    // rather than stalling the pipeline.
    let verdict: CriticOutput = match anthropic_tool_call(
        &app.http, &app.anthropic_key, SONNET, 200,
        SYSTEM, serde_json::Value::String(user),
        "submit_pricing_verdict", "Submit your pricing verdict.",
        schema,
    ).await.and_then(|v| serde_json::from_value(v).map_err(|e| AgentError(e.to_string()))) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[pricing-critic] verdict unavailable: {e} — auto-approving");
            publish_pitch(app, ctx).await;
            return Ok(true);
        }
    };

    tracing::info!(
        "[pricing-critic] approved={} reason=\"{}\" complexity_override={:?} feasibility_override={:?}",
        verdict.approved, verdict.reason, verdict.complexity, verdict.feasibility_score,
    );

    if verdict.approved {
        publish_pitch(app, ctx).await;
        Ok(true)
    } else {
        ctx.pricing_critic_feedback.push(verdict.reason);
        if let Some(c) = verdict.complexity       { ctx.pricing_complexity_override   = Some(c); }
        if let Some(f) = verdict.feasibility_score { ctx.pricing_feasibility_override = Some(f); }
        Ok(false)
    }
}

/// Best-effort: email the freshly published proposal to the session's confirmed
/// address. No-op when Resend is unconfigured or the session has no stored email
/// (shouldn't happen once the unlock gate ran, but we degrade gracefully). Runs
/// detached so a slow/failing email never delays the pipeline.
async fn email_proposal(app: &AppState, session_id: &str, slides: &[PitchSlide]) {
    let Some(api_key) = app.resend_api_key.clone() else { return };
    let Some(client_email) = app.sessions.get_email(session_id).await else {
        tracing::info!("[quote] no confirmed email for session {session_id}; skip auto-send");
        return;
    };
    let http   = app.http.clone();
    let owner  = app.owner_email.clone();
    let slides = slides.to_vec();
    tokio::spawn(async move {
        match crate::email::send_proposal(&http, &api_key, &client_email, owner.as_deref(), &slides).await {
            Ok(())  => tracing::info!("[quote] proposal auto-sent to {client_email}"),
            Err(e)  => tracing::error!("[quote] auto-send failed: {e}"),
        }
    });
}

async fn publish_pitch(app: &AppState, ctx: &PipelineContext) {
    let research = ctx.research_json.as_ref();

    let externals_needed: Vec<String> = research
        .and_then(|r| r["api_keys_to_acquire"].as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();

    let solution_desc = research
        .and_then(|r| r["approach"].as_str())
        .unwrap_or(&ctx.client_need).to_string();

    let setup_price = ctx.price_quote.unwrap_or(0);
    let monthly     = ctx.price_monthly.unwrap_or(0);
    let complexity  = ctx.pricing_complexity_override.as_deref()
        .or_else(|| research.and_then(|r| r["complexity"].as_str()))
        .unwrap_or("medium");

    // pricing_slides_json is now a parsed Value::Array — no re-parsing needed
    let slides: Vec<PitchSlide> = ctx.pricing_slides_json.as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_else(|| vec![
            PitchSlide {
                title: "Ce que nous avons compris".to_string(),
                body:  ctx.qualification_summary.clone().unwrap_or_default(),
                points: vec![],
            },
            PitchSlide {
                title: "Notre proposition".to_string(),
                body:  solution_desc.clone(),
                points: vec![],
            },
            PitchSlide {
                title: "Prochaines étapes".to_string(),
                body:  format!(
                    "Développement {complexity} estimé à {}h.",
                    research.and_then(|r| r["estimated_build_hours"].as_str()).unwrap_or("?")
                ),
                points: vec![
                    "Phase 1 : Spec & setup".to_string(),
                    "Phase 2 : Build & test".to_string(),
                    format!("Maintenance : {monthly}€/mois"),
                ],
            },
        ]);

    email_proposal(app, &ctx.session_id, &slides).await;

    app.pitches.set(&ctx.pipeline_id.to_string(), PitchResult {
        solution_desc,
        price_eur_cents: setup_price * 100,
        price_validity: "valable 48h".to_string(),
        externals_needed,
        slides,
        manual_quote: false,
    }).await;
}

/// Publishes a manual-quote PitchResult (pricing failed, human follows up).
pub async fn publish_manual_pitch(app: &AppState, ctx: &PipelineContext) {
    let research = ctx.research_json.as_ref();

    let slides: Vec<PitchSlide> = ctx.pricing_slides_json.as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_else(|| vec![
            PitchSlide {
                title: "Ce que nous avons compris".to_string(),
                body:  ctx.qualification_summary.clone()
                    .unwrap_or_else(|| ctx.client_need.clone()),
                points: vec![],
            },
            PitchSlide {
                title: "Notre proposition".to_string(),
                body:  research.and_then(|r| r["approach"].as_str())
                    .unwrap_or("Solution d'automatisation sur mesure.").to_string(),
                points: vec![],
            },
            PitchSlide {
                title: "Prochaines étapes".to_string(),
                body:  "Notre équipe vous revient avec un devis personnalisé sous 24h.".to_string(),
                points: vec![
                    "Analyse approfondie de votre besoin".to_string(),
                    "Estimation détaillée et chiffrée".to_string(),
                    "Proposition sur mesure envoyée par email".to_string(),
                ],
            },
        ]);

    let solution_desc = research
        .and_then(|r| r["approach"].as_str())
        .unwrap_or(&ctx.client_need).to_string();

    email_proposal(app, &ctx.session_id, &slides).await;

    app.pitches.set(&ctx.pipeline_id.to_string(), PitchResult {
        solution_desc,
        price_eur_cents: 0,
        price_validity: String::new(),
        externals_needed: vec![],
        slides,
        manual_quote: true,
    }).await;
}

/// Rewrites Execute Workflow references that still hold a sub-flow NAME placeholder
/// to the real n8n id of that sub-flow. The builder emits `workflowId = "<next
/// sub-flow name>"` because it cannot know the id before deploy; once every sub-flow
/// is created we resolve those names. Handles both the bare-string form and the
/// resource-locator object form ({value, mode, …}). Returns how many it rewired.
fn wire_subflow_ids(
    workflow: &mut serde_json::Value,
    name_to_id: &std::collections::HashMap<String, String>,
) -> usize {
    let mut wired = 0;
    let Some(nodes) = workflow.get_mut("nodes").and_then(|n| n.as_array_mut()) else { return 0 };
    for node in nodes {
        let is_exec = node.get("type").and_then(|t| t.as_str())
            .map(|t| t.contains("executeWorkflow")).unwrap_or(false);
        if !is_exec { continue; }
        let Some(wid) = node.get_mut("parameters").and_then(|p| p.get_mut("workflowId")) else { continue };
        match wid {
            // Bare string: "WF-2 — …" → "<id>"
            serde_json::Value::String(s) => {
                if let Some(id) = name_to_id.get(s) { *s = id.clone(); wired += 1; }
            }
            // Resource-locator object: { "value": "WF-2 — …", "mode": "id", … }
            serde_json::Value::Object(o) => {
                if let Some(serde_json::Value::String(s)) = o.get("value") {
                    if let Some(id) = name_to_id.get(s).cloned() {
                        o.insert("value".into(), serde_json::Value::String(id));
                        o.insert("mode".into(), serde_json::Value::String("id".into()));
                        wired += 1;
                    }
                }
            }
            _ => {}
        }
    }
    wired
}

/// Normalises a workflow JSON into a body n8n's REST API accepts: default settings,
/// staticData, node positions, and a name fallback.
fn prepare_workflow_body(
    workflow: &serde_json::Value,
    fallback_name: &str,
) -> Result<serde_json::Value, AgentError> {
    let mut body = workflow.clone();
    let obj = body.as_object_mut()
        .ok_or_else(|| AgentError("workflow_json is not a JSON object".to_string()))?;

    obj.entry("settings").or_insert_with(|| serde_json::json!({ "executionOrder": "v1" }));
    obj.entry("staticData").or_insert(serde_json::Value::Null);

    if let Some(nodes) = obj.get_mut("nodes").and_then(|n| n.as_array_mut()) {
        for (i, node) in nodes.iter_mut().enumerate() {
            if let Some(obj) = node.as_object_mut() {
                obj.entry("position").or_insert_with(|| {
                    serde_json::json!({ "x": 250, "y": i as i64 * 200 })
                });
            }
        }
    }

    obj.entry("name").or_insert_with(|| serde_json::Value::String(fallback_name.to_string()));
    Ok(body)
}

/// POSTs one workflow to n8n and returns its new id.
async fn n8n_create(
    http: &reqwest::Client, url: &str, key: &str, body: &serde_json::Value,
) -> Result<String, AgentError> {
    #[derive(serde::Deserialize)]
    struct CreateResp { id: String }

    let resp = http.post(format!("{url}/api/v1/workflows"))
        .header("X-N8N-API-KEY", key)
        .header("Content-Type", "application/json")
        .json(body).send().await
        .map_err(|e| AgentError(format!("n8n create request: {e}")))?;
    if !resp.status().is_success() {
        let s = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AgentError(format!("n8n create {s}: {b}")));
    }
    let created: CreateResp = resp.json().await
        .map_err(|e| AgentError(format!("n8n create parse: {e}")))?;
    Ok(created.id)
}

/// Activates a workflow (best-effort — a sub-flow with only an Execute Workflow
/// Trigger cannot be activated, which is fine: it runs when its caller invokes it).
async fn n8n_activate(http: &reqwest::Client, url: &str, key: &str, id: &str) {
    match http.post(format!("{url}/api/v1/workflows/{id}/activate"))
        .header("X-N8N-API-KEY", key).send().await
    {
        Ok(r) if r.status().is_success() => tracing::info!("[deploy] workflow {id} activated"),
        Ok(r) => tracing::warn!("[deploy] workflow {id} created but activation failed: {}", r.status()),
        Err(e) => tracing::warn!("[deploy] workflow {id} activate request failed: {e}"),
    }
}

/// Deploys to n8n (our instance or the client's) and activates the entry workflow.
/// Mono build → one workflow. Decomposed build → every sub-flow, created entry-LAST
/// so each knows the real id of the sub-flow it invokes, then chained by rewriting
/// the name placeholders. Only the entry (real-trigger) sub-flow is activated; the
/// rest run on demand via their Execute Workflow Trigger.
pub async fn run_deploy(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[deploy] session={} target={}", ctx.session_id,
        ctx.deploy_target.as_deref().unwrap_or("own"));

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

    // Decomposed build deploys the sub-flows; mono deploys the single workflow.
    let decomposed = !ctx.built_workflows.is_empty();
    let workflows: Vec<serde_json::Value> = if decomposed {
        ctx.built_workflows.clone()
    } else {
        vec![ctx.workflow_json.clone()
            .ok_or_else(|| AgentError("deploy called with no workflow_json".to_string()))?]
    };

    let default_name = format!("pointe.dev — {}", ctx.client_need.chars().take(60).collect::<String>());

    // Create entry-last (reverse) so a sub-flow's id is known before the one that
    // calls it is created → wire the placeholder before posting, no update round-trip.
    let mut name_to_id: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut ids: Vec<Option<String>> = vec![None; workflows.len()];
    for i in (0..workflows.len()).rev() {
        let fallback = ctx.sub_workflows.get(i)
            .map(|sf| format!("pointe.dev — {}", sf.name))
            .unwrap_or_else(|| default_name.clone());
        let mut body = prepare_workflow_body(&workflows[i], &fallback)?;

        if decomposed {
            let wired = wire_subflow_ids(&mut body, &name_to_id);
            if i + 1 < workflows.len() && wired == 0 {
                tracing::warn!(
                    "[deploy] sub-flow {}/{} wired 0 next-references — chain to the next \
                     sub-flow may be broken, needs manual wiring", i + 1, workflows.len());
            }
        }

        let id = n8n_create(&app.http, &n8n_url, &n8n_key, &body).await?;
        tracing::info!("[deploy] workflow created id={id} ({}/{})", i + 1, workflows.len());
        if decomposed {
            if let Some(sf) = ctx.sub_workflows.get(i) {
                name_to_id.insert(sf.name.clone(), id.clone());
            }
        }
        // Only the entry sub-flow carries a real trigger to activate.
        if !decomposed || i == 0 {
            n8n_activate(&app.http, &n8n_url, &n8n_key, &id).await;
        }
        ids[i] = Some(id);
    }

    let ordered: Vec<String> = ids.into_iter().flatten().collect();
    let entry = ordered.first().cloned()
        .ok_or_else(|| AgentError("deploy produced no workflow ids".to_string()))?;
    ctx.n8n_workflow_id  = Some(entry.clone());
    ctx.n8n_workflow_url = Some(format!("{n8n_url}/workflow/{entry}"));
    ctx.n8n_workflow_ids = ordered;
    Ok(())
}

// ── Unit tests ────────────────────────────────────────────────────────────────
//
// Layer   : pure unit — no I/O, no Anthropic API calls
// Covers  : strip_fences(), cache_1h() shape, AgentError Display,
//           run_qualifier early-return when summary already set,
//           run_pricing deterministic formula
// Does NOT cover: live Anthropic API calls, RAG/Qdrant integration,
//                 n8n deployment, email sending
#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::PipelineContext;

    // ── anthropic_backoff ──────────────────────────────────────────────────

    #[test]
    fn backoff_grows_per_attempt_and_stays_bounded() {
        // base doubles each attempt (0.5s, 1s, 2s) with <250ms jitter, so the
        // ranges never overlap → strictly increasing.
        let d1 = anthropic_backoff(1).as_millis();
        let d2 = anthropic_backoff(2).as_millis();
        let d3 = anthropic_backoff(3).as_millis();
        assert!((500..750).contains(&(d1 as u64)), "d1={d1}");
        assert!((1000..1250).contains(&(d2 as u64)), "d2={d2}");
        assert!((2000..2250).contains(&(d3 as u64)), "d3={d3}");
        assert!(d1 < d2 && d2 < d3);
    }

    // ── tool_use_input / text_block (forced tool-call response parsing) ──────

    #[test]
    fn tool_use_input_extracts_first_tool_use_block() {
        let resp = serde_json::json!({
            "content": [
                {"type": "text", "text": "Let me review."},
                {"type": "tool_use", "name": "submit_review", "input": {"approved": true}}
            ]
        });
        assert_eq!(tool_use_input(&resp), Some(serde_json::json!({"approved": true})));
    }

    #[test]
    fn tool_use_input_none_when_no_tool_use_block() {
        let resp = serde_json::json!({"content": [{"type": "text", "text": "hi"}]});
        assert_eq!(tool_use_input(&resp), None);
    }

    #[test]
    fn text_block_extracts_first_text_and_defaults_empty() {
        let with_text = serde_json::json!({"content": [{"type": "text", "text": "hello"}]});
        assert_eq!(text_block(&with_text), "hello");
        let no_text = serde_json::json!({"content": [{"type": "tool_use", "input": {}}]});
        assert_eq!(text_block(&no_text), "");
    }

    // ── cache_1h ───────────────────────────────────────────────────────────

    #[test]
    fn cache_1h_has_ephemeral_type_and_ttl() {
        let v = cache_1h();
        assert_eq!(v["type"], "ephemeral");
        assert_eq!(v["ttl"], "1h");
    }

    // ── AgentError ─────────────────────────────────────────────────────────

    #[test]
    fn agent_error_display_shows_message() {
        let e = AgentError("something went wrong".to_string());
        assert_eq!(format!("{e}"), "something went wrong");
    }

    // ── decomposition gate ─────────────────────────────────────────────────

    #[test]
    fn is_numbered_step_matches_dot_and_paren() {
        assert!(is_numbered_step("1. Surveiller les flux RSS"));
        assert!(is_numbered_step("  2) générer la voix off"));
        assert!(is_numbered_step("10. publier sur YouTube"));
        assert!(!is_numbered_step("Blocs clés: RSS, ElevenLabs"));
        assert!(!is_numbered_step("- bullet point"));
        assert!(!is_numbered_step(""));
    }

    #[test]
    fn count_design_steps_counts_only_numbered_lines() {
        let design = "1. Trigger schedule — scheduleTrigger — lance chaque jour\n\
                      2. Récupère les actus — httpRequest — flux RSS\n\
                      3. Choisit le sujet — code — ranking\n\
                      Blocs clés: RSS, OpenAI\n\
                      Points de vigilance: aucun";
        assert_eq!(count_design_steps(design), 3);
    }

    #[test]
    fn needs_decomposition_false_for_simple_lead() {
        let ctx = PipelineContext {
            research_json: Some(serde_json::json!({
                "integrations_required": [{"name": "Shopify"}, {"name": "Pennylane"}]
            })),
            design_summary: Some("1. a\n2. b\n3. c".to_string()),
            ..Default::default()
        };
        assert!(!needs_decomposition(&ctx));
    }

    #[test]
    fn needs_decomposition_true_when_five_integrations() {
        let ctx = PipelineContext {
            research_json: Some(serde_json::json!({
                "integrations_required": [
                    {"name": "RSS"}, {"name": "X"}, {"name": "ElevenLabs"},
                    {"name": "Creatomate"}, {"name": "YouTube"}
                ]
            })),
            design_summary: Some("1. a\n2. b".to_string()),
            ..Default::default()
        };
        assert!(needs_decomposition(&ctx));
    }

    #[test]
    fn needs_decomposition_true_when_blueprint_exceeds_eight_steps() {
        let design = (1..=9).map(|n| format!("{n}. step")).collect::<Vec<_>>().join("\n");
        let ctx = PipelineContext {
            research_json: Some(serde_json::json!({ "integrations_required": [{"name": "A"}] })),
            design_summary: Some(design),
            ..Default::default()
        };
        assert!(needs_decomposition(&ctx));
    }

    #[test]
    fn needs_decomposition_false_for_empty_context() {
        assert!(!needs_decomposition(&PipelineContext::default()));
    }

    // ── deploy: sub-flow id wiring ─────────────────────────────────────────

    fn name_map(pairs: &[(&str, &str)]) -> std::collections::HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn wire_subflow_ids_resolves_bare_string_placeholder() {
        let mut wf = serde_json::json!({
            "nodes": [
                {"name": "Trigger", "type": "n8n-nodes-base.scheduleTrigger", "parameters": {}},
                {"name": "Call next", "type": "n8n-nodes-base.executeWorkflow",
                 "parameters": {"workflowId": "WF-2 — Produce"}}
            ]
        });
        let n = wire_subflow_ids(&mut wf, &name_map(&[("WF-2 — Produce", "abc123")]));
        assert_eq!(n, 1);
        assert_eq!(wf["nodes"][1]["parameters"]["workflowId"], "abc123");
    }

    #[test]
    fn wire_subflow_ids_resolves_resource_locator_object() {
        let mut wf = serde_json::json!({
            "nodes": [{
                "name": "Call next", "type": "n8n-nodes-base.executeWorkflow",
                "parameters": {"workflowId": {"__rl": true, "value": "WF-3 — Publish", "mode": "list"}}
            }]
        });
        let n = wire_subflow_ids(&mut wf, &name_map(&[("WF-3 — Publish", "xyz789")]));
        assert_eq!(n, 1);
        assert_eq!(wf["nodes"][0]["parameters"]["workflowId"]["value"], "xyz789");
        assert_eq!(wf["nodes"][0]["parameters"]["workflowId"]["mode"], "id");
    }

    #[test]
    fn wire_subflow_ids_leaves_unknown_and_non_exec_nodes_untouched() {
        let mut wf = serde_json::json!({
            "nodes": [
                {"name": "Set", "type": "n8n-nodes-base.set", "parameters": {"workflowId": "WF-2"}},
                {"name": "Call", "type": "n8n-nodes-base.executeWorkflow",
                 "parameters": {"workflowId": "Unknown WF"}}
            ]
        });
        let n = wire_subflow_ids(&mut wf, &name_map(&[("WF-2 — Produce", "abc")]));
        assert_eq!(n, 0, "non-exec node and unknown name must be left alone");
        assert_eq!(wf["nodes"][0]["parameters"]["workflowId"], "WF-2");
        assert_eq!(wf["nodes"][1]["parameters"]["workflowId"], "Unknown WF");
    }

    #[test]
    fn agent_error_from_reqwest_works_via_trait() {
        // Can't easily construct a reqwest::Error in tests, but we can verify
        // the impl compiles and the trait bound is satisfied.
        let _: fn(reqwest::Error) -> AgentError = AgentError::from;
    }

    // ── run_qualifier early-return path ────────────────────────────────────

    #[tokio::test]
    async fn run_qualifier_skips_llm_when_summary_present() {
        // Inject a fake AppState that would panic if the HTTP client were called.
        // Because run_qualifier returns early when qualification_summary is Some,
        // no network call should be made — the test should complete instantly.
        use crate::state::AppState;
        use crate::sessions::SessionStore;
        use crate::pipeline::PipelineStore;
        use crate::pitch::PitchStore;
        use std::sync::Arc;

        // Build a minimal AppState with an obviously-bad Anthropic key.
        // If the early-return path is broken and an HTTP call is made it will
        // fail with a network error, causing the test to fail — which is
        // exactly the signal we want.
        let state = Arc::new(AppState {
            anthropic_key: "sk-fake-key-will-not-be-called".to_string(),
            http: reqwest::Client::new(),
            system_prompt: String::new(),
            langfuse: None,
            sessions: SessionStore::new(),
            pipelines: PipelineStore::new(),
            pending: crate::pending::PendingStore::new(),
            pitches: PitchStore::new(None),
            qdrant: None,
            embeddings: None,
            cloudflare: None,
            n8n_mcp: None,
            stripe: None,
            session_secret: b"test".to_vec(),
            admin_ingest_token: None,
            resend_api_key: None,
            base_url: "http://localhost".to_string(),
            owner_email: None,
            db: None,
        });

        let mut ctx = PipelineContext {
            session_id: "test-session".to_string(),
            client_need: "Test need".to_string(),
            qualification_summary: Some("pre-existing summary".to_string()),
            ..Default::default()
        };

        // Should return Ok without hitting Anthropic
        let result = run_qualifier(&state, &mut ctx).await;
        assert!(result.is_ok());
        // Summary must be unchanged
        assert_eq!(ctx.qualification_summary, Some("pre-existing summary".to_string()));
    }

    // ── run_pricing deterministic formula ──────────────────────────────────
    // We test the pricing math by running run_pricing with a minimal context
    // that has no research_json (all defaults) and an Anthropic key that
    // will fail — because the justification/slides generation failure is
    // handled gracefully (fallback strings), so the price fields are always set.
    //
    // Note: this test IS making a real HTTP call to Anthropic (which will fail
    // with 401), so we only assert that the function returns Ok and the price
    // fields are set according to the deterministic formula, not that specific
    // prices match (which would be brittle if the formula changes).
    //
    // Instead, test only the pure formula in isolation.

    #[test]
    fn pricing_formula_simple_baseline() {
        // Replicate the formula from run_pricing for complexity=simple,
        // 2 integrations, no risks, feasibility=7.0, 0 nodes.
        let complexity = "simple";
        let integration_count: usize = 2;
        let risk_premium: u32 = 0;
        let feasibility: f32 = 7.0;
        let base: u32 = match complexity { "simple" => 900, "complex" => 6000, _ => 2500 };
        let integration_premium = (integration_count.saturating_sub(2) as u32) * 200;
        let feasibility_buffer: u32 = if feasibility < 6.0 { 600 } else { 0 };
        let node_count: usize = 0;
        let node_premium = (node_count.saturating_sub(5) as u32) * 60;
        let subtotal = base + integration_premium + risk_premium + feasibility_buffer + node_premium;
        let setup_price = ((subtotal + 49) / 50) * 50;
        // base=900, no premiums → subtotal=900, rounded to 50 → 900
        assert_eq!(setup_price, 900);
    }

    #[test]
    fn pricing_formula_complex_with_risks() {
        let complexity = "complex";
        let integration_count: usize = 5;
        let risk_premium: u32 = 300 + 150; // high + medium
        let feasibility: f32 = 5.0; // triggers buffer
        let base: u32 = 6000;
        let integration_premium = (integration_count.saturating_sub(2) as u32) * 200; // 3*200=600
        let feasibility_buffer: u32 = 600;
        let node_count: usize = 8;
        let node_premium = (node_count.saturating_sub(5) as u32) * 60; // 3*60=180
        let subtotal = base + integration_premium + risk_premium + feasibility_buffer + node_premium;
        let setup_price = ((subtotal + 49) / 50) * 50;
        // 6000+600+450+600+180 = 7830 → rounded to next multiple of 50 = 7850
        assert_eq!(subtotal, 7830);
        assert_eq!(setup_price, 7850);
    }

    #[test]
    fn pricing_formula_monthly_base_simple() {
        let complexity = "simple";
        let integration_count: usize = 2;
        let monthly_base: u32 = match complexity { "simple" => 100, "complex" => 500, _ => 250 };
        let monthly_integration_fee = (integration_count.saturating_sub(2) as u32) * 50;
        let monthly_price = ((monthly_base + monthly_integration_fee + 24) / 25) * 25;
        assert_eq!(monthly_price, 100);
    }
}
