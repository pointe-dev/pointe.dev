use axum::{
    routing::get,
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing_subscriber;

mod handlers;
mod state;

use state::AppState;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Application state
    let state = Arc::new(AppState::new());

    // Build router
    let app = Router::new()
        .route("/", get(handlers::health::health_check))
        .route("/api/services", get(handlers::services::get_services))
        .with_state(state)
        .layer(CorsLayer::permissive());

    // Bind to localhost:3001
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001")
        .await
        .expect("Failed to bind to port 3001");

    tracing::info!("✨ pointe.dev backend listening on http://127.0.0.1:3001");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
