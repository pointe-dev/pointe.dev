//! Minimal n8n MCP client (Streamable HTTP, JSON-RPC).
//!
//! Gated on `N8N_MCP_URL` + `N8N_MCP_TOKEN`: absent → `from_env` returns None and
//! the builder/critic/designer fall back to their ungrounded single-shot prompts.
//!
//! Why a hand-rolled client and not the Anthropic Messages-API MCP connector:
//! the hosted connector cannot complete its handshake with the n8n MCP server
//! (generic "Error while communicating with MCP server") even though the server
//! is reachable and valid. The backend, however, talks to it cleanly over raw
//! JSON-RPC — so we expose the catalogue tools to the model as ordinary
//! client-side tools and proxy `tools/call` here. The server is stateless: a
//! single POST per call, no `initialize` handshake or session id required.
//!
//! The token is a secret (a long-lived n8n MCP JWT) — it lives in the environment
//! (`N8N_MCP_TOKEN`), never in the repo.

use serde_json::{json, Value};

#[derive(Clone)]
pub struct N8nMcpConfig {
    url: String,
    token: String,
}

impl N8nMcpConfig {
    /// Reads the n8n MCP server URL + token from the environment. Both must be
    /// present and non-empty, or grounding stays disabled (returns None).
    pub fn from_env() -> Option<Self> {
        let url = std::env::var("N8N_MCP_URL").ok().filter(|s| !s.is_empty())?;
        let token = std::env::var("N8N_MCP_TOKEN").ok().filter(|s| !s.is_empty())?;
        Some(Self { url, token })
    }

    #[cfg(test)]
    pub fn new(url: impl Into<String>, token: impl Into<String>) -> Self {
        Self { url: url.into(), token: token.into() }
    }

    /// Calls one MCP tool via JSON-RPC `tools/call` and returns the concatenated
    /// text of its result content. Errors (transport, JSON-RPC error, tool error)
    /// come back as `Err(String)` so the caller can hand them to the model as a
    /// `tool_result` with `is_error: true` rather than killing the pipeline.
    pub async fn call_tool(
        &self,
        http: &reqwest::Client,
        name: &str,
        arguments: Value,
    ) -> Result<String, String> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments },
        });
        let resp = http
            .post(&self.url)
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            // The server is strict Streamable HTTP — it 406s unless the client
            // accepts text/event-stream, which is also how it frames the reply.
            .header("Accept", "application/json, text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("n8n MCP request: {e}"))?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(format!("n8n MCP {name} → HTTP {s}: {}", b.chars().take(200).collect::<String>()));
        }

        let text = resp.text().await.map_err(|e| format!("n8n MCP read: {e}"))?;
        parse_jsonrpc_result(&text)
    }

    /// Anthropic tool definitions for the catalogue tools named in `allowed`.
    /// The model calls these like any client-side tool; we execute them via
    /// `call_tool`. Default-deny: only the named tools are exposed (the write/
    /// deploy tools the server also offers are never surfaced).
    pub fn grounding_tools(&self, allowed: &[&str]) -> Vec<Value> {
        allowed.iter().filter_map(|t| tool_def(t)).collect()
    }

    /// Validates n8n Workflow SDK `code` via the MCP `validate_workflow` tool.
    /// Ok(()) when the code parses to a valid workflow; Err(feedback) otherwise,
    /// with the parser diagnostics so the builder can fix and retry. Used as the
    /// structural gate in the code-authoring build path.
    pub async fn validate_code(&self, http: &reqwest::Client, code: &str) -> Result<(), String> {
        match self.call_tool(http, "validate_workflow", json!({ "code": code })).await {
            Ok(text) => parse_validate_result(&text),
            // An invalid parse comes back as an MCP tool error; its text is the diagnostic.
            Err(diag) => Err(diag),
        }
    }

    /// Creates a workflow from validated SDK `code` via `create_workflow_from_code`,
    /// optionally inside a project/folder, and returns the new workflow id.
    pub async fn create_from_code(
        &self,
        http: &reqwest::Client,
        code: &str,
        name: &str,
        description: &str,
        project_id: Option<&str>,
        folder_id: Option<&str>,
    ) -> Result<String, String> {
        let mut args = json!({ "code": code, "name": name, "description": description });
        if let Some(p) = project_id { args["projectId"] = json!(p); }
        if let Some(f) = folder_id { args["folderId"] = json!(f); }
        let text = self.call_tool(http, "create_workflow_from_code", args).await?;
        parse_create_result(&text)
    }

    /// Resolves a folder NAME (e.g. `N8N_TEST_FOLDER`) to the `(projectId, folderId)`
    /// `create_from_code` needs, via the MCP catalogue. Uses the first (personal)
    /// project and the exact-name folder match. Err on any lookup failure.
    pub async fn resolve_folder(
        &self,
        http: &reqwest::Client,
        folder_name: &str,
    ) -> Result<(String, String), String> {
        let projects = self.call_tool(http, "search_projects", json!({})).await?;
        let pv: Value = serde_json::from_str(&projects).unwrap_or(Value::Null);
        let project_id = pv["data"][0]["id"].as_str()
            .ok_or_else(|| format!("resolve_folder: no project in {projects}"))?
            .to_string();

        let folders = self.call_tool(http, "search_folders",
            json!({ "projectId": project_id, "query": folder_name })).await?;
        let fv: Value = serde_json::from_str(&folders).unwrap_or(Value::Null);
        let folder_id = fv["data"].as_array()
            .and_then(|a| a.iter()
                .find(|f| f["name"].as_str() == Some(folder_name))
                .or_else(|| a.first()))
            .and_then(|f| f["id"].as_str())
            .ok_or_else(|| format!("resolve_folder: folder '{folder_name}' not found in {folders}"))?
            .to_string();

        Ok((project_id, folder_id))
    }
}

/// Pulls the verdict out of `validate_workflow`'s JSON result text.
fn parse_validate_result(text: &str) -> Result<(), String> {
    let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    if v["valid"].as_bool() == Some(true) {
        return Ok(());
    }
    let errs = v["errors"].as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("; "))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| text.to_string());
    Err(errs)
}

/// Pulls the new workflow id out of `create_workflow_from_code`'s JSON result text.
fn parse_create_result(text: &str) -> Result<String, String> {
    let v: Value = serde_json::from_str(text)
        .map_err(|e| format!("create_from_code: unparseable result ({e}): {text}"))?;
    v["workflowId"].as_str().map(str::to_string)
        .ok_or_else(|| format!("create_from_code: no workflowId in result: {text}"))
}

/// Pulls the JSON-RPC result text out of the server's reply. The server answers
/// with an SSE frame (`event: message` / `data: {...}`) — we scan for the first
/// `data:` line that parses to a JSON-RPC envelope and flatten its `result`.
fn parse_jsonrpc_result(raw: &str) -> Result<String, String> {
    for line in raw.lines() {
        let line = line.trim_start();
        let payload = line.strip_prefix("data:").map(str::trim).unwrap_or(line);
        let Ok(v) = serde_json::from_str::<Value>(payload) else { continue };
        if let Some(err) = v.get("error") {
            return Err(format!("JSON-RPC error: {err}"));
        }
        let Some(result) = v.get("result") else { continue };
        // result.content is an array of blocks; concatenate the text ones.
        let text: String = result["content"]
            .as_array()
            .map(|blocks| {
                blocks.iter()
                    .filter_map(|b| b.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if result["isError"].as_bool() == Some(true) {
            return Err(format!("tool reported error: {}", text.chars().take(300).collect::<String>()));
        }
        return Ok(text);
    }
    Err("no JSON-RPC result in MCP response".to_string())
}

/// Anthropic tool definition for a known n8n catalogue tool, or None if unknown.
fn tool_def(name: &str) -> Option<Value> {
    let (description, schema) = match name {
        "search_nodes" => (
            "Search the live n8n node catalogue by service name, trigger type, or utility \
             function. Returns the REAL node type ids, versions, and discriminators. Call \
             this to confirm a node exists and to get its exact type string before using it.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "queries": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Search queries, e.g. [\"gmail\", \"schedule trigger\", \"set\"]"
                    }
                },
                "required": ["queries"]
            }),
        ),
        "get_node_types" => (
            "Get the exact n8n node type definitions: real type id, typeVersion, and the \
             precise parameter names a node accepts. Call this before emitting a node so its \
             type/version/parameters are correct — never guess parameter names.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "nodeIds": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Node type ids to define, e.g. [\"n8n-nodes-base.scheduleTrigger\", \"n8n-nodes-base.set\"]"
                    }
                },
                "required": ["nodeIds"]
            }),
        ),
        "get_suggested_nodes" => (
            "Get recommended n8n nodes for one or more workflow technique categories \
             (e.g. scheduling, scraping_and_research, content_generation, triage, \
             data_transformation, notification). Use it to discover the right nodes for a need.",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "categories": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Technique categories, e.g. [\"scheduling\", \"content_generation\"]"
                    }
                },
                "required": ["categories"]
            }),
        ),
        _ => return None,
    };
    Some(json!({ "name": name, "description": description, "input_schema": schema }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_none_when_unconfigured() {
        let prev_url = std::env::var("N8N_MCP_URL").ok();
        let prev_tok = std::env::var("N8N_MCP_TOKEN").ok();
        std::env::remove_var("N8N_MCP_URL");
        std::env::remove_var("N8N_MCP_TOKEN");
        assert!(N8nMcpConfig::from_env().is_none());
        if let Some(v) = prev_url { std::env::set_var("N8N_MCP_URL", v); }
        if let Some(v) = prev_tok { std::env::set_var("N8N_MCP_TOKEN", v); }
    }

    #[test]
    fn grounding_tools_is_default_deny_with_allowlist() {
        let cfg = N8nMcpConfig::new("https://x", "t");
        let tools = cfg.grounding_tools(&["search_nodes", "get_node_types"]);
        assert_eq!(tools.len(), 2);
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&"search_nodes"));
        assert!(names.contains(&"get_node_types"));
        // Each is a well-formed Anthropic tool def.
        for t in &tools {
            assert!(t["description"].as_str().is_some());
            assert_eq!(t["input_schema"]["type"], "object");
        }
        // An unknown / non-allowlisted tool is never produced.
        assert!(cfg.grounding_tools(&["create_workflow_from_code"]).is_empty());
    }

    #[test]
    fn parse_jsonrpc_result_flattens_sse_text() {
        let raw = "event: message\ndata: {\"result\":{\"content\":[{\"type\":\"text\",\"text\":\"hello\"},{\"type\":\"text\",\"text\":\"world\"}]},\"jsonrpc\":\"2.0\",\"id\":1}\n";
        assert_eq!(parse_jsonrpc_result(raw).unwrap(), "hello\nworld");
    }

    #[test]
    fn parse_validate_result_ok_when_valid() {
        assert!(parse_validate_result(r#"{"valid":true,"nodeCount":3}"#).is_ok());
    }

    #[test]
    fn parse_validate_result_returns_joined_errors_when_invalid() {
        let err = parse_validate_result(r#"{"valid":false,"errors":["unbalanced brackets","unknown id 'x'"]}"#)
            .unwrap_err();
        assert!(err.contains("unbalanced brackets") && err.contains("unknown id 'x'"));
    }

    #[test]
    fn parse_create_result_extracts_workflow_id() {
        assert_eq!(
            parse_create_result(r#"{"workflowId":"abc123","name":"x","nodeCount":2}"#).unwrap(),
            "abc123"
        );
        assert!(parse_create_result(r#"{"name":"x"}"#).is_err());
    }

    #[test]
    fn parse_jsonrpc_result_surfaces_errors() {
        let rpc_err = "data: {\"error\":{\"code\":-32601,\"message\":\"no such tool\"},\"jsonrpc\":\"2.0\",\"id\":1}";
        assert!(parse_jsonrpc_result(rpc_err).is_err());
        let tool_err = "data: {\"result\":{\"isError\":true,\"content\":[{\"type\":\"text\",\"text\":\"bad args\"}]},\"jsonrpc\":\"2.0\",\"id\":1}";
        assert!(parse_jsonrpc_result(tool_err).is_err());
        assert!(parse_jsonrpc_result("event: ping\n").is_err());
    }
}
