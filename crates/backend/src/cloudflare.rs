//! Managed RAG on Cloudflare: Workers AI for embeddings, Vectorize for vector
//! storage. Replaces the local fastembed + Qdrant stack, which OOM-crashed the
//! 4 GB VPS (see the Qdrant-disabled note) by moving 100% of the RAG off-box.
//!
//! Gated on `CF_ACCOUNT_ID` + `CF_API_TOKEN`: absent → `from_env` returns None and
//! the builder keeps its current stub behaviour, exactly like the old `QDRANT_URL`
//! gate. This module is the embeddings half; Vectorize upsert/query follow.

use reqwest::Client;
use serde::{Deserialize, Serialize};

/// bge-m3 on Workers AI: multilingual (FR/EN/DE…), retrieval-grade, 1024-dim.
/// Unlike e5-large (the old local model), bge-m3 takes NO `query:`/`passage:`
/// prefix — feed raw text on both the ingest and the query side.
const EMBED_MODEL: &str = "@cf/baai/bge-m3";

/// Embedding width. Must match the Vectorize index `--dimensions`.
pub const VECTOR_DIM: usize = 1024;

#[derive(Clone)]
pub struct CloudflareRag {
    http: Client,
    account_id: String,
    api_token: String,
    /// Vectorize index name (`pointe-rag` by default).
    index: String,
}

impl CloudflareRag {
    /// Builds from env, or None if unconfigured (RAG stays off — no regression).
    /// Reuses the process-wide reqwest client, like the Anthropic and Qdrant callers.
    pub fn from_env(http: Client) -> Option<Self> {
        let account_id = std::env::var("CF_ACCOUNT_ID").ok()?;
        let api_token = std::env::var("CF_API_TOKEN").ok()?;
        let index = std::env::var("CF_VECTORIZE_INDEX")
            .unwrap_or_else(|_| "pointe-rag".to_string());
        Some(Self { http, account_id, api_token, index })
    }

    /// Embeds one text → a 1024-d vector via Workers AI.
    pub async fn embed(&self, text: String) -> Result<Vec<f32>, String> {
        let mut rows = self.embed_batch(vec![text]).await?;
        if rows.is_empty() {
            return Err("Workers AI returned no embedding".into());
        }
        Ok(rows.remove(0))
    }

    /// Embeds a batch in a single call. bge-m3 accepts `text` as an array.
    pub async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, String> {
        #[derive(serde::Serialize)]
        struct Req {
            text: Vec<String>,
        }

        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/ai/run/{EMBED_MODEL}",
            self.account_id
        );

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.api_token)
            .json(&Req { text: texts })
            .send()
            .await
            .map_err(|e| format!("Workers AI request: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Workers AI {status}: {body}"));
        }

        let parsed: EmbedResp = resp
            .json()
            .await
            .map_err(|e| format!("Workers AI parse: {e}"))?;
        parse_embeddings(parsed)
    }

    fn vectorize_url(&self, op: &str) -> String {
        format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/vectorize/v2/indexes/{}/{op}",
            self.account_id, self.index
        )
    }

    /// Upserts templates into the index. Each item is `(id, vector, metadata)`.
    /// The body is NDJSON — one `{id, values, metadata}` object per line.
    pub async fn upsert(&self, items: Vec<(String, Vec<f32>, TemplateDoc)>) -> Result<usize, String> {
        let n = items.len();
        let mut body = String::new();
        for (id, values, meta) in items {
            let line = serde_json::json!({ "id": id, "values": values, "metadata": meta });
            body.push_str(&serde_json::to_string(&line).map_err(|e| e.to_string())?);
            body.push('\n');
        }

        let resp = self
            .http
            .post(self.vectorize_url("upsert"))
            .bearer_auth(&self.api_token)
            .header(reqwest::header::CONTENT_TYPE, "application/x-ndjson")
            .body(body)
            .send()
            .await
            .map_err(|e| format!("Vectorize upsert request: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(format!("Vectorize upsert {status}: {b}"));
        }

        tracing::info!("[cloudflare] upserted {n} templates into '{}'", self.index);
        Ok(n)
    }

    /// Returns the `top_k` templates nearest to `query_vector`, optionally filtered
    /// to a language (requires a metadata index on `lang`, created via wrangler).
    pub async fn query(
        &self,
        query_vector: Vec<f32>,
        top_k: u32,
        lang: Option<&str>,
    ) -> Result<Vec<TemplateDoc>, String> {
        let mut body = serde_json::json!({
            "vector": query_vector,
            "topK": top_k,
            "returnMetadata": "all",
        });
        if let Some(l) = lang {
            body["filter"] = serde_json::json!({ "lang": l });
        }

        let resp = self
            .http
            .post(self.vectorize_url("query"))
            .bearer_auth(&self.api_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Vectorize query request: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(format!("Vectorize query {status}: {b}"));
        }

        let parsed: QueryResp = resp
            .json()
            .await
            .map_err(|e| format!("Vectorize query parse: {e}"))?;
        parse_matches(parsed)
    }
}

/// A retrievable template: lightweight, searchable fields only. The full n8n
/// `workflow_json` is intentionally NOT stored here — the builder uses only
/// name/description/tags today, and Vectorize caps metadata near 10 KiB per
/// vector. Feeding full templates to the builder is a separate later enhancement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemplateDoc {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub lang: String,
    pub source: String,
}

/// Vectorize query response: `{ "result": { "matches": [{ "metadata": {..} }] } }`.
#[derive(Deserialize)]
struct QueryResp {
    success: bool,
    result: Option<QueryResult>,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct QueryResult {
    matches: Vec<Match>,
}

#[derive(Deserialize)]
struct Match {
    metadata: Option<TemplateDoc>,
    #[allow(dead_code)]
    score: Option<f32>,
}

fn parse_matches(r: QueryResp) -> Result<Vec<TemplateDoc>, String> {
    if !r.success {
        return Err(format!("Vectorize query unsuccessful: {:?}", r.errors));
    }
    let matches = r.result.ok_or("Vectorize query: missing result")?.matches;
    Ok(matches.into_iter().filter_map(|m| m.metadata).collect())
}

/// Workers AI embedding response: `{ "result": { "data": [[..]] }, "success": true }`.
#[derive(Deserialize)]
struct EmbedResp {
    success: bool,
    result: Option<EmbedResult>,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct EmbedResult {
    data: Vec<Vec<f32>>,
}

/// Pulls the vectors out of a response, validating success and dimensions.
fn parse_embeddings(r: EmbedResp) -> Result<Vec<Vec<f32>>, String> {
    if !r.success {
        return Err(format!("Workers AI unsuccessful: {:?}", r.errors));
    }
    let data = r.result.ok_or("Workers AI: missing result")?.data;
    for (i, row) in data.iter().enumerate() {
        if row.len() != VECTOR_DIM {
            return Err(format!(
                "Workers AI: row {i} has {} dims, expected {VECTOR_DIM}",
                row.len()
            ));
        }
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resp_from(json: serde_json::Value) -> EmbedResp {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn parses_a_well_formed_batch() {
        let row: Vec<f32> = vec![0.1; VECTOR_DIM];
        let json = serde_json::json!({
            "success": true,
            "result": { "shape": [2, VECTOR_DIM], "data": [row, vec![0.2_f32; VECTOR_DIM]] },
            "errors": []
        });
        let out = parse_embeddings(resp_from(json)).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].len(), VECTOR_DIM);
    }

    #[test]
    fn rejects_unsuccessful_response() {
        let json = serde_json::json!({
            "success": false,
            "result": null,
            "errors": [{ "code": 1001, "message": "model not found" }]
        });
        assert!(parse_embeddings(resp_from(json)).is_err());
    }

    #[test]
    fn rejects_wrong_dimension() {
        let json = serde_json::json!({
            "success": true,
            "result": { "data": [[0.1_f32, 0.2, 0.3]] },
            "errors": []
        });
        let err = parse_embeddings(resp_from(json)).unwrap_err();
        assert!(err.contains("dims"), "got: {err}");
    }

    #[test]
    fn parses_query_matches_into_docs() {
        let json = serde_json::json!({
            "success": true,
            "result": { "count": 1, "matches": [
                { "id": "t1", "score": 0.91, "metadata": {
                    "name": "Slack digest", "description": "Daily summary to Slack",
                    "tags": ["slack", "cron"], "lang": "fr", "source": "n8n"
                }},
                { "id": "t2", "score": 0.80, "metadata": null }
            ]},
            "errors": []
        });
        let docs = parse_matches(serde_json::from_value(json).unwrap()).unwrap();
        assert_eq!(docs.len(), 1, "the null-metadata match is dropped");
        assert_eq!(docs[0].name, "Slack digest");
        assert_eq!(docs[0].lang, "fr");
    }

    #[test]
    fn rejects_unsuccessful_query() {
        let json = serde_json::json!({
            "success": false, "result": null,
            "errors": [{ "code": 4001, "message": "index not found" }]
        });
        assert!(parse_matches(serde_json::from_value(json).unwrap()).is_err());
    }

    #[test]
    fn from_env_none_when_unconfigured() {
        // Neither var set in the unit-test environment → RAG stays off.
        std::env::remove_var("CF_ACCOUNT_ID");
        std::env::remove_var("CF_API_TOKEN");
        assert!(CloudflareRag::from_env(Client::new()).is_none());
    }

    /// Live smoke against Workers AI. Gated on real creds so CI stays green without
    /// them, mirroring the DB tests. Run:
    ///   CF_ACCOUNT_ID=… CF_API_TOKEN=… cargo test -p backend -- --ignored embeds_live
    #[tokio::test]
    #[ignore]
    async fn embeds_live() {
        let rag = CloudflareRag::from_env(Client::new())
            .expect("set CF_ACCOUNT_ID and CF_API_TOKEN to run this test");
        let v = rag.embed("Bonjour, automatisons vos tâches.".into()).await.unwrap();
        assert_eq!(v.len(), VECTOR_DIM);
    }
}
