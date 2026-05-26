use axum::{
    routing::{get, post},
    Router,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing_subscriber;

mod handlers;
mod state;

use state::AppState;

#[derive(Deserialize, Serialize)]
pub struct UseCase {
    pub description: String,
}

#[derive(Serialize)]
pub struct AiResponse {
    pub response: String,
}

async fn handle_ai_chat(Json(payload): Json<UseCase>) -> Json<AiResponse> {
    let response = format!(
        "Thanks for sharing: \"{}\"\n\nBased on your use case, we can help you:\n• Automate repetitive workflows\n• Reduce manual hours by up to 80%\n• Set up monitoring and alerts\n\nReady to move forward?",
        payload.description
    );
    
    Json(AiResponse { response })
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Log current working directory
    let cwd = std::env::current_dir().expect("Failed to get CWD");
    tracing::info!("📁 Current working directory: {:?}", cwd);

    // Check if frontend directory exists
    let frontend_dir = std::path::Path::new("./frontend");
    if frontend_dir.exists() {
        tracing::info!("✅ /frontend directory exists");
        if let Ok(entries) = std::fs::read_dir("./frontend") {
            for entry in entries {
                if let Ok(entry) = entry {
                    tracing::info!("   📄 {:?}", entry.path());
                }
            }
        }
    } else {
        tracing::warn!("❌ /frontend directory NOT found!");
    }

    // Application state
    let state = Arc::new(AppState::new());

    // Build router
    let app = Router::new()
        // API routes
        .route("/api/health", get(handlers::health::health_check))
        .route("/api/services", get(handlers::services::get_services))
        .route("/api/ai/chat", post(handle_ai_chat))
        .with_state(state)
        // Serve static frontend assets
        .nest_service("/pkg", ServeDir::new("./crates/frontend/pkg"))
        .nest_service("/", ServeDir::new("./crates/frontend"))
        .layer(CorsLayer::permissive());

    // Bind to all interfaces (0.0.0.0) so Railway/Docker can access it
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001")
        .await
        .expect("Failed to bind to port 3001");

    tracing::info!("✨ pointe.dev backend + frontend listening on http://0.0.0.0:3001");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
