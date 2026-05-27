use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing_subscriber;

mod handlers;
mod state;

use state::AppState;

const SYSTEM_PROMPT: &str = "\
Tu es l'assistant IA de pointe.dev, une agence d'automatisation sur mesure. \
Tu accompagnes les prospects à identifier comment l'automatisation peut transformer leurs opérations. \
Tu es concis, précis, professionnel et chaleureux.

Règles absolues :
- Réponds TOUJOURS dans la langue de l'utilisateur (FR, EN ou DE)
- Pose des questions ciblées pour qualifier le besoin : secteur, volume de tâches, taille d'équipe, douleur principale
- Quand l'utilisateur décrit un processus ou workflow, génère OBLIGATOIREMENT un diagramme Mermaid \
  dans le format exact suivant (sans espace avant les backticks) :
```mermaid
graph LR
  A[Étape 1] --> B[Étape 2]
```
- Les nœuds Mermaid doivent être courts (3-4 mots max), le graphe lisible
- Après le diagramme, explique brièvement comment pointe.dev automatise ce flux
- Si le prospect semble qualifié (processus répétitif, volume significatif), propose de planifier un appel
- Ne jamais halluciner des chiffres précis — utilise des fourchettes réalistes
- Réponse max : 200 mots hors diagramme";

#[derive(Deserialize)]
struct ChatRequest {
    description: String,
}

#[derive(Serialize)]
struct ChatResponse {
    response: String,
}

#[derive(Serialize)]
struct ClaudeRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<ClaudeMessage<'a>>,
}

#[derive(Serialize)]
struct ClaudeMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
}

#[derive(Deserialize)]
struct ClaudeContent {
    text: String,
}

async fn handle_ai_chat(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, StatusCode> {
    let body = ClaudeRequest {
        model: "claude-haiku-4-5-20251001",
        max_tokens: 1024,
        system: SYSTEM_PROMPT,
        messages: vec![ClaudeMessage {
            role: "user",
            content: &payload.description,
        }],
    };

    let resp = state
        .http
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &state.anthropic_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Claude API request failed: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        tracing::error!("Claude API error {status}: {text}");
        return Err(StatusCode::BAD_GATEWAY);
    }

    let claude: ClaudeResponse = resp.json().await.map_err(|e| {
        tracing::error!("Claude response parse error: {e}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let text = claude
        .content
        .into_iter()
        .next()
        .map(|c| c.text)
        .unwrap_or_default();

    Ok(Json(ChatResponse { response: text }))
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let state = Arc::new(AppState::new());

    let app = Router::new()
        .route("/api/health", get(handlers::health::health_check))
        .route("/api/services", get(handlers::services::get_services))
        .route("/api/ai/chat", post(handle_ai_chat))
        .with_state(state)
        .nest_service("/pkg", ServeDir::new("./crates/frontend/pkg"))
        .nest_service("/", ServeDir::new("./crates/frontend"))
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3001")
        .await
        .expect("Failed to bind to port 3001");

    tracing::info!("✨ pointe.dev listening on http://0.0.0.0:3001");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}
