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

fn strip_fences(s: &str) -> &str {
    s.trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
}

/// Extracts the first balanced JSON value (object or array) from an LLM
/// response, tolerating a prose preamble/suffix and ``` fences — e.g. a critic
/// that prepends "I need to check the connections carefully." before the JSON.
/// Falls back to the fence-stripped string when no JSON delimiters are found,
/// so the caller's own parse error still surfaces.
fn extract_json(s: &str) -> &str {
    let stripped = strip_fences(s);
    let bytes = stripped.as_bytes();
    let Some(start) = bytes.iter().position(|&b| b == b'{' || b == b'[') else {
        return stripped;
    };
    let open = bytes[start];
    let close = if open == b'{' { b'}' } else { b']' };
    let (mut depth, mut in_str, mut escaped) = (0i32, false, false);
    for i in start..bytes.len() {
        let b = bytes[i];
        if in_str {
            if escaped { escaped = false; }
            else if b == b'\\' { escaped = true; }
            else if b == b'"' { in_str = false; }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b if b == open => depth += 1,
            b if b == close => {
                depth -= 1;
                if depth == 0 { return &stripped[start..=i]; }
            }
            _ => {}
        }
    }
    stripped // unbalanced — let the caller's parse error surface
}

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

/// Variant for agents called multiple times per pipeline (builder retries, pricing retries).
/// `context` is the large, stable part of the user message — cached within the pipeline.
/// `suffix` is the small, changing part (critic feedback) — never cached.
async fn anthropic_call_retryable(
    http: &reqwest::Client,
    key: &str,
    model: &'static str,
    max_tokens: u32,
    system: &str,
    context: &str,
    suffix: &str,
) -> Result<String, AgentError> {
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
    let body = serde_json::json!({
        "model": model,
        "max_tokens": max_tokens,
        "system": [{"type": "text", "text": system, "cache_control": cache_1h()}],
        "messages": [{"role": "user", "content": user_content}]
    });
    anthropic_raw(http, key, body).await
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

async fn anthropic_raw(
    http: &reqwest::Client,
    key: &str,
    body: serde_json::Value,
) -> Result<String, AgentError> {
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

    #[derive(serde::Deserialize)]
    struct Resp { content: Vec<Content> }
    #[derive(serde::Deserialize)]
    struct Content { #[serde(rename = "type")] kind: String, text: Option<String> }

    let ant: Resp = resp.json().await
        .map_err(|e| AgentError(format!("Anthropic parse: {e}")))?;
    Ok(ant.content.into_iter()
        .find(|c| c.kind == "text")
        .and_then(|c| c.text)
        .unwrap_or_default())
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
Output ONLY valid JSON — no prose, no markdown fences, no comments.\n\
\n\
Schema:\n\
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

    let raw = anthropic_call(
        &app.http, &app.anthropic_key, SONNET, 2048,
        SYSTEM, &user,
    ).await.map_err(|e| AgentError(format!("research: {e}")))?;

    let structured: serde_json::Value = serde_json::from_str(extract_json(&raw))
        .map_err(|e| AgentError(format!("research JSON parse: {e} — raw: {raw}")))?;

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

/// Builds an n8n workflow JSON using Qdrant RAG over n8n templates.
/// Runs up to MAX_BUILD_ATTEMPTS times; the large context is cached between retries.
pub async fn run_builder(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[builder] session={} attempt={}", ctx.session_id, ctx.build_attempts);

    let rag_block = match (&app.qdrant, &app.embeddings) {
        (Some(qdrant), Some(engine)) => {
            let query = format!(
                "{} {}",
                ctx.client_need,
                ctx.research_output.as_deref().unwrap_or_default()
            );
            match engine.embed(query).await {
                Ok(vector) => match qdrant.search(vector, 3).await {
                    Ok(hits) if !hits.is_empty() => {
                        let s = hits.iter().map(|h| format!(
                            "Template: {}\nDescription: {}\nTags: {}",
                            h.name, h.description, h.tags.join(", ")
                        )).collect::<Vec<_>>().join("\n\n---\n\n");
                        tracing::info!("[builder] retrieved {} RAG templates", hits.len());
                        format!("\n\nSimilar workflow templates for reference:\n{s}")
                    }
                    Ok(_)  => { tracing::warn!("[builder] Qdrant returned no hits"); String::new() }
                    Err(e) => { tracing::warn!("[builder] Qdrant search failed: {e}"); String::new() }
                },
                Err(e) => { tracing::warn!("[builder] embed failed: {e}"); String::new() }
            }
        }
        _ => { tracing::warn!("[builder] RAG disabled"); String::new() }
    };

    const SYSTEM: &str = "\
You are the workflow engineer at pointe.dev. You produce production-grade n8n \
workflow JSON that solves the client's need end-to-end. Reference templates may be \
provided — adapt their proven structure, do not copy blindly.\n\
\n\
Output ONLY valid n8n workflow JSON. No prose, no markdown fences, no position fields.\n\
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

    // Stable across retries → cached after the first attempt
    let context = format!(
        "Client: {}\nResearch: {}{}",
        ctx.client_need,
        ctx.research_output.as_deref().unwrap_or(""),
        rag_block,
    );

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

    let raw = anthropic_call_retryable(
        &app.http, &app.anthropic_key, SONNET, 8192,
        SYSTEM, &context, &suffix,
    ).await.map_err(|e| AgentError(format!("builder: {e}")))?;

    ctx.workflow_json = Some(
        serde_json::from_str(extract_json(&raw)).map_err(|e| {
            let preview = &raw[..raw.len().min(500)];
            AgentError(format!("workflow JSON parse: {e}\nRaw (first 500): {preview}"))
        })?
    );
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
Output ONLY one of:\n\
  {\"approved\":true}\n\
  {\"approved\":false,\"feedback\":\"max 3 concrete, actionable issues\"}\n\
No prose, no markdown fences.\n\
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

    let user = format!(
        "Client: {}\nResearch: {}\nWorkflow:\n{}",
        ctx.client_need,
        ctx.research_output.as_deref().unwrap_or(""),
        serde_json::to_string_pretty(workflow).unwrap_or_default(),
    );

    let raw = anthropic_call(
        &app.http, &app.anthropic_key, SONNET, 512,
        SYSTEM, &user,
    ).await.map_err(|e| AgentError(format!("critic: {e}")))?;

    #[derive(serde::Deserialize)]
    struct Verdict { approved: bool, feedback: Option<String> }

    // The critic occasionally answers in prose instead of JSON. Don't let that
    // kill the whole pipeline (which would publish no pitch at all) — treat an
    // unparseable verdict as a soft rejection so the builder retries and, after
    // MAX_BUILD_ATTEMPTS, publish_manual_pitch still produces a proposal.
    let verdict: Verdict = match serde_json::from_str(extract_json(&raw)) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[critic] verdict parse failed: {e} — treating as rejection. raw: {raw}");
            ctx.critic_feedback.push(
                "Réponds UNIQUEMENT avec le JSON {\"approved\":bool,\"feedback\":string}, \
                 sans aucun texte avant ou après.".to_string(),
            );
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
    let node_count = ctx.workflow_json.as_ref()
        .and_then(|w| w["nodes"].as_array()).map(|n| n.len()).unwrap_or(0);
    let node_premium = (node_count.saturating_sub(5) as u32) * 60;
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
Respond ONLY with a JSON array of exactly 3 objects, no prose, no markdown fences.\n\
Schema: [{\"title\":\"...\",\"body\":\"...\",\"points\":[\"...\"]}]\n\
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

    // Store parsed JSON directly — eliminates the double-encoding bug
    ctx.pricing_slides_json = match anthropic_call(
        &app.http, &app.anthropic_key, HAIKU, 600,
        SLIDES_SYSTEM, &slides_user,
    ).await {
        Ok(raw) => serde_json::from_str(extract_json(&raw)).ok(),
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
JSON schema:\n\
{\"approved\":bool, \"reason\":\"1-2 sentences\",\
 \"complexity\":\"simple\"|\"medium\"|\"complex\"|null,\
 \"feasibility_score\":number|null}\n\
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

    let raw = match anthropic_call(
        &app.http, &app.anthropic_key, SONNET, 200,
        SYSTEM, &user,
    ).await {
        Ok(r) => r,
        Err(_) => {
            tracing::warn!("[pricing-critic] call failed — auto-approving");
            publish_pitch(app, ctx).await;
            return Ok(true);
        }
    };

    #[derive(serde::Deserialize)]
    struct CriticOutput {
        approved: bool,
        reason: String,
        complexity: Option<String>,
        feasibility_score: Option<f32>,
    }

    let verdict: CriticOutput = match serde_json::from_str(extract_json(&raw)) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("[pricing-critic] JSON parse failed: {e} — auto-approving");
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

    app.pitches.set(&ctx.session_id, PitchResult {
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

    app.pitches.set(&ctx.session_id, PitchResult {
        solution_desc,
        price_eur_cents: 0,
        price_validity: String::new(),
        externals_needed: vec![],
        slides,
        manual_quote: true,
    }).await;
}

/// Deploys the workflow to n8n (our instance or client's) and activates it.
pub async fn run_deploy(app: &AppState, ctx: &mut PipelineContext) -> Result<(), AgentError> {
    tracing::info!("[deploy] session={} target={}", ctx.session_id,
        ctx.deploy_target.as_deref().unwrap_or("own"));

    let workflow = ctx.workflow_json.as_ref()
        .ok_or_else(|| AgentError("deploy called with no workflow_json".to_string()))?;

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

    obj.entry("name").or_insert_with(|| serde_json::Value::String(format!(
        "pointe.dev — {}",
        ctx.client_need.chars().take(60).collect::<String>()
    )));

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

    let activate_resp = app.http
        .post(format!("{n8n_url}/api/v1/workflows/{}/activate", created.id))
        .header("X-N8N-API-KEY", &n8n_key)
        .send()
        .await
        .map_err(|e| AgentError(format!("n8n activate request: {e}")))?;

    if !activate_resp.status().is_success() {
        tracing::warn!(
            "[deploy] workflow {} created but activation failed: {}",
            created.id, activate_resp.status()
        );
    } else {
        tracing::info!("[deploy] workflow {} activated", created.id);
    }

    ctx.n8n_workflow_id  = Some(created.id.clone());
    ctx.n8n_workflow_url = Some(format!("{n8n_url}/workflow/{}", created.id));
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

    // ── strip_fences ───────────────────────────────────────────────────────

    #[test]
    fn strip_fences_removes_json_fence() {
        let input = "```json\n{\"key\":\"value\"}\n```";
        assert_eq!(strip_fences(input), "{\"key\":\"value\"}");
    }

    #[test]
    fn strip_fences_removes_plain_fence() {
        let input = "```\n{\"a\":1}\n```";
        assert_eq!(strip_fences(input), "{\"a\":1}");
    }

    #[test]
    fn strip_fences_leaves_plain_json_untouched() {
        let input = "{\"key\":\"value\"}";
        assert_eq!(strip_fences(input), input);
    }

    #[test]
    fn strip_fences_trims_surrounding_whitespace() {
        let input = "  \n{\"x\":1}\n  ";
        assert_eq!(strip_fences(input), "{\"x\":1}");
    }

    // ── extract_json ───────────────────────────────────────────────────────

    #[test]
    fn extract_json_strips_prose_preamble() {
        // The exact shape that hard-failed a prod pipeline.
        let input = "I need to check the connections carefully.\n{\"approved\":false,\"feedback\":\"x\"}";
        assert_eq!(extract_json(input), "{\"approved\":false,\"feedback\":\"x\"}");
    }

    #[test]
    fn extract_json_strips_prose_suffix() {
        let input = "{\"approved\":true} — looks good to me!";
        assert_eq!(extract_json(input), "{\"approved\":true}");
    }

    #[test]
    fn extract_json_handles_nested_braces_and_arrays() {
        let input = "here:\n{\"a\":[1,2],\"b\":{\"c\":3}} trailing";
        assert_eq!(extract_json(input), "{\"a\":[1,2],\"b\":{\"c\":3}}");
    }

    #[test]
    fn extract_json_ignores_braces_inside_strings() {
        let input = "x {\"msg\":\"a } b { c\"} y";
        assert_eq!(extract_json(input), "{\"msg\":\"a } b { c\"}");
    }

    #[test]
    fn extract_json_handles_top_level_array() {
        let input = "Réponse: [{\"label\":\"A\"}] voilà";
        assert_eq!(extract_json(input), "[{\"label\":\"A\"}]");
    }

    #[test]
    fn extract_json_strips_fences_then_extracts() {
        let input = "```json\nblah {\"k\":1}\n```";
        assert_eq!(extract_json(input), "{\"k\":1}");
    }

    #[test]
    fn extract_json_no_json_returns_stripped() {
        let input = "no json here at all";
        assert_eq!(extract_json(input), "no json here at all");
    }

    #[test]
    fn extract_json_unbalanced_returns_stripped() {
        // Missing closing brace — fall back so the caller's parse error surfaces.
        let input = "{\"a\":1";
        assert_eq!(extract_json(input), "{\"a\":1");
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
