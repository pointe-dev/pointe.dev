use reqwest::Client;
use serde::{Deserialize, Serialize};

const MODEL: &str = "text-embedding-3-small";
pub const VECTOR_DIM: usize = 1536;

#[derive(Serialize)]
struct EmbedReq<'a> {
    input: &'a str,
    model: &'static str,
}

#[derive(Deserialize)]
struct EmbedResp {
    data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
    embedding: Vec<f32>,
}

/// Embeds `text` with OpenAI text-embedding-3-small (1536 dims).
pub async fn embed(http: &Client, key: &str, text: &str) -> Result<Vec<f32>, String> {
    let resp = http
        .post("https://api.openai.com/v1/embeddings")
        .bearer_auth(key)
        .json(&EmbedReq { input: text, model: MODEL })
        .send()
        .await
        .map_err(|e| format!("embed request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("OpenAI embed {status}: {body}"));
    }

    let data: EmbedResp = resp.json().await.map_err(|e| format!("embed parse: {e}"))?;
    data.data.into_iter()
        .next()
        .map(|d| d.embedding)
        .ok_or_else(|| "empty embedding response".to_string())
}
