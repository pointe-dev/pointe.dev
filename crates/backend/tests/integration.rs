//! Integration tests — layer: HTTP handler wiring
//!
//! Spins up real Axum handlers with in-memory state (no external DB, no
//! Anthropic calls, no Stripe API).  Verifies HTTP status codes, response
//! shapes, and error paths.
//!
//! Does NOT cover:
//!   - AI chat quality / prompt correctness
//!   - Stripe payment flow (requires live Stripe API)
//!   - Email delivery (RESEND_API_KEY not set in tests)
//!   - Postgres write-through (PitchStore created with None db)
//!   - Pipeline run() state machine (requires Anthropic API)

use axum::{
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    routing::{get, post},
    Json, Router,
};
use axum_test::TestServer;
use backend_lib::{
    pipeline::PipelineStore,
    pitch::{PitchResult, PitchSlide, PitchStore},
    sessions::SessionStore,
    state::AppState,
    stripe::StripeClient,
};
use serde_json::{json, Value};
use std::{net::SocketAddr, sync::Arc};

// ── Shared test helpers ────────────────────────────────────────────────────────

fn test_state() -> Arc<AppState> {
    Arc::new(AppState {
        anthropic_key: "sk-fake-for-tests".to_string(),
        http: reqwest::Client::new(),
        system_prompt: "You are a test assistant.".to_string(),
        langfuse: None,
        sessions: SessionStore::new(),
        pipelines: PipelineStore::new(),
        pitches: PitchStore::new(None),
        qdrant: None,
        embeddings: None,
        stripe: Some(StripeClient::new(
            reqwest::Client::new(),
            "sk_test_fake".to_string(),
            "whsec_test_fake_webhook_secret".to_string(),
        )),
        session_secret: b"integration-test-secret".to_vec(),
        resend_api_key: None,
        base_url: "http://localhost".to_string(),
        owner_email: None,
        db: None,
        admin_ingest_token: Some("integration-admin-token".to_string()),
    })
}

/// Minimal router for handlers that live inside main.rs but are accessible
/// via the re-exported path.  We test each handler in isolation.
fn health_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/health", get(backend_lib::handlers::health::health_check))
        .route("/api/services", get(backend_lib::handlers::services::get_services))
        .with_state(state)
}

fn pitch_router(state: Arc<AppState>) -> Router {
    // We expose the pitch poll and pipeline-result handlers by rebuilding
    // the same routes that main.rs wires up but without the full server.
    use axum::extract::Query as AxQuery;

    // Inline handler matching main.rs handle_pitch_poll
    async fn pitch_poll(
        State(s): State<Arc<AppState>>,
        AxQuery(params): AxQuery<std::collections::HashMap<String, String>>,
    ) -> Json<Value> {
        let sid = params.get("sid").cloned().unwrap_or_default();
        match s.pitches.get(&sid).await {
            Some(r) => Json(json!({
                "ready":            true,
                "manual_quote":     r.manual_quote,
                "solution_desc":    r.solution_desc,
                "price_eur_cents":  r.price_eur_cents,
                "price_validity":   r.price_validity,
                "externals_needed": r.externals_needed,
                "slides":           r.slides,
            })),
            None => Json(json!({ "ready": false })),
        }
    }

    Router::new()
        .route("/api/pitch/result", get(pitch_poll))
        .with_state(state)
}

fn stripe_webhook_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route(
            "/api/stripe/webhook",
            post(backend_lib::handlers::stripe::webhook),
        )
        .with_state(state)
}

fn ingest_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/admin/ingest", post(backend_lib::handlers::ingest::ingest))
        .with_state(state)
}

// ── GET /api/health ────────────────────────────────────────────────────────────

#[tokio::test]
async fn health_check_returns_200() {
    let state = test_state();
    let app = health_router(state);
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/health").await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn health_check_body_contains_healthy() {
    let state = test_state();
    let app = health_router(state);
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/health").await;
    let body = resp.text();
    assert!(body.contains("healthy"), "response body should contain 'healthy': {body}");
}

// ── GET /api/services ──────────────────────────────────────────────────────────

#[tokio::test]
async fn get_services_returns_200_with_services_array() {
    let state = test_state();
    let app = health_router(state);
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/services").await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert!(body["services"].is_array(), "should have a 'services' array");
    assert!(!body["services"].as_array().unwrap().is_empty());
}

// ── POST /api/admin/ingest ────────────────────────────────────────────────────

#[tokio::test]
async fn admin_ingest_requires_token() {
    let state = test_state();
    let app = ingest_router(state);
    let server = TestServer::new(app).unwrap();

    let resp = server.post("/api/admin/ingest").json(&json!([])).await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

// ── GET /api/pitch/result — happy path ────────────────────────────────────────

#[tokio::test]
async fn pitch_result_returns_ready_true_when_found() {
    let state = test_state();
    // Pre-populate the pitch store
    let pitch = PitchResult {
        solution_desc: "Automate Shopify orders".to_string(),
        price_eur_cents: 120_000,
        price_validity: "valable 48h".to_string(),
        externals_needed: vec!["Shopify API key".to_string()],
        slides: vec![PitchSlide {
            title: "Ce que nous avons compris".to_string(),
            body: "Vous saisissez chaque commande à la main.".to_string(),
            points: vec![],
        }],
        manual_quote: false,
    };
    state.pitches.set("test-session-abc", pitch).await;

    let app = pitch_router(state);
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/pitch/result").add_query_param("sid", "test-session-abc").await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(body["ready"], true);
    assert_eq!(body["price_eur_cents"], 120_000);
    assert_eq!(body["price_validity"], "valable 48h");
}

// ── GET /api/pitch/result — not found ─────────────────────────────────────────

#[tokio::test]
async fn pitch_result_returns_ready_false_when_not_found() {
    let state = test_state();
    let app = pitch_router(state);
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/pitch/result").add_query_param("sid", "nonexistent-session").await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert_eq!(body["ready"], false);
}

// ── POST /api/stripe/webhook — missing signature = 400 ────────────────────────

#[tokio::test]
async fn stripe_webhook_missing_sig_returns_400() {
    let state = test_state();
    let app = stripe_webhook_router(state);
    let server = TestServer::new(app).unwrap();

    // No Stripe-Signature header → must return 400 Bad Request
    let resp = server
        .post("/api/stripe/webhook")
        .bytes(b"{\"type\":\"checkout.session.completed\"}".as_slice().into())
        .content_type("application/json")
        .await;

    assert_eq!(
        resp.status_code(),
        StatusCode::BAD_REQUEST,
        "missing Stripe-Signature must be rejected with 400, got {}",
        resp.status_code()
    );
}

// ── POST /api/stripe/webhook — wrong secret = 400 ─────────────────────────────

#[tokio::test]
async fn stripe_webhook_wrong_signature_returns_400() {
    use std::time::{SystemTime, UNIX_EPOCH};
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let state = test_state();
    let app = stripe_webhook_router(state);
    let server = TestServer::new(app).unwrap();

    let payload = b"{\"type\":\"checkout.session.completed\"}";
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;

    // Sign with the WRONG secret (not "whsec_test_fake_webhook_secret")
    let signed = format!("{ts}.{}", String::from_utf8_lossy(payload));
    let mut mac = Hmac::<Sha256>::new_from_slice(b"wrong_secret").unwrap();
    mac.update(signed.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    let sig_header = format!("t={ts},v1={sig}");

    let resp = server
        .post("/api/stripe/webhook")
        .add_header("stripe-signature", sig_header.parse::<HeaderValue>().unwrap())
        .bytes(payload.as_slice().into())
        .content_type("application/json")
        .await;

    assert_eq!(
        resp.status_code(),
        StatusCode::BAD_REQUEST,
        "wrong signature must be rejected with 400, got {}",
        resp.status_code()
    );
}

// ── POST /api/pipeline/start ───────────────────────────────────────────────────

#[tokio::test]
async fn pipeline_start_returns_pipeline_id() {
    let state = test_state();
    let app = Router::new()
        .route("/api/pipeline/start", post(backend_lib::handlers::pipeline::start))
        .with_state(state);
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/pipeline/start")
        .json(&json!({ "session_id": "s1", "client_need": "Automate my invoices" }))
        .await;
    resp.assert_status_ok();
    let body: Value = resp.json();
    assert!(
        body["pipeline_id"].as_str().is_some(),
        "should return a pipeline_id UUID string"
    );
}

// ── GET /api/pipeline/:id — not found ─────────────────────────────────────────

#[tokio::test]
async fn pipeline_status_returns_404_for_unknown_id() {
    let state = test_state();
    let app = Router::new()
        .route("/api/pipeline/:id", get(backend_lib::handlers::pipeline::status))
        .with_state(state);
    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/api/pipeline/00000000-0000-0000-0000-000000000000")
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}
