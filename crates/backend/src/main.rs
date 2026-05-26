use axum::{
    routing::get,
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
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

    // Serve static files (frontend WASM, assets, etc.)
    let _serve_dir = ServeDir::new("./frontend")
        .not_found_service(
            // Fallback to index.html for SPA routing
            axum::response::IntoResponse::into_response(
                axum::http::StatusCode::NOT_FOUND
            ),
        );

    // Build router
    let app = Router::new()
        // API routes
        .route("/api/health", get(handlers::health::health_check))
        .route("/api/services", get(handlers::services::get_services))
        .with_state(state)
        // Serve static frontend assets
        .nest_service("/pkg", ServeDir::new("./frontend/pkg"))
        .nest_service("/", ServeDir::new("./frontend"))
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
