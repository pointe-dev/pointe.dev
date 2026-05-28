use reqwest::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::embeddings::VECTOR_DIM;

pub const COLLECTION: &str = "n8n_templates";

#[derive(Clone)]
pub struct QdrantStore {
    http: Client,
    base_url: String,
}

/// A template stored in Qdrant. `workflow_json` is kept as a JSON string
/// to avoid nested serialization issues with Qdrant payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplatePayload {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub workflow_json: String,
}

#[derive(Debug, Serialize)]
pub struct TemplatePoint {
    pub payload: TemplatePayload,
    pub vector: Vec<f32>,
}

impl QdrantStore {
    pub fn new(http: Client, base_url: String) -> Self {
        Self { http, base_url }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Creates the collection if it doesn't exist yet. Idempotent.
    pub async fn ensure_collection(&self) -> Result<(), String> {
        let check = self.http
            .get(self.url(&format!("/collections/{COLLECTION}")))
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if check.status().is_success() {
            return Ok(());
        }

        #[derive(Serialize)]
        struct CreateReq { vectors: VecParams }
        #[derive(Serialize)]
        struct VecParams { size: usize, distance: &'static str }

        let resp = self.http
            .put(self.url(&format!("/collections/{COLLECTION}")))
            .json(&CreateReq { vectors: VecParams { size: VECTOR_DIM, distance: "Cosine" } })
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(format!("Qdrant create collection {s}: {b}"));
        }

        tracing::info!("[qdrant] collection '{COLLECTION}' created ({VECTOR_DIM} dims, Cosine)");
        Ok(())
    }

    /// Upserts a batch of template points into the collection.
    pub async fn upsert(&self, points: Vec<TemplatePoint>) -> Result<usize, String> {
        #[derive(Serialize)]
        struct UpsertReq { points: Vec<Point> }
        #[derive(Serialize)]
        struct Point { id: String, vector: Vec<f32>, payload: TemplatePayload }

        let n = points.len();
        let body = UpsertReq {
            points: points.into_iter().map(|p| Point {
                id: Uuid::new_v4().to_string(),
                vector: p.vector,
                payload: p.payload,
            }).collect(),
        };

        let resp = self.http
            .put(self.url(&format!("/collections/{COLLECTION}/points")))
            .query(&[("wait", "true")])
            .json(&body)
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(format!("Qdrant upsert {s}: {b}"));
        }

        tracing::info!("[qdrant] upserted {n} templates");
        Ok(n)
    }

    /// Returns the top-`limit` templates most similar to `query_vector`.
    pub async fn search(
        &self,
        query_vector: Vec<f32>,
        limit: u64,
    ) -> Result<Vec<TemplatePayload>, String> {
        #[derive(Serialize)]
        struct SearchReq { vector: Vec<f32>, limit: u64, with_payload: bool }

        #[derive(Deserialize)]
        struct SearchResp { result: Vec<Hit> }

        #[derive(Deserialize)]
        struct Hit { payload: Option<TemplatePayload>, score: f32 }

        let raw = self.http
            .post(self.url(&format!("/collections/{COLLECTION}/points/search")))
            .json(&SearchReq { vector: query_vector, limit, with_payload: true })
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !raw.status().is_success() {
            let s = raw.status();
            let b = raw.text().await.unwrap_or_default();
            return Err(format!("Qdrant search {s}: {b}"));
        }

        let resp: SearchResp = raw.json()
            .await
            .map_err(|e| format!("Qdrant search parse: {e}"))?;

        let hits: Vec<_> = resp.result.iter()
            .filter_map(|h| h.payload.clone().map(|p| (p, h.score)))
            .collect();

        tracing::debug!("[qdrant] search returned {} hits (top score: {:.3})",
            hits.len(), hits.first().map(|(_, s)| *s).unwrap_or(0.0));

        Ok(hits.into_iter().map(|(p, _)| p).collect())
    }
}
