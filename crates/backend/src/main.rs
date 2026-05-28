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

mod agents;
mod embeddings;
mod handlers;
mod langfuse;
mod pipeline;
mod qdrant;
mod sessions;
mod state;
mod stripe;

use embeddings::EmbeddingEngine;
use langfuse::LangfuseClient;
use pipeline::PipelineStore;
use qdrant::QdrantStore;
use sessions::{SessionStore, FREE_MESSAGES};
use state::AppState;
use stripe::StripeClient;

const FALLBACK_PROMPT: &str = "\
Tu es l'assistant IA de pointe.dev, une agence d'automatisation sur mesure. \
Tu accompagnes les prospects à identifier comment l'automatisation peut transformer leurs opérations. \
Tu es concis, précis, professionnel et chaleureux.

Règles absolues :
- Réponds TOUJOURS dans la langue de l'utilisateur (FR, EN ou DE)
- Pose des questions ciblées pour qualifier le besoin : secteur, volume de tâches, taille d'équipe, douleur principale
- Quand l'utilisateur décrit un processus ou workflow, génère OBLIGATOIREMENT un bloc workflow \
  dans le format exact suivant (sans espace avant les backticks) :
```workflow
{\"nodes\":[{\"id\":\"1\",\"label\":\"Étape 1\",\"kind\":\"start\"},{\"id\":\"2\",\"label\":\"Étape 2\",\"kind\":\"process\"}],\"edges\":[{\"from\":\"1\",\"to\":\"2\"}]}
```
- Les nœuds doivent être courts (3-4 mots max), 4-6 nœuds maximum
- Après le workflow, explique brièvement comment pointe.dev automatise ce flux
- Ne jamais halluciner des chiffres précis — utilise des fourchettes réalistes
- Réponse max : 200 mots hors workflow

Déclenchement du pipeline :
Dès que tu as collecté les 4 éléments suivants, INCLUS un bloc qualify INVISIBLE à la fin de ta réponse :
  1. secteur d'activité
  2. douleur principale (tâche répétitive ou source d'erreurs)
  3. outils actuels utilisés (CRM, ERP, e-mail, etc.)
  4. volume approximatif (ex: 50 commandes/jour, 200 leads/mois)

Format du bloc qualify (toujours en dernier, jamais affiché à l'utilisateur) :
```qualify
{\"client_need\": \"une phrase décrivant précisément le besoin d'automatisation\", \"summary\": \"secteur | douleur | outils | volume\"}
```";

#[derive(Deserialize)]
struct HistoryMsg {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatRequest {
    description: String,
    #[serde(default)]
    history: Vec<HistoryMsg>,
    session_id: String,
}

#[derive(Serialize)]
struct ChatResponse {
    response: String,
    messages_used: u32,
    messages_free: u32,
    /// Set when the AI qualifies the prospect and a pipeline has been launched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pipeline_id: Option<String>,
}

#[derive(serde::Deserialize)]
struct QualifyBlock {
    client_need: String,
    summary: String,
}

/// Strips a ```qualify block from the AI response.
/// Returns (display_text, Option<QualifyBlock>).
fn parse_qualify(text: &str) -> (String, Option<QualifyBlock>) {
    const OPEN: &str = "```qualify";
    const CLOSE: &str = "```";
    if let Some(start) = text.find(OPEN) {
        let after_tag = &text[start + OPEN.len()..];
        let after = match after_tag.find('\n') {
            Some(nl) => &after_tag[nl + 1..],
            None => return (text.to_string(), None),
        };
        if let Some(end) = after.find(CLOSE) {
            let json = after[..end].trim();
            let before = text[..start].trim();
            let rest = after[end + CLOSE.len()..].trim();
            let display = match (before.is_empty(), rest.is_empty()) {
                (true,  true)  => String::new(),
                (false, true)  => before.to_string(),
                (true,  false) => rest.to_string(),
                (false, false) => format!("{before}\n\n{rest}"),
            };
            let block = serde_json::from_str::<QualifyBlock>(json).ok();
            return (display, block);
        }
    }
    (text.to_string(), None)
}

#[derive(Deserialize)]
struct UnlockRequest {
    session_id: String,
    email: String,
}

#[derive(Serialize)]
struct UnlockResponse {
    ok: bool,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: &'static str,
    max_tokens: u32,
    system: String,
    messages: Vec<AnthropicMessage>,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

async fn handle_unlock(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UnlockRequest>,
) -> Json<UnlockResponse> {
    let email = payload.email.trim().to_lowercase();
    let valid = email.contains('@') && email.contains('.');
    if valid {
        state.sessions.unlock_with_email(&payload.session_id, email).await;
    }
    Json(UnlockResponse { ok: valid })
}

async fn handle_ai_chat(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, StatusCode> {
    let session_key = payload.session_id.clone();
    if !state.sessions.check_and_increment(&session_key).await {
        return Err(StatusCode::PAYMENT_REQUIRED);
    }
    let messages_used = state.sessions.message_count(&session_key).await;

    let start = Utc::now();

    let messages: Vec<AnthropicMessage> = payload.history.into_iter()
        .map(|h| AnthropicMessage { role: h.role, content: h.content })
        .chain(std::iter::once(AnthropicMessage {
            role: "user".to_string(),
            content: payload.description.clone(),
        }))
        .collect();

    let body = AnthropicRequest {
        model: "claude-haiku-4-5-20251001",
        max_tokens: 1024,
        system: state.system_prompt.clone(),
        messages,
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
            tracing::error!("Anthropic request failed: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        tracing::error!("Anthropic error {status}: {text}");
        return Err(StatusCode::BAD_GATEWAY);
    }

    let raw = resp.text().await.map_err(|e| {
        tracing::error!("Anthropic read error: {e}");
        StatusCode::BAD_GATEWAY
    })?;

    let ant_resp: AnthropicResponse = serde_json::from_str(&raw).map_err(|e| {
        tracing::error!("Anthropic parse error: {e} — body: {raw}");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let raw_text = ant_resp.content.into_iter()
        .find(|c| c.kind == "text")
        .and_then(|c| c.text)
        .unwrap_or_default();
    let end = Utc::now();

    // Strip qualify block and launch pipeline if the AI decided to qualify
    let (display_text, pipeline_id) = {
        let (display, maybe_qualify) = parse_qualify(&raw_text);
        let pid = if let Some(q) = maybe_qualify {
            let id = state.pipelines.create(
                payload.session_id.clone(),
                q.client_need,
                Some(q.summary),
            ).await;
            pipeline::spawn(id, state.pipelines.clone(), state.clone());
            tracing::info!("Pipeline {id} launched from chat session={}", payload.session_id);
            Some(id.to_string())
        } else {
            None
        };
        (display, pid)
    };

    if state.langfuse.is_some() {
        let input = payload.description.clone();
        let output = display_text.clone();
        let state2 = state.clone();
        tokio::spawn(async move {
            if let Some(lf) = &state2.langfuse {
                lf.trace(&input, &output, "claude-haiku-4-5-20251001", start, end).await;
            }
        });
    }

    Ok(Json(ChatResponse {
        response: display_text,
        messages_used,
        messages_free: FREE_MESSAGES,
        pipeline_id,
    }))
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let http = reqwest::Client::new();
    let anthropic_key = std::env::var("ANTHROPIC_API_KEY").expect("ANTHROPIC_API_KEY must be set");

    let (system_prompt, langfuse) = init_langfuse(http.clone()).await;

    let qdrant = std::env::var("QDRANT_URL").ok().map(|url| {
        tracing::info!("Qdrant configured at {url}");
        QdrantStore::new(http.clone(), url)
    });
    if qdrant.is_none() {
        tracing::warn!("QDRANT_URL not set — RAG disabled, builder uses stub");
    }

    // BGE-M3 downloads ~300 MB on first run, then stays cached.
    let embeddings = tokio::task::spawn_blocking(EmbeddingEngine::new)
        .await
        .unwrap_or_else(|e| Err(format!("join error: {e}")))
        .map_err(|e| tracing::warn!("Embedding engine unavailable: {e} — RAG disabled"))
        .ok();
    if embeddings.is_some() {
        tracing::info!("Embedding engine ready (BGE-M3, 1024 dims, local)");
    }

    let stripe = match (
        std::env::var("STRIPE_SECRET_KEY").ok(),
        std::env::var("STRIPE_WEBHOOK_SECRET").ok(),
    ) {
        (Some(sk), Some(wh)) => {
            tracing::info!("Stripe configured");
            Some(StripeClient::new(http.clone(), sk, wh))
        }
        _ => {
            tracing::warn!("STRIPE_SECRET_KEY or STRIPE_WEBHOOK_SECRET not set — payments disabled");
            None
        }
    };

    let state = Arc::new(AppState {
        anthropic_key,
        http,
        system_prompt,
        langfuse,
        sessions: SessionStore::new(),
        pipelines: PipelineStore::new(),
        qdrant,
        embeddings,
        stripe,
    });

    let app = Router::new()
        .route("/api/health", get(handlers::health::health_check))
        .route("/api/services", get(handlers::services::get_services))
        .route("/api/ai/chat", post(handle_ai_chat))
        .route("/api/auth/unlock", post(handle_unlock))
        .route("/api/pipeline/start", post(handlers::pipeline::start))
        .route("/api/pipeline/:id", get(handlers::pipeline::status))
        .route("/api/pipeline/:id/resume", post(handlers::pipeline::resume))
        .route("/api/admin/ingest", post(handlers::ingest::ingest))
        .route("/api/stripe/checkout", post(handlers::stripe::create_checkout))
        .route("/api/stripe/webhook", post(handlers::stripe::webhook))
        .route("/mcp", post(handlers::mcp::handle))
        .with_state(state)
        .nest(
            "/pkg",
            Router::new()
                .nest_service("/", ServeDir::new("./crates/frontend/pkg"))
                .layer(middleware::from_fn(no_store)),
        )
        .nest(
            "/",
            Router::new()
                .nest_service("/", ServeDir::new("./crates/frontend"))
                .layer(middleware::from_fn(no_store)),
        )
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
