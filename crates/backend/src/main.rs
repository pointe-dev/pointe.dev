use axum::{
    extract::{ConnectInfo, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::Redirect,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tower_http::services::{ServeDir, ServeFile};
use tracing_subscriber::EnvFilter;

mod agents;
mod capabilities;
mod cloudflare;
mod config;
mod credentials;
mod email;
mod embeddings;
mod guardrails;
mod handlers;
mod langfuse;
mod mcp;
mod oauth;
mod pending;
mod pipeline;
mod pitch;
mod qdrant;
mod sessions;
mod state;
mod stripe;

use embeddings::EmbeddingEngine;
use langfuse::LangfuseClient;
use pipeline::PipelineStore;
use pitch::{PitchResult, PitchStore};
use qdrant::QdrantStore;
use sessions::SessionStore;
use state::AppState;
use stripe::StripeClient;
use sqlx::postgres::PgPoolOptions;

/// Used only when Langfuse is unreachable. Single source of truth shared
/// with scripts/push-prompts.sh so the local fallback and the Langfuse
/// `qualifier-chatbot-prompt` never drift.
const FALLBACK_PROMPT: &str = include_str!("../../../prompts/qualifier-chatbot-prompt.txt");

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
    /// SHA-256 hex of browser signals (UA+lang+tz+screen). Used as secondary
    /// rate-limit bucket alongside IP to prevent localStorage-clearing abuse.
    #[serde(default)]
    fingerprint: Option<String>,
}

#[derive(Serialize)]
struct ChatResponse {
    response: String,
    /// Remaining spendable balance (gift + purchased), in cents.
    balance_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pipeline_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    options: Vec<ChatOption>,
    /// True when the visitor qualified but isn't unlocked yet: the pipeline is
    /// stashed (not spawned) until they confirm their email. The frontend opens
    /// the email modal in response.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    needs_unlock: bool,
    /// True when the credit balance is exhausted — the frontend shows the
    /// top-up / subscription options.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    needs_credits: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChatOption {
    pub label: String,
}

/// Strips a ```options block from the AI response.
/// Returns (display_text, Vec<ChatOption>).
fn parse_options(text: &str) -> (String, Vec<ChatOption>) {
    const OPEN: &str = "```options";
    const CLOSE: &str = "```";
    if let Some(start) = text.find(OPEN) {
        let after_tag = &text[start + OPEN.len()..];
        let after = match after_tag.find('\n') {
            Some(nl) => &after_tag[nl + 1..],
            None => return (text.to_string(), vec![]),
        };
        if let Some(end) = after.find(CLOSE) {
            let json = after[..end].trim();
            let before = text[..start].trim_end();
            let rest = after[end + CLOSE.len()..].trim_start();
            let display = match (before.is_empty(), rest.is_empty()) {
                (true,  true)  => String::new(),
                (false, true)  => before.to_string(),
                (true,  false) => rest.to_string(),
                (false, false) => format!("{before}\n\n{rest}"),
            };
            let opts = serde_json::from_str::<Vec<ChatOption>>(json).unwrap_or_default();
            return (display, opts);
        }
    }
    (text.to_string(), vec![])
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
    /// Browser fingerprint (same as chat) — used to stop re-farming signup credits.
    #[serde(default)]
    fingerprint: Option<String>,
}

#[derive(Serialize)]
struct UnlockResponse {
    ok: bool,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: &'static str,
    max_tokens: u32,
    /// Structured system block: a single text part carrying cache_control so the
    /// (large) system prompt is cached and re-read across turns of a conversation.
    system: serde_json::Value,
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
    #[serde(default)]
    usage: Option<AnthropicUsage>,
}

#[derive(Deserialize, Default)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    cache_creation_input_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
}

#[derive(Deserialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

async fn handle_unlock(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UnlockRequest>,
) -> Json<UnlockResponse> {
    let email = payload.email.trim().to_lowercase();
    let valid = email.contains('@') && email.contains('.');
    if !valid {
        return Json(UnlockResponse { ok: false });
    }

    // Capture the email NOW (chat enabled immediately) and grant the signup
    // credits — verification by link is deferred (required later, before payment).
    let signed = SessionStore::sign_token(&email, &state.session_secret);
    state.sessions.unlock_with_email(&payload.session_id, email.clone(), &signed).await;
    let ip = real_ip(addr, &headers);
    let fp_key = payload
        .fingerprint
        .as_deref()
        .map(|fp| SessionStore::fp_bucket(&ip, fp));
    let granted = state
        .sessions
        .grant_signup_credits(&payload.session_id, fp_key.as_deref())
        .await;
    tracing::info!("Lead captured ({email}) — signup credits granted={granted}");

    // Still send the double-opt-in link so the email gets verified before payment.
    let confirm_token = SessionStore::sign_confirm_token(&email, &payload.session_id, &state.session_secret);
    let encoded_email = urlencoding::encode(&email);
    let confirm_url = format!(
        "{}/api/auth/confirm?e={}&s={}&t={}",
        state.base_url, encoded_email, payload.session_id, confirm_token
    );

    match &state.resend_api_key {
        Some(api_key) => {
            if let Err(e) = send_confirm_email(&state.http, api_key, &email, &confirm_url).await {
                tracing::error!("Failed to send confirmation email to {email}: {e}");
                // Capture + credits already done; the lead can chat. Report ok.
            }
        }
        None => {
            // Dev mode: log the link so you can test without a mail provider
            tracing::warn!("RESEND_API_KEY not set — confirm link for {email}: {confirm_url}");
        }
    }

    Json(UnlockResponse { ok: true })
}

#[derive(Deserialize)]
struct ConfirmParams {
    e: String,
    s: String,
    t: String,
}

async fn handle_confirm(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ConfirmParams>,
) -> Redirect {
    let email = params.e.trim().to_lowercase();
    let session_id = params.s.trim().to_string();
    let token = params.t.trim().to_string();

    if email.is_empty() || session_id.is_empty() || token.is_empty() {
        return Redirect::to("/");
    }
    if !email.contains('@') {
        return Redirect::to("/");
    }

    if !SessionStore::verify_confirm_token(&email, &session_id, &token, &state.session_secret) {
        tracing::warn!("Invalid confirmation token for email: {email}");
        return Redirect::to("/");
    }

    let signed = SessionStore::sign_token(&email, &state.session_secret);
    // Capture may already have happened at /unlock; this is idempotent.
    let first_unlock = state.sessions.unlock_with_email(&session_id, email.clone(), &signed).await;
    // Mark BOTH the session id and the token-keyed session as verified — the link
    // click is the proof of email ownership required before payment.
    state.sessions.mark_email_verified(&session_id).await;
    state.sessions.mark_email_verified(&signed).await;
    tracing::info!("Email verified for: {email}");

    // First confirmation only: spawn the pipeline that was gated behind the
    // email. `unlock_with_email` returns false on replays, so we never spawn
    // twice from repeated link clicks.
    if first_unlock {
        if let Some(q) = state.pending.take_qualify(&session_id).await {
            let id = state.pipelines.create(
                session_id.clone(),
                q.client_need,
                Some(q.summary),
            ).await;
            pipeline::spawn(id, state.pipelines.clone(), state.clone());
            state.pending.set_spawned(session_id.clone(), id.to_string()).await;
            tracing::info!("Pipeline {id} launched after email unlock for session={session_id}");
        }
    }

    let redirect_url = format!("/?_sid={}", signed);
    Redirect::to(&redirect_url)
}

/// Re-export so handlers in this file can call `resend_send(...)` directly.
use email::resend_send;

async fn send_confirm_email(
    http: &reqwest::Client,
    api_key: &str,
    to_email: &str,
    confirm_url: &str,
) -> Result<(), String> {
    let html = format!(
        r#"<!DOCTYPE html>
<html><body style="font-family:sans-serif;background:#0a0a0a;margin:0;padding:40px 20px">
<div style="max-width:480px;margin:auto;background:#111;border-radius:16px;padding:40px;border:1px solid #222">
  <p style="color:#dc2626;font-size:20px;font-weight:700;margin:0 0 20px">pointe.dev</p>
  <h1 style="color:#f3f4f6;font-size:20px;font-weight:600;margin:0 0 12px">Confirmez votre accès</h1>
  <p style="color:#9ca3af;font-size:14px;line-height:1.7;margin:0 0 28px">
    Cliquez sur le bouton ci-dessous pour continuer votre conversation et accéder à notre analyse d'automatisation personnalisée.
  </p>
  <a href="{confirm_url}" style="display:inline-block;background:#dc2626;color:white;padding:14px 28px;border-radius:10px;text-decoration:none;font-weight:600;font-size:15px">
    Continuer la conversation →
  </a>
  <p style="color:#4b5563;font-size:12px;margin-top:32px;line-height:1.6">
    Si vous n'avez pas demandé cet accès, ignorez simplement cet email.
  </p>
</div>
</body></html>"#
    );
    resend_send(http, api_key, to_email, "Continuez votre conversation avec pointe.dev", &html).await
        .inspect(|_| tracing::info!("Confirmation email sent to {to_email}"))
}

// ── Pitch: n8n pipeline callback ─────────────────────────────────────────────

#[derive(Deserialize)]
struct PipelineResultPayload {
    session_id: String,
    #[serde(flatten)]
    result: PitchResult,
}

async fn handle_pipeline_result(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<PipelineResultPayload>,
) -> Json<serde_json::Value> {
    if payload.session_id.is_empty() {
        return Json(serde_json::json!({ "ok": false, "error": "missing session_id" }));
    }
    state.pitches.set(&payload.session_id, payload.result).await;
    Json(serde_json::json!({ "ok": true }))
}

// ── Pitch: frontend polling ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct PitchPollParams {
    /// Pipeline id — pitches are keyed per pipeline so each qualification keeps
    /// its own result (and a re-qualification never returns a previous one).
    pid: String,
}

async fn handle_pitch_poll(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PitchPollParams>,
) -> Json<serde_json::Value> {
    match state.pitches.get(&params.pid).await {
        Some(r) => Json(serde_json::json!({
            "ready":            true,
            "manual_quote":     r.manual_quote,
            "solution_desc":    r.solution_desc,
            "price_eur_cents":  r.price_eur_cents,
            "price_validity":   r.price_validity,
            "externals_needed": r.externals_needed,
            "slides":           r.slides,
        })),
        None => Json(serde_json::json!({ "ready": false })),
    }
}

// ── Auth: email confirmation status (frontend poll) ───────────────────────────

#[derive(Deserialize)]
struct AuthStatusParams { sid: String }

async fn handle_auth_status(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuthStatusParams>,
) -> Json<serde_json::Value> {
    let unlocked = state.sessions.is_unlocked(&params.sid).await;
    let email    = state.sessions.get_email(&params.sid).await;
    // Present only when a gated pipeline was spawned on confirm — lets the
    // polling tab start watching the pitch without a page reload.
    let pipeline_id = state.pending.spawned_id(&params.sid).await;
    Json(serde_json::json!({ "unlocked": unlocked, "email": email, "pipeline_id": pipeline_id }))
}

/// Extract the best-guess real IP from the request.
fn real_ip(addr: SocketAddr, headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string())
}

async fn handle_ai_chat(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, StatusCode> {
    let session_key = payload.session_id.clone();
    let _ip = real_ip(addr, &headers);

    // Gate 1 — email capture is required before the first message. An uncaptured
    // session gets no chat; the frontend opens the email prompt on `needs_unlock`.
    if !state.sessions.is_unlocked(&session_key).await {
        return Ok(Json(ChatResponse {
            response: String::new(),
            balance_cents: 0,
            pipeline_id: None,
            options: Vec::new(),
            needs_unlock: true,
            needs_credits: false,
        }));
    }

    // Gate 2 — charge the per-message cost. Out of credits → ask to top up.
    if !state.sessions.charge(&session_key, sessions::MESSAGE_COST_CENTS).await {
        return Ok(Json(ChatResponse {
            response: String::new(),
            balance_cents: state.sessions.balance_cents(&session_key).await,
            pipeline_id: None,
            options: Vec::new(),
            needs_unlock: false,
            needs_credits: true,
        }));
    }

    let start = Utc::now();

    let messages: Vec<AnthropicMessage> = payload.history.into_iter()
        .map(|h| AnthropicMessage { role: h.role, content: h.content })
        .chain(std::iter::once(AnthropicMessage {
            role: "user".to_string(),
            content: payload.description.clone(),
        }))
        .collect();

    let body = AnthropicRequest {
        // Sonnet for the conversational qualifier: markedly more natural/persuasive
        // than Haiku, and its ~1024-token cache minimum (vs Haiku's ~4096, measured)
        // means our ~2240-token system prompt caches at its current size.
        model: "claude-sonnet-4-6",
        max_tokens: 1024,
        // Cache breakpoint on the system prompt (identical across conversation turns,
        // which happen seconds apart → high hit rate within a conversation).
        system: serde_json::json!([{
            "type": "text",
            "text": state.system_prompt,
            "cache_control": { "type": "ephemeral", "ttl": "1h" }
        }]),
        messages,
    };

    let resp = state
        .http
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &state.anthropic_key)
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "prompt-caching-2024-07-31,extended-cache-ttl-2025-04-11")
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

    if let Some(u) = &ant_resp.usage {
        tracing::info!(
            "[chat] tokens in={} cache_write={} cache_read={} (hit_ratio={:.0}%)",
            u.input_tokens, u.cache_creation_input_tokens, u.cache_read_input_tokens,
            {
                let total = u.input_tokens + u.cache_creation_input_tokens + u.cache_read_input_tokens;
                if total > 0 { u.cache_read_input_tokens as f64 / total as f64 * 100.0 } else { 0.0 }
            }
        );
    }

    let raw_text = ant_resp.content.into_iter()
        .find(|c| c.kind == "text")
        .and_then(|c| c.text)
        .unwrap_or_default();
    let end = Utc::now();

    // Strip qualify block. If the AI qualified, the pipeline launches only for
    // an unlocked (email-confirmed) session; otherwise we stash the
    // qualification and ask the visitor to confirm their email first.
    let (display_text, pipeline_id, options, needs_unlock) = {
        let (after_qualify, maybe_qualify) = parse_qualify(&raw_text);
        let (pid, gate) = if let Some(q) = maybe_qualify {
            if state.sessions.is_unlocked(&session_key).await {
                let id = state.pipelines.create(
                    payload.session_id.clone(),
                    q.client_need,
                    Some(q.summary),
                ).await;
                pipeline::spawn(id, state.pipelines.clone(), state.clone());
                tracing::info!("Pipeline {id} launched from chat session={}", payload.session_id);
                (Some(id.to_string()), false)
            } else {
                state.pending.stash(
                    session_key.clone(),
                    pending::PendingQualification { client_need: q.client_need, summary: q.summary },
                ).await;
                tracing::info!("Qualification stashed — awaiting email unlock for session={session_key}");
                (None, true)
            }
        } else {
            (None, false)
        };
        let (display, opts) = parse_options(&after_qualify);
        (display, pid, opts, gate)
    };

    if state.langfuse.is_some() {
        let input = payload.description.clone();
        let output = display_text.clone();
        let state2 = state.clone();
        tokio::spawn(async move {
            if let Some(lf) = &state2.langfuse {
                lf.trace(&input, &output, "claude-sonnet-4-6", start, end).await;
            }
        });
    }

    Ok(Json(ChatResponse {
        response: display_text,
        balance_cents: state.sessions.balance_cents(&session_key).await,
        pipeline_id,
        options,
        needs_unlock,
        needs_credits: false,
    }))
}

/// Initialises tracing to stdout, plus a daily-rolling file when `LOG_DIR` is
/// set (prod, via a mounted Docker volume that survives container recreate).
/// Returns the non-blocking writer guard, which the caller must keep alive.
fn init_tracing() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::prelude::*;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let stdout_layer = tracing_subscriber::fmt::layer().compact();

    match std::env::var("LOG_DIR") {
        Ok(dir) if !dir.is_empty() => {
            let appender = tracing_appender::rolling::daily(&dir, "backend.log");
            let (writer, guard) = tracing_appender::non_blocking(appender);
            let file_layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(writer);
            tracing_subscriber::registry()
                .with(filter)
                .with(stdout_layer)
                .with(file_layer)
                .init();
            tracing::info!("File logging enabled at {dir}/backend.log");
            Some(guard)
        }
        _ => {
            tracing_subscriber::registry()
                .with(filter)
                .with(stdout_layer)
                .init();
            None
        }
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    // Keep the WorkerGuard alive for the whole program: dropping it flushes and
    // stops the background log-writing thread. Bound to a named local in main so
    // it lives until the process exits.
    let _log_guard = init_tracing();

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

    // BGE-M3 only needed when Qdrant RAG is active.
    let embeddings = if qdrant.is_some() {
        tokio::task::spawn_blocking(EmbeddingEngine::new)
            .await
            .unwrap_or_else(|e| Err(format!("join error: {e}")))
            .map_err(|e| tracing::warn!("Embedding engine unavailable: {e} — RAG disabled"))
            .ok()
    } else {
        None
    };
    if embeddings.is_some() {
        tracing::info!("Embedding engine ready (BGE-M3, 1024 dims, local)");
    }

    // Managed RAG on Cloudflare (Workers AI + Vectorize). Takes precedence over the
    // local qdrant+fastembed path when CF_ACCOUNT_ID + CF_API_TOKEN are set.
    let cloudflare = cloudflare::CloudflareRag::from_env(http.clone());
    match &cloudflare {
        Some(_) => tracing::info!("Cloudflare RAG configured (Workers AI bge-m3 + Vectorize)"),
        None => tracing::info!("CF_ACCOUNT_ID/CF_API_TOKEN not set — Cloudflare RAG off"),
    }

    // n8n MCP connector: grounds the build pipeline on the real node catalogue.
    let n8n_mcp = mcp::N8nMcpConfig::from_env();
    match &n8n_mcp {
        Some(_) => tracing::info!("n8n MCP connector configured (builder/critic/designer grounded on live node catalogue)"),
        None => tracing::info!("N8N_MCP_URL/N8N_MCP_TOKEN not set — n8n MCP grounding off"),
    }

    let stripe = match (
        // Treat an empty value (`STRIPE_SECRET_KEY=` in .env) as unset — otherwise we'd
        // build a StripeClient with an empty key that passes the 503 "not configured"
        // guard and only fails later at the API call with an opaque 502.
        std::env::var("STRIPE_SECRET_KEY").ok().filter(|s| !s.is_empty()),
        std::env::var("STRIPE_WEBHOOK_SECRET").ok().filter(|s| !s.is_empty()),
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

    let session_secret = config::load_session_secret();
    let admin_ingest_token = config::load_admin_ingest_token();
    if admin_ingest_token.is_none() {
        tracing::warn!("ADMIN_INGEST_TOKEN not set — /api/admin/ingest will reject requests");
    }

    let resend_api_key = std::env::var("RESEND_API_KEY").ok();
    if resend_api_key.is_none() {
        tracing::warn!("RESEND_API_KEY not set — confirmation links will be logged instead of emailed");
    }

    let base_url = std::env::var("BASE_URL")
        .unwrap_or_else(|_| "http://localhost:3001".to_string());
    tracing::info!("Base URL: {base_url}");

    let owner_email = std::env::var("OWNER_EMAIL").ok();
    if let Some(ref e) = owner_email {
        tracing::info!("Owner notifications → {e}");
    }

    let db = match std::env::var("DATABASE_URL") {
        Ok(url) => {
            match PgPoolOptions::new().max_connections(5).connect(&url).await {
                Ok(pool) => {
                    if let Err(e) = pitch::run_migrations(&pool).await {
                        tracing::warn!("DB migration failed: {e} — falling back to in-memory");
                        None
                    } else if let Err(e) = sessions::run_migrations(&pool).await {
                        tracing::warn!("Session DB migration failed: {e} — falling back to in-memory");
                        None
                    } else if let Err(e) = pipeline::run_migrations(&pool).await {
                        tracing::warn!("Pipeline DB migration failed: {e} — falling back to in-memory");
                        None
                    } else {
                        tracing::info!("Postgres connected — pitch + session + pipeline persistence enabled");
                        Some(pool)
                    }
                }
                Err(e) => {
                    tracing::warn!("DATABASE_URL set but connection failed: {e} — in-memory only");
                    None
                }
            }
        }
        Err(_) => {
            tracing::warn!("DATABASE_URL not set — pitches stored in-memory only (lost on restart)");
            None
        }
    };

    let sessions = SessionStore::with_db(db.clone()).await;
    let pipelines = PipelineStore::with_db(db.clone()).await;

    let state = Arc::new(AppState {
        anthropic_key,
        http,
        system_prompt,
        langfuse,
        sessions,
        pipelines,
        pending: pending::PendingStore::new(),
        pitches: PitchStore::new(db.clone()),
        qdrant,
        embeddings,
        cloudflare,
        n8n_mcp,
        stripe,
        session_secret,
        admin_ingest_token,
        resend_api_key,
        base_url,
        owner_email,
        db,
    });

    // Re-drive pipelines that were mid-flight when the process last stopped.
    // hydrate() restored their state from Postgres; spawn() resumes execution from
    // their current stage. Paused (AwaitingPayment) and terminal stages are skipped.
    {
        let resumable = state.pipelines.resumable_ids().await;
        if !resumable.is_empty() {
            tracing::info!("[pipeline] re-spawning {} in-flight pipeline(s) after restart", resumable.len());
            for id in resumable {
                pipeline::spawn(id, state.pipelines.clone(), state.clone());
            }
        }
    }

    let app = Router::new()
        .route("/api/health", get(handlers::health::health_check))
        .route("/api/services", get(handlers::services::get_services))
        .route("/api/ai/chat", post(handle_ai_chat))
        .route("/api/auth/unlock", post(handle_unlock))
        .route("/api/auth/confirm", get(handle_confirm))
        .route("/api/pitch/pipeline-result", post(handle_pipeline_result))
        .route("/api/pitch/result", get(handle_pitch_poll))
        .route("/api/auth/status", get(handle_auth_status))
        .route("/api/pipeline/start", post(handlers::pipeline::start))
        .route("/api/pipeline/:id", get(handlers::pipeline::status))
        .route("/api/pipeline/:id/resume", post(handlers::pipeline::resume))
        .route("/api/pipeline/:id/delivery", get(handlers::pipeline::delivery))
        .route("/api/credentials/provision", post(handlers::credentials::provision))
        .route("/api/oauth/start", post(handlers::credentials::oauth_start))
        .route("/api/client/workflows", get(handlers::client::workflows))
        .route("/api/admin/ingest", post(handlers::ingest::ingest))
        .route("/api/admin/dossiers", get(handlers::admin::dossiers))
        .route("/api/admin/dossiers/:id/respawn", post(handlers::admin::respawn))
        .route("/api/admin/secret-share", post(handlers::secret_share::secret_share))
        .route("/api/stripe/checkout", post(handlers::stripe::create_checkout))
        .route("/api/credits/topup", post(handlers::stripe::create_topup))
        .route("/api/stripe/webhook", post(handlers::stripe::webhook))
        .route("/mcp", post(handlers::mcp::handle))
        .route("/merci", get(serve_index))
        .route("/admin", get(serve_index))
        .route("/espace", get(serve_index))
        .with_state(state)
        .nest(
            "/pkg",
            Router::new()
                .nest_service("/", ServeDir::new("./crates/frontend/pkg"))
                .layer(middleware::from_fn(no_store)),
        )
        .fallback_service(
            Router::new()
                .nest_service("/", ServeDir::new("./crates/frontend"))
                .layer(middleware::from_fn(no_store)),
        )
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new());

    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .expect("Failed to bind");

    tracing::info!("✨ pointe.dev listening on http://{bind_addr}");

    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("Server error");
}

async fn serve_index() -> impl axum::response::IntoResponse {
    match tokio::fs::read("./crates/frontend/index.html").await {
        Ok(bytes) => axum::response::Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-store")
            .body(axum::body::Body::from(bytes))
            .unwrap(),
        Err(_) => axum::response::Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(axum::body::Body::empty())
            .unwrap(),
    }
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

// ── Unit tests ────────────────────────────────────────────────────────────────
//
// Layer   : pure unit — no I/O, no HTTP
// Covers  : parse_qualify() — block extraction, display-text trimming,
//           before+after text reconstruction, JSON parse failure, absent block
// Does NOT cover: the AI response generation, Anthropic API, session handling,
//                 email confirmation flow
#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_qualify ──────────────────────────────────────────────────────

    #[test]
    fn parse_qualify_extracts_block_and_strips_display() {
        let text = r#"Here is my answer.
```qualify
{"client_need":"automate orders","summary":"sector|pain|tools|volume"}
```"#;
        let (display, block) = parse_qualify(text);
        assert_eq!(display.trim(), "Here is my answer.");
        let b = block.expect("block must be present");
        assert_eq!(b.client_need, "automate orders");
        assert_eq!(b.summary, "sector|pain|tools|volume");
    }

    #[test]
    fn parse_qualify_no_block_returns_text_unchanged() {
        let text = "Just a normal AI response.";
        let (display, block) = parse_qualify(text);
        assert_eq!(display, text);
        assert!(block.is_none());
    }

    #[test]
    fn parse_qualify_invalid_json_returns_none_block() {
        let text = "Before\n```qualify\nnot-valid-json\n```\nAfter";
        let (_, block) = parse_qualify(text);
        assert!(block.is_none(), "malformed JSON must not crash");
    }

    #[test]
    fn parse_qualify_preserves_before_and_after_text() {
        let text = "BEFORE\n```qualify\n{\"client_need\":\"n\",\"summary\":\"s\"}\n```\nAFTER";
        let (display, block) = parse_qualify(text);
        assert!(display.contains("BEFORE"));
        assert!(display.contains("AFTER"));
        assert!(block.is_some());
    }

    #[test]
    fn parse_qualify_empty_display_when_only_block() {
        let text = "```qualify\n{\"client_need\":\"n\",\"summary\":\"s\"}\n```";
        let (display, _) = parse_qualify(text);
        // Display should be empty (no content outside the block)
        assert!(display.is_empty());
    }

    #[test]
    fn parse_qualify_block_only_before() {
        let text = "Visible text\n```qualify\n{\"client_need\":\"x\",\"summary\":\"y\"}\n```";
        let (display, block) = parse_qualify(text);
        assert_eq!(display.trim(), "Visible text");
        assert!(block.is_some());
    }

    // ── parse_options ──────────────────────────────────────────────────────

    #[test]
    fn parse_options_extracts_labels_and_strips_block() {
        let text = "Quel est votre secteur ?\n```options\n[{\"label\":\"E-commerce\"},{\"label\":\"Santé\"},{\"label\":\"Autre\"}]\n```";
        let (display, opts) = parse_options(text);
        assert_eq!(display.trim(), "Quel est votre secteur ?");
        assert_eq!(opts.len(), 3);
        assert_eq!(opts[0].label, "E-commerce");
        assert_eq!(opts[2].label, "Autre");
    }

    #[test]
    fn parse_options_no_block_returns_text_unchanged() {
        let text = "A normal reply with no options.";
        let (display, opts) = parse_options(text);
        assert_eq!(display, text);
        assert!(opts.is_empty());
    }

    #[test]
    fn parse_options_invalid_json_returns_empty_vec() {
        let text = "Before\n```options\nnot-json\n```\nAfter";
        let (_, opts) = parse_options(text);
        assert!(opts.is_empty(), "malformed JSON must degrade gracefully");
    }

    #[test]
    fn parse_options_preserves_before_and_after_text() {
        let text = "BEFORE\n```options\n[{\"label\":\"A\"}]\n```\nAFTER";
        let (display, opts) = parse_options(text);
        assert!(display.contains("BEFORE"));
        assert!(display.contains("AFTER"));
        assert_eq!(opts.len(), 1);
    }

    #[test]
    fn parse_options_empty_array_yields_no_options() {
        let text = "Pick one\n```options\n[]\n```";
        let (display, opts) = parse_options(text);
        assert_eq!(display.trim(), "Pick one");
        assert!(opts.is_empty());
    }

    // ── real_ip ────────────────────────────────────────────────────────────

    #[test]
    fn real_ip_prefers_x_forwarded_for() {
        use axum::http::HeaderMap;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.1, 10.0.0.1".parse().unwrap());
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 4321);
        let ip = real_ip(addr, &headers);
        assert_eq!(ip, "203.0.113.1");
    }

    #[test]
    fn real_ip_falls_back_to_socket_addr() {
        use axum::http::HeaderMap;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        let headers = HeaderMap::new();
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 5678);
        let ip = real_ip(addr, &headers);
        assert_eq!(ip, "192.168.1.1");
    }
}
