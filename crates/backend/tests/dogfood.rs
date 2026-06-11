//! Dogfood harness — drives the REAL pipeline agents against a concrete brief
//! and prints the full trace, so we can study what pointe.dev actually produces.
//!
//! Mirrors the redesigned flow with its payment boundary:
//!   PRE-payment : qualifier → research → designer → design_critic → pricing  (NO JSON)
//!   POST-payment: builder → critic                                           (real JSON)
//!
//! Gated on a real ANTHROPIC_API_KEY (makes live LLM calls, costs a few cents),
//! like the cloudflare `embeds_live` smoke test. CF creds are read from env too:
//! present + allowed IP → RAG grounding is exercised; otherwise the builder falls
//! back to an empty RAG block (graceful) and we observe the LLM-only baseline.
//!
//! Run:
//!   set -a; . <(grep -E '^(ANTHROPIC_API_KEY|CF_)' .env.prod | sed 's/\r$//'); set +a
//!   cargo test -p backend --test dogfood -- --ignored --nocapture dogfood_full_pipeline

use std::sync::Arc;

use backend_lib::{
    pipeline::{PipelineStore, MAX_DESIGN_ATTEMPTS, MAX_BUILD_ATTEMPTS},
    pitch::PitchStore,
    sessions::SessionStore,
    state::AppState,
};

/// Minimal AppState wired with a REAL Anthropic key (from env) and CF from env.
fn dogfood_state() -> Arc<AppState> {
    let anthropic_key =
        std::env::var("ANTHROPIC_API_KEY").expect("set ANTHROPIC_API_KEY to run the dogfood test");
    let http = reqwest::Client::new();
    let cloudflare = backend_lib::cloudflare::CloudflareRag::from_env(http.clone());

    Arc::new(AppState {
        anthropic_key,
        http,
        system_prompt: String::new(),
        langfuse: None,
        sessions: SessionStore::new(),
        pipelines: PipelineStore::new(),
        pending: backend_lib::pending::PendingStore::new(),
        pitches: PitchStore::new(None),
        qdrant: None,
        embeddings: None,
        cloudflare,
        n8n_mcp: backend_lib::mcp::N8nMcpConfig::from_env(),
        stripe: None,
        session_secret: b"dogfood-secret".to_vec(),
        resend_api_key: None,
        base_url: "http://localhost".to_string(),
        owner_email: None,
        db: None,
        admin_ingest_token: None,
    })
}

const BRIEF: &str = "Je veux une chaîne YouTube Shorts et Instagram Reels entièrement \
automatisée sur l'actualité de l'IA. Chaque jour le système doit : surveiller les \
dernières actus IA (flux RSS, X/Twitter), choisir le sujet le plus viral, écrire un \
script court de 30 à 45 secondes, générer une voix off avec ElevenLabs, monter une \
vidéo verticale 9:16 avec sous-titres via Creatomate, puis publier automatiquement sur \
YouTube et Instagram avec un titre et des hashtags optimisés. Fréquence : 1 à 3 vidéos \
par jour. Je n'ai aucune équipe, ça doit tourner tout seul.";

#[tokio::test]
#[ignore]
async fn dogfood_full_pipeline() {
    let app = dogfood_state();
    println!("\n========== DOGFOOD: brief « chaîne IA » ==========");
    println!("RAG (cloudflare) configuré: {}", app.cloudflare.is_some());

    let store = PipelineStore::new();
    let id = store
        .create("dogfood-session".to_string(), BRIEF.to_string(), None)
        .await;
    let mut ctx = store.get_ctx(id).await.expect("ctx");

    // ───────── PRE-PAYMENT: qualification only, NO JSON ─────────
    backend_lib::agents::run_qualifier(&app, &mut ctx).await.expect("qualifier");
    println!("\n----- 1. QUALIFICATION SUMMARY -----\n{}",
        ctx.qualification_summary.as_deref().unwrap_or("(none)"));

    backend_lib::agents::run_research(&app, &mut ctx).await.expect("research");
    println!("\n----- 2. RESEARCH OUTPUT -----\n{}",
        ctx.research_output.as_deref().unwrap_or("(none)"));

    // Designer + design critic loop (high-level blueprint, no JSON)
    let mut design_ok = false;
    for _ in 1..=MAX_DESIGN_ATTEMPTS {
        ctx.design_attempts += 1;
        backend_lib::agents::run_designer(&app, &mut ctx).await.expect("designer");
        println!("\n----- 3. SOLUTION DESIGN (attempt {}) -----\n{}",
            ctx.design_attempts, ctx.design_summary.as_deref().unwrap_or("(none)"));

        design_ok = backend_lib::agents::run_design_critic(&app, &mut ctx).await.expect("design_critic");
        println!("\n----- 4. DESIGN CRITIC (attempt {}): approved={design_ok} -----", ctx.design_attempts);
        if let Some(fb) = ctx.design_critic_feedback.last() {
            println!("feedback: {fb}");
        }
        if design_ok { break; }
    }

    backend_lib::agents::run_pricing(&app, &mut ctx).await.expect("pricing");
    println!("\n----- 5. PRICING -----\nsetup={:?}€  monthly={:?}€\njustification: {}",
        ctx.price_quote, ctx.price_monthly,
        ctx.price_justification.as_deref().unwrap_or("(none)"));

    // The whole point of the redesign: no workflow JSON exists before payment.
    assert!(ctx.workflow_json.is_none(), "no JSON should be built pre-payment");
    println!("\n===== 💳 PAYMENT BOUNDARY — workflow_json is None (correct) =====");

    // ───────── POST-PAYMENT: build the real JSON ─────────
    let mut build_ok = false;
    for _ in 1..=MAX_BUILD_ATTEMPTS {
        ctx.build_attempts += 1;
        backend_lib::agents::run_builder(&app, &mut ctx).await.expect("builder");
        println!("\n----- 6. WORKFLOW JSON (attempt {}) -----\n{}",
            ctx.build_attempts,
            serde_json::to_string_pretty(ctx.workflow_json.as_ref().expect("workflow_json")).unwrap_or_default());

        build_ok = backend_lib::agents::run_critic(&app, &mut ctx).await.expect("critic");
        println!("\n----- 7. BUILD CRITIC (attempt {}): approved={build_ok} -----", ctx.build_attempts);
        if let Some(fb) = ctx.critic_feedback.last() {
            println!("feedback: {fb}");
        }
        if build_ok { break; }
    }

    println!("\n========== FIN — design_ok={design_ok} build_ok={build_ok} ==========\n");
    assert!(ctx.workflow_json.is_some(), "builder should have produced something to inspect");
}
