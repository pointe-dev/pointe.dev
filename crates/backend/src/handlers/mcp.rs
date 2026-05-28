/// MCP server — Streamable HTTP transport (JSON-RPC 2.0)
///
/// Clients configure this in their Claude Code / Cursor / Cline:
///   { "type": "http", "url": "https://api.pointe.dev/mcp",
///     "headers": { "X-Pointe-Key": "<pipeline_id>" } }
///
/// The X-Pointe-Key is the pipeline_id UUID returned when a workflow was built.
/// It is used to resolve the correct n8n instance (ours or the client's own).
use axum::{extract::State, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::pipeline::PipelineStore;
use crate::state::AppState;

// ── JSON-RPC 2.0 types ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RpcRequest {
    #[allow(dead_code)]
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
pub struct RpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcError>,
}

#[derive(Serialize)]
struct RpcError {
    code: i32,
    message: String,
}

fn ok(id: Option<Value>, result: Value) -> Json<RpcResponse> {
    Json(RpcResponse { jsonrpc: "2.0", id, result: Some(result), error: None })
}

fn rpc_err(id: Option<Value>, code: i32, msg: impl Into<String>) -> Json<RpcResponse> {
    Json(RpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(RpcError { code, message: msg.into() }),
    })
}

fn text(s: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": s.into() }] })
}

// ── Auth helper ───────────────────────────────────────────────────────────────

/// Resolves (n8n_url, n8n_api_key) from a pipeline context.
/// Falls back to env vars when deploy_target = "own".
async fn n8n_creds(pipelines: &PipelineStore, pid: Uuid) -> Option<(String, String)> {
    let ctx = pipelines.get_ctx(pid).await?;
    match ctx.deploy_target.as_deref().unwrap_or("own") {
        "client" => Some((ctx.client_n8n_url?, ctx.client_n8n_key?)),
        _ => Some((
            std::env::var("N8N_URL").ok()?,
            std::env::var("N8N_API_KEY").ok()?,
        )),
    }
}

fn parse_key(headers: &HeaderMap) -> Option<Uuid> {
    headers.get("x-pointe-key")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
}

// ── Tool descriptors ──────────────────────────────────────────────────────────

fn tools_list() -> Value {
    json!({ "tools": [
        {
            "name": "list_workflows",
            "description": "List all automation workflows deployed by pointe.dev for this client.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "trigger_workflow",
            "description": "Manually trigger one of your automation workflows with optional input data.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workflow_id": { "type": "string", "description": "n8n workflow ID" },
                    "data": { "type": "object", "description": "Optional JSON payload forwarded to the workflow's webhook" }
                },
                "required": ["workflow_id"]
            }
        },
        {
            "name": "get_executions",
            "description": "Get recent execution history for your workflows (status, timestamps, errors).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workflow_id": { "type": "string", "description": "Filter by workflow ID (optional)" },
                    "limit": { "type": "integer", "description": "Number of results (default 10, max 25)" }
                }
            }
        },
        {
            "name": "request_automation",
            "description": "Request a new automation from pointe.dev. Describe the process and we will research, build, validate, price, and deploy it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "description": { "type": "string", "description": "Plain-language description of the process to automate" }
                },
                "required": ["description"]
            }
        },
        {
            "name": "get_pipeline_status",
            "description": "Track the build progress of an automation pipeline.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pipeline_id": { "type": "string", "description": "Pipeline ID returned by request_automation" }
                },
                "required": ["pipeline_id"]
            }
        }
    ]})
}

// ── Tool handlers ─────────────────────────────────────────────────────────────

async fn tool_list_workflows(state: &AppState, pid: Uuid) -> Result<Value, String> {
    let (n8n_url, n8n_key) = n8n_creds(&state.pipelines, pid).await
        .ok_or("Pipeline not found or n8n credentials missing")?;

    let resp = state.http
        .get(format!("{n8n_url}/api/v1/workflows"))
        .header("X-N8N-API-KEY", &n8n_key)
        .send().await.map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("n8n error: {}", resp.status()));
    }
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    let lines: Vec<String> = body["data"].as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|w| format!(
            "• {} — id: {} ({})",
            w["name"].as_str().unwrap_or("unnamed"),
            w["id"].as_str().unwrap_or("?"),
            if w["active"].as_bool().unwrap_or(false) { "active" } else { "inactive" },
        ))
        .collect();

    Ok(text(if lines.is_empty() {
        "No workflows found.".to_string()
    } else {
        lines.join("\n")
    }))
}

async fn tool_trigger_workflow(state: &AppState, pid: Uuid, args: &Value) -> Result<Value, String> {
    let (n8n_url, n8n_key) = n8n_creds(&state.pipelines, pid).await
        .ok_or("Pipeline not found or n8n credentials missing")?;

    let wf_id = args["workflow_id"].as_str().ok_or("workflow_id required")?;
    let data = &args["data"];

    // Fetch workflow to find its webhook node path
    let wf_resp = state.http
        .get(format!("{n8n_url}/api/v1/workflows/{wf_id}"))
        .header("X-N8N-API-KEY", &n8n_key)
        .send().await.map_err(|e| e.to_string())?;

    if !wf_resp.status().is_success() {
        return Err(format!("Workflow {wf_id} not found: {}", wf_resp.status()));
    }
    let wf: Value = wf_resp.json().await.map_err(|e| e.to_string())?;
    let wf_name = wf["name"].as_str().unwrap_or("unnamed").to_string();

    let webhook_path = wf["nodes"].as_array()
        .and_then(|nodes| nodes.iter().find(|n|
            n["type"].as_str() == Some("n8n-nodes-base.webhook")
        ))
        .and_then(|n| n["parameters"]["path"].as_str())
        .map(|p| p.to_string());

    let Some(path) = webhook_path else {
        return Ok(text(format!(
            "Workflow \"{wf_name}\" has no webhook trigger. \
             Activate it manually in n8n or add a Webhook node as the trigger."
        )));
    };

    let webhook_url = format!("{n8n_url}/webhook/{path}");
    let req = if data.is_object() {
        state.http.post(&webhook_url).json(data)
    } else {
        state.http.post(&webhook_url)
    };

    let trigger = req.send().await.map_err(|e| format!("Webhook call failed: {e}"))?;
    Ok(text(format!(
        "✓ Triggered \"{wf_name}\" — HTTP {}\nWebhook: {webhook_url}",
        trigger.status()
    )))
}

async fn tool_get_executions(state: &AppState, pid: Uuid, args: &Value) -> Result<Value, String> {
    let (n8n_url, n8n_key) = n8n_creds(&state.pipelines, pid).await
        .ok_or("Pipeline not found or n8n credentials missing")?;

    let limit = args["limit"].as_u64().unwrap_or(10).min(25);
    let mut url = format!("{n8n_url}/api/v1/executions?limit={limit}&includeData=false");
    if let Some(wid) = args["workflow_id"].as_str() {
        url.push_str(&format!("&workflowId={wid}"));
    }

    let resp = state.http.get(&url)
        .header("X-N8N-API-KEY", &n8n_key)
        .send().await.map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("n8n error: {}", resp.status()));
    }
    let body: Value = resp.json().await.map_err(|e| e.to_string())?;
    let lines: Vec<String> = body["data"].as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|e| {
            let status = e["status"].as_str().unwrap_or("unknown");
            let icon = match status { "success" => "✓", "error" => "✗", _ => "·" };
            format!(
                "{icon} [{status}] {} — workflow: {} (exec id: {})",
                e["startedAt"].as_str().unwrap_or("?"),
                e["workflowId"].as_str().unwrap_or("?"),
                e["id"].as_str().unwrap_or("?"),
            )
        })
        .collect();

    Ok(text(if lines.is_empty() {
        "No executions found.".to_string()
    } else {
        lines.join("\n")
    }))
}

async fn tool_request_automation(app: Arc<AppState>, key_pid: Option<Uuid>, args: &Value) -> Result<Value, String> {
    let description = args["description"].as_str().ok_or("description required")?.to_string();
    let session_id = key_pid
        .map(|p| p.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let new_id = app.pipelines.create(session_id, description.clone(), None).await;
    crate::pipeline::spawn(new_id, app.pipelines.clone(), app);

    Ok(text(format!(
        "Pipeline started — ID: {new_id}\n\n\
         Track progress with get_pipeline_status.\n\
         Stages: qualifying → researching → building → validating → pricing → awaiting_payment → live\n\n\
         Request: {description}"
    )))
}

async fn tool_get_pipeline_status(pipelines: &PipelineStore, args: &Value) -> Result<Value, String> {
    let pid_str = args["pipeline_id"].as_str().ok_or("pipeline_id required")?;
    let pid = Uuid::parse_str(pid_str).map_err(|_| "Invalid pipeline_id")?;

    let guard = pipelines.0.read().await;
    let record = guard.get(&pid).ok_or("Pipeline not found")?;
    let stage = serde_json::to_value(&record.stage).unwrap_or_default();
    let stage_name = stage["stage"].as_str().unwrap_or("unknown");

    let mut lines = vec![
        format!("Pipeline: {pid}"),
        format!("Stage: {stage_name}"),
        format!("Updated: {}", record.updated_at.to_rfc3339()),
    ];

    if let Some(price) = record.ctx.price_quote {
        lines.push(format!("Setup: €{price}"));
    }
    if let Some(monthly) = record.ctx.price_monthly {
        lines.push(format!("Monthly: €{monthly}/mo"));
    }
    if let Some(justification) = &record.ctx.price_justification {
        lines.push(format!("Justification: {justification}"));
    }
    if let Some(wf_url) = &record.ctx.n8n_workflow_url {
        lines.push(format!("Workflow URL: {wf_url}"));
    }
    if let (Some(reason),) = (stage["reason"].as_str(),) {
        lines.push(format!("Reason: {reason}"));
    }

    Ok(text(lines.join("\n")))
}

// ── Main handler ──────────────────────────────────────────────────────────────

pub async fn handle(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<RpcRequest>,
) -> Json<RpcResponse> {
    let id = req.id.clone();
    let key_pid = parse_key(&headers);

    match req.method.as_str() {
        "initialize" => ok(id, json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": { "name": "pointe.dev", "version": "1.0.0" },
            "capabilities": { "tools": {} }
        })),

        "ping" => ok(id, json!({})),

        // Notifications need no response body
        m if m.starts_with("notifications/") => ok(None, json!(null)),

        "tools/list" => ok(id, tools_list()),

        "tools/call" => {
            let tool = req.params["name"].as_str().unwrap_or("").to_string();
            let args = &req.params["arguments"];

            // Tools that need n8n auth require a valid key
            let requires_n8n = matches!(
                tool.as_str(),
                "list_workflows" | "trigger_workflow" | "get_executions"
            );
            if requires_n8n && key_pid.is_none() {
                return rpc_err(id, -32001, "X-Pointe-Key header required (pipeline UUID)");
            }

            let result = match tool.as_str() {
                "list_workflows" => tool_list_workflows(&state, key_pid.unwrap()).await,
                "trigger_workflow" => tool_trigger_workflow(&state, key_pid.unwrap(), args).await,
                "get_executions" => tool_get_executions(&state, key_pid.unwrap(), args).await,
                "request_automation" => tool_request_automation(state.clone(), key_pid, args).await,
                "get_pipeline_status" => tool_get_pipeline_status(&state.pipelines, args).await,
                _ => Err(format!("Unknown tool: {tool}")),
            };

            match result {
                Ok(v) => ok(id, v),
                Err(e) => rpc_err(id, -32003, e),
            }
        }

        other => rpc_err(id, -32601, format!("Method not found: {other}")),
    }
}
