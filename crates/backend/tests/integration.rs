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
    extract::State,
    http::{HeaderName, HeaderValue, StatusCode},
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
use std::sync::Arc;

// ── Shared test helpers ────────────────────────────────────────────────────────

fn test_state() -> Arc<AppState> {
    state_with_stripe(StripeClient::new(
        reqwest::Client::new(),
        "sk_test_fake".to_string(),
        "whsec_test_fake_webhook_secret".to_string(),
    ))
}

/// Same as `test_state` but with a caller-supplied StripeClient — lets the live
/// checkout test inject a real `sk_test_...` key without cloning AppState.
fn state_with_stripe(stripe: StripeClient) -> Arc<AppState> {
    Arc::new(AppState {
        anthropic_key: "sk-fake-for-tests".to_string(),
        http: reqwest::Client::new(),
        system_prompt: "You are a test assistant.".to_string(),
        langfuse: None,
        sessions: SessionStore::new(),
        pipelines: PipelineStore::new(),
        pending: backend_lib::pending::PendingStore::new(),
        pitches: PitchStore::new(None),
        qdrant: None,
        embeddings: None,
        cloudflare: None,
        n8n_mcp: None,
        stripe: Some(stripe),
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

// ── GET /api/admin/dossiers ─────────────────────────────────────────────────────

fn dossiers_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/admin/dossiers", get(backend_lib::handlers::admin::dossiers))
        .with_state(state)
}

#[tokio::test]
async fn admin_dossiers_requires_token() {
    let state = test_state();
    let app = dossiers_router(state);
    let server = TestServer::new(app).unwrap();

    let resp = server.get("/api/admin/dossiers").await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_dossiers_lists_pipeline_with_pitch_when_authed() {
    let state = test_state();
    let pid = state.pipelines.create(
        "sess-admin".to_string(),
        "Automatiser la relance client".to_string(),
        Some("résumé qualif".to_string()),
    ).await;
    state.pitches.set(&pid.to_string(), PitchResult {
        solution_desc: "Workflow de relance".to_string(),
        price_eur_cents: 240_000,
        price_validity: "valable 48h".to_string(),
        externals_needed: vec![],
        slides: vec![],
        manual_quote: false,
    }).await;

    let app = dossiers_router(state);
    let server = TestServer::new(app).unwrap();

    let resp = server
        .get("/api/admin/dossiers")
        .add_header(HeaderName::from_static("x-admin-token"), HeaderValue::from_static("integration-admin-token"))
        .await;
    resp.assert_status_ok();

    let body: Value = resp.json();
    let arr = body.as_array().expect("dossiers is an array");
    assert_eq!(arr.len(), 1);
    let d = &arr[0];
    assert_eq!(d["pipeline_id"], pid.to_string());
    assert_eq!(d["client_need"], "Automatiser la relance client");
    assert_eq!(d["stage"], "qualifying");
    assert_eq!(d["pitch"]["price_eur_cents"], 240_000);
}

// ── POST /api/admin/dossiers/:id/respawn ────────────────────────────────────────

fn respawn_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/api/admin/dossiers/:id/respawn", post(backend_lib::handlers::admin::respawn))
        .with_state(state)
}

#[tokio::test]
async fn admin_respawn_requires_token() {
    let state = test_state();
    let app = respawn_router(state);
    let server = TestServer::new(app).unwrap();

    let resp = server.post("/api/admin/dossiers/whatever/respawn").await;
    resp.assert_status(StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_respawn_unknown_dossier_returns_404() {
    let state = test_state();
    let app = respawn_router(state);
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/admin/dossiers/00000000-0000-0000-0000-000000000000/respawn")
        .add_header(HeaderName::from_static("x-admin-token"), HeaderValue::from_static("integration-admin-token"))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_respawn_existing_dossier_returns_fresh_pipeline_id() {
    let state = test_state();
    let pid = state.pipelines.create(
        "sess-respawn".to_string(),
        "need to re-run".to_string(),
        Some("résumé".to_string()),
    ).await;

    let app = respawn_router(state);
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post(&format!("/api/admin/dossiers/{}/respawn", pid))
        .add_header(HeaderName::from_static("x-admin-token"), HeaderValue::from_static("integration-admin-token"))
        .await;
    resp.assert_status_ok();

    let body: Value = resp.json();
    let new_id = body["pipeline_id"].as_str().expect("new pipeline_id");
    assert_ne!(new_id, pid.to_string(), "respawn creates a fresh pipeline keyed by a new id");
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

/// Signs a raw webhook body with the test webhook secret the way Stripe does, and
/// returns the full `Stripe-Signature` header value (`t=<ts>,v1=<hmac>`).
fn sign_stripe_webhook(raw: &[u8]) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
    let signed = format!("{ts}.{}", String::from_utf8_lossy(raw));
    let mut mac = Hmac::<Sha256>::new_from_slice(b"whsec_test_fake_webhook_secret").unwrap();
    mac.update(signed.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    format!("t={ts},v1={sig}")
}

/// Seeds a pipeline parked at AwaitingPayment with a quoted price, exactly as the
/// live pipeline leaves it just before the client pays.
async fn seed_awaiting_payment(state: &std::sync::Arc<AppState>, session_id: &str) -> uuid::Uuid {
    use backend_lib::pipeline::PipelineStage;
    let pid = state.pipelines.create(
        session_id.to_string(),
        "Automatiser la facturation".to_string(),
        Some("résumé qualif".to_string()),
    ).await;
    let mut ctx = state.pipelines.get_ctx(pid).await.unwrap();
    ctx.price_quote = Some(1200);
    ctx.price_monthly = Some(90);
    state.pipelines.advance(pid, PipelineStage::AwaitingPayment, ctx).await;
    pid
}

/// POSTs a signed body to the webhook route and returns the test response.
async fn post_signed_webhook(server: &TestServer, raw: Vec<u8>) -> axum_test::TestResponse {
    let sig_header = sign_stripe_webhook(&raw);
    server
        .post("/api/stripe/webhook")
        .add_header("stripe-signature", sig_header.parse::<HeaderValue>().unwrap())
        .bytes(raw.into())
        .content_type("application/json")
        .await
}

// ── POST /api/stripe/webhook — paid checkout resumes a pipeline (E2E seam) ──────
//
// The genuine encaissabilité seam: a *correctly-signed* `checkout.session.completed`
// with `payment_status: "paid"`, carrying a pipeline's `metadata.pipeline_id`, must
// move that pipeline out of AwaitingPayment (resume_after_payment → Decomposing,
// then spawn() drives it on). Proves "payment captured → funnel advances", with a
// real HMAC over the raw body.
#[tokio::test]
async fn stripe_webhook_paid_checkout_resumes_pipeline() {
    use backend_lib::pipeline::PipelineStage;

    let state = test_state();
    let pid = seed_awaiting_payment(&state, "sess-pay").await;

    let app = stripe_webhook_router(state.clone());
    let server = TestServer::new(app).unwrap();

    // Event Stripe POSTs after a card payment captures: completed + paid.
    let payload = json!({
        "type": "checkout.session.completed",
        "data": { "object": {
            "payment_status": "paid",
            "metadata": { "pipeline_id": pid.to_string() }
        } }
    });
    let raw = serde_json::to_vec(&payload).unwrap();

    let resp = post_signed_webhook(&server, raw).await;
    resp.assert_status_ok(); // Stripe always wants a 200 so it stops retrying.

    // The pipeline must have left AwaitingPayment — payment captured advanced it.
    let (stage, _) = state.pipelines.status(pid).await.expect("pipeline still exists");
    assert_ne!(
        stage, PipelineStage::AwaitingPayment,
        "a paid checkout.session.completed must resume the pipeline, got {stage:?}"
    );
}

// ── POST /api/stripe/webhook — async-PENDING completed must NOT resume ───────────
//
// Security/correctness guard: for async payment methods (SEPA, Bancontact…) Stripe
// fires `checkout.session.completed` with `payment_status: "unpaid"` BEFORE the money
// clears. We must NOT resume the pipeline yet — otherwise we'd deliver work for a
// payment that could still fail. The pipeline must stay parked at AwaitingPayment.
#[tokio::test]
async fn stripe_webhook_completed_unpaid_does_not_resume() {
    use backend_lib::pipeline::PipelineStage;

    let state = test_state();
    let pid = seed_awaiting_payment(&state, "sess-async-pending").await;

    let app = stripe_webhook_router(state.clone());
    let server = TestServer::new(app).unwrap();

    let payload = json!({
        "type": "checkout.session.completed",
        "data": { "object": {
            "payment_status": "unpaid",
            "metadata": { "pipeline_id": pid.to_string() }
        } }
    });
    let raw = serde_json::to_vec(&payload).unwrap();

    let resp = post_signed_webhook(&server, raw).await;
    resp.assert_status_ok(); // still 200 — we acknowledge but take no action.

    // Must remain parked: no money captured yet.
    let (stage, _) = state.pipelines.status(pid).await.expect("pipeline still exists");
    assert_eq!(
        stage, PipelineStage::AwaitingPayment,
        "an unpaid (async-pending) completed event must NOT resume the pipeline, got {stage:?}"
    );
}

// ── POST /api/stripe/webhook — async_payment_succeeded resumes the pipeline ──────
//
// The real money-confirmed signal for async methods. Once SEPA/etc. clears, Stripe
// fires `checkout.session.async_payment_succeeded`; that must resume the pipeline that
// the earlier (unpaid) `completed` deliberately left parked.
#[tokio::test]
async fn stripe_webhook_async_payment_succeeded_resumes_pipeline() {
    use backend_lib::pipeline::PipelineStage;

    let state = test_state();
    let pid = seed_awaiting_payment(&state, "sess-async-ok").await;

    let app = stripe_webhook_router(state.clone());
    let server = TestServer::new(app).unwrap();

    let payload = json!({
        "type": "checkout.session.async_payment_succeeded",
        "data": { "object": {
            "payment_status": "paid",
            "metadata": { "pipeline_id": pid.to_string() }
        } }
    });
    let raw = serde_json::to_vec(&payload).unwrap();

    let resp = post_signed_webhook(&server, raw).await;
    resp.assert_status_ok();

    let (stage, _) = state.pipelines.status(pid).await.expect("pipeline still exists");
    assert_ne!(
        stage, PipelineStage::AwaitingPayment,
        "async_payment_succeeded must resume the pipeline, got {stage:?}"
    );
}

// ── POST /api/stripe/webhook — async_payment_failed must NOT resume ─────────────
//
// A bounced async payment must never advance delivery. The pipeline stays parked at
// AwaitingPayment so the client can retry — we don't deliver work that wasn't paid for.
#[tokio::test]
async fn stripe_webhook_async_payment_failed_does_not_resume() {
    use backend_lib::pipeline::PipelineStage;

    let state = test_state();
    let pid = seed_awaiting_payment(&state, "sess-async-fail").await;

    let app = stripe_webhook_router(state.clone());
    let server = TestServer::new(app).unwrap();

    let payload = json!({
        "type": "checkout.session.async_payment_failed",
        "data": { "object": {
            "payment_status": "unpaid",
            "metadata": { "pipeline_id": pid.to_string() }
        } }
    });
    let raw = serde_json::to_vec(&payload).unwrap();

    let resp = post_signed_webhook(&server, raw).await;
    resp.assert_status_ok();

    let (stage, _) = state.pipelines.status(pid).await.expect("pipeline still exists");
    assert_eq!(
        stage, PipelineStage::AwaitingPayment,
        "a failed async payment must leave the pipeline parked, got {stage:?}"
    );
}

// ── POST /api/stripe/checkout — creates a real Stripe session (test mode) ──────
//
// Hits the live Stripe **test** API, so it's #[ignore]d by default and only runs
// when STRIPE_TEST_SECRET_KEY is exported:
//   STRIPE_TEST_SECRET_KEY=sk_test_... cargo test -p backend \
//     --test integration -- --ignored stripe_checkout_creates_real_session --nocapture
// Proves we can actually create a payable Checkout Session from a quoted pipeline.
#[tokio::test]
#[ignore = "calls live Stripe test API; needs STRIPE_TEST_SECRET_KEY"]
async fn stripe_checkout_creates_real_session() {
    use backend_lib::pipeline::PipelineStage;

    let secret = std::env::var("STRIPE_TEST_SECRET_KEY")
        .expect("set STRIPE_TEST_SECRET_KEY (sk_test_...) to run this test");
    assert!(secret.starts_with("sk_test_"), "must use a TEST key, never live");

    // Build state carrying a real test StripeClient.
    let state = state_with_stripe(StripeClient::new(
        reqwest::Client::new(),
        secret,
        "whsec_unused_for_checkout".to_string(),
    ));

    // Seed a quoted pipeline at AwaitingPayment, as the live funnel would.
    let pid = state.pipelines.create(
        "sess-checkout".to_string(),
        "Automatiser les devis".to_string(),
        None,
    ).await;
    let mut ctx = state.pipelines.get_ctx(pid).await.unwrap();
    ctx.price_quote = Some(1500);
    ctx.price_monthly = Some(120);
    ctx.workflow_json = Some(json!({ "name": "Devis automatiques" }));
    state.pipelines.advance(pid, PipelineStage::AwaitingPayment, ctx).await;

    let app = Router::new()
        .route("/api/stripe/checkout", post(backend_lib::handlers::stripe::create_checkout))
        .with_state(state);
    let server = TestServer::new(app).unwrap();

    let resp = server
        .post("/api/stripe/checkout")
        .json(&json!({ "pipeline_id": pid.to_string() }))
        .await;
    resp.assert_status_ok();

    let body: Value = resp.json();
    let url = body["checkout_url"].as_str().expect("checkout_url present");
    let sid = body["session_id"].as_str().expect("session_id present");
    println!("[encaissabilité] real Stripe TEST session created:\n  id={sid}\n  url={url}");
    assert!(url.contains("checkout.stripe.com"), "expected hosted Stripe URL, got {url}");
    assert!(sid.starts_with("cs_test_"), "expected a TEST session id, got {sid}");
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
