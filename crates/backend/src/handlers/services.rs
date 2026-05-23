use axum::http::StatusCode;
use axum::Json;
use serde_json::json;

pub async fn get_services() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "services": [
                {
                    "id": "01",
                    "name": "AI Product Commercialization",
                    "description": "Turning raw AI models into production-grade SaaS applications",
                },
                {
                    "id": "02",
                    "name": "Business Process Automation",
                    "description": "Replace manual spreadsheets with autonomous AI agent systems",
                },
                {
                    "id": "03",
                    "name": "High-Performance Systems",
                    "description": "Rust backends with microsecond latencies and absolute reliability",
                },
            ]
        })),
    )
}
