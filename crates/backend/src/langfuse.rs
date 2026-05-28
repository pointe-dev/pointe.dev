use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub struct LangfuseClient {
    http: Client,
    base_url: String,
    public_key: String,
    secret_key: String,
    pub prompt_name: String,
    pub prompt_version: u32,
}

#[derive(Deserialize)]
struct PromptResp {
    version: u32,
    #[serde(rename = "type")]
    kind: String,
    prompt: Value,
}

impl LangfuseClient {
    pub fn new(http: Client, base_url: String, public_key: String, secret_key: String) -> Self {
        Self { http, base_url, public_key, secret_key, prompt_name: String::new(), prompt_version: 0 }
    }

    pub async fn fetch_prompt(&mut self, name: &str) -> Result<String, String> {
        let url = format!("{}/api/public/prompts?name={}", self.base_url, name);
        let resp = self.http
            .get(&url)
            .basic_auth(&self.public_key, Some(&self.secret_key))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Langfuse {status}: {body}"));
        }

        let pr: PromptResp = resp.json().await.map_err(|e| e.to_string())?;
        self.prompt_version = pr.version;
        self.prompt_name = name.to_string();

        match pr.kind.as_str() {
            "text" => pr.prompt.as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| "prompt field is not a string".to_string()),
            "chat" => pr.prompt.as_array()
                .and_then(|msgs| msgs.iter().find(|m| m["role"] == "system"))
                .and_then(|m| m["content"].as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| "no system message in chat prompt".to_string()),
            t => Err(format!("unknown prompt type: {t}")),
        }
    }

    pub async fn trace(
        &self,
        user_input: &str,
        output: &str,
        model: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) {
        let trace_id = Uuid::new_v4().to_string();
        let gen_id = Uuid::new_v4().to_string();
        let start_iso = start.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let end_iso = end.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let payload = json!({
            "batch": [
                {
                    "id": Uuid::new_v4().to_string(),
                    "type": "trace-create",
                    "timestamp": &end_iso,
                    "body": {
                        "id": &trace_id,
                        "name": "qualifier-chat",
                        "input": user_input,
                        "output": output,
                    }
                },
                {
                    "id": Uuid::new_v4().to_string(),
                    "type": "generation-create",
                    "timestamp": &end_iso,
                    "body": {
                        "id": &gen_id,
                        "traceId": &trace_id,
                        "name": "openrouter-completion",
                        "model": model,
                        "startTime": &start_iso,
                        "endTime": &end_iso,
                        "input": [{"role": "user", "content": user_input}],
                        "output": {"role": "assistant", "content": output},
                        "promptName": &self.prompt_name,
                        "promptVersion": self.prompt_version,
                    }
                }
            ]
        });

        let url = format!("{}/api/public/ingestion", self.base_url);
        match self.http
            .post(&url)
            .basic_auth(&self.public_key, Some(&self.secret_key))
            .json(&payload)
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => tracing::debug!("Langfuse trace sent"),
            Ok(r) => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                tracing::warn!("Langfuse trace error {status}: {body}");
            }
            Err(e) => tracing::warn!("Langfuse trace failed: {e}"),
        }
    }
}
