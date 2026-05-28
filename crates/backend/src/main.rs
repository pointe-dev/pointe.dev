use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing_subscriber;

mod handlers;
mod langfuse;
mod state;

use langfuse::LangfuseClient;
use state::AppState;

const FALLBACK_PROMPT: &str = "\
Tu es l'assistant IA de pointe.dev, une agence d'automatisation sur mesure. \
Tu accompagnes les prospects à identifier comment l'automatisation peut transformer leurs opérations. \
Tu es concis, précis, professionnel et chaleureux.

Règles absolues :
- Réponds TOUJOURS dans la langue de l'utilisateur (FR, EN ou DE)
- Pose des questions ciblées pour qualifier le besoin : secteur, volume de tâches, taille d'équipe, douleur principale
- Quand l'utilisateur décrit un processus ou workflow, génère OBLIGATOIREMENT un diagramme Mermaid \
  dans le format exact suivant (sans espace avant les backticks) :
```mermaid
graph TD
  A[Étape 1] --> B[Étape 2]
```
- Utilise TOUJOURS graph TD (top-down), jamais LR
- Les nœuds Mermaid doivent être courts (3-4 mots max), 4-6 nœuds maximum, le graphe lisible
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
struct OpenRouterRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<OpenRouterMessage<'a>>,
}

#[derive(Serialize)]
struct OpenRouterMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct OpenRouterResponse {
    choices: Vec<OpenRouterChoice>,
}

#[derive(Deserialize)]
struct OpenRouterChoice {
    message: OpenRouterMessageOut,
}

#[derive(Deserialize)]
struct OpenRouterMessageOut {
    content: String,
}

async fn handle_ai_chat(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, StatusCode> {
    let start = Utc::now();

    let body = OpenRouterRequest {
        model: "openrouter/free",
        max_tokens: 1024,
        messages: vec![
            OpenRouterMessage { role: "system", content: &state.system_prompt },
            OpenRouterMessage { role: "user",   content: &payload.description },
        ],
    };

    let resp = state
        .http
        .post("https://openrouter.ai/api/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", state.openrouter_key))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("OpenRouter request failed: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        tracing::error!("OpenRouter error {status}: {text}");
        return Err(StatusCode::BAD_GATEWAY);
    }

    let raw = resp.text().await.map_err(|e| {
        tracing::error!("OpenRouter read error: {e}");
        StatusCode::BAD_GATEWAY
    })?;
    tracing::debug!("OpenRouter raw response: {raw}");

    let or_resp: OpenRouterResponse = serde_json::from_str(&raw).map_err(|e| {
        tracing::error!("OpenRouter parse error: {e} — body: {raw}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let text = or_resp.choices.into_iter().next().map(|c| c.message.content).unwrap_or_default();
    let end = Utc::now();

    if state.langfuse.is_some() {
        let input = payload.description.clone();
        let output = text.clone();
        let state2 = state.clone();
        tokio::spawn(async move {
            if let Some(lf) = &state2.langfuse {
                lf.trace(&input, &output, "openrouter/free", start, end).await;
            }
        });
    }

    Ok(Json(ChatResponse { response: text }))
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let http = reqwest::Client::new();
    let openrouter_key = std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY must be set");

    let (system_prompt, langfuse) = init_langfuse(http.clone()).await;

    let state = Arc::new(AppState { openrouter_key, http, system_prompt, langfuse });

    let app = Router::new()
        .route("/api/health", get(handlers::health::health_check))
        .route("/api/services", get(handlers::services::get_services))
        .route("/api/ai/chat", post(handle_ai_chat))
        .with_state(state)
        .nest(
            "/pkg",
            Router::new()
                .nest_service("/", ServeDir::new("./crates/frontend/pkg"))
                .layer(middleware::from_fn(no_store)),
        )
        .nest_service("/", ServeDir::new("./crates/frontend"))
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new());

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("Failed to bind");

    tracing::info!("✨ pointe.dev listening on http://{bind_addr}");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}

async fn no_store(req: axum::extract::Request, next: Next) -> axum::response::Response {
    let mut resp = next.run(req).await;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    resp
}

async fn init_langfuse(http: reqwest::Client) -> (String, Option<LangfuseClient>) {
    let (Some(pub_key), Some(sec_key), Some(base_url)) = (
        std::env::var("LANGFUSE_PUBLIC_KEY").ok(),
        std::env::var("LANGFUSE_SECRET_KEY").ok(),
        std::env::var("LANGFUSE_BASE_URL").ok(),
    ) else {
        tracing::warn!("Langfuse keys not set, using fallback prompt");
        return (FALLBACK_PROMPT.to_string(), None);
    };

    let mut client = LangfuseClient::new(http, base_url, pub_key, sec_key);
    match client.fetch_prompt("qualifier-chatbot-prompt").await {
        Ok(prompt) => {
            tracing::info!(
                "Loaded Langfuse prompt '{}' v{}",
                client.prompt_name,
                client.prompt_version
            );
            (prompt, Some(client))
        }
        Err(e) => {
            tracing::warn!("Failed to fetch Langfuse prompt: {e} — using fallback");
            (FALLBACK_PROMPT.to_string(), Some(client))
        }
    }
}
