use axum::http::StatusCode;
use serde_json::json;

pub async fn health_check() -> (StatusCode, String) {
    (
        StatusCode::OK,
        json!({"status": "healthy", "service": "pointe.dev"}).to_string(),
    )
}
