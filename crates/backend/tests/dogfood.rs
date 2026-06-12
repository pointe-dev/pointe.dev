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
    pipeline::{PipelineStore, PipelineContext, SubWorkflowPlan, MAX_DESIGN_ATTEMPTS, MAX_BUILD_ATTEMPTS},
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

/// Drives qualifier → research → designer, then the decomposition gate +
/// run_decomposer on the hard "chaîne IA" brief, and prints the split. Lets us see
/// whether the 7-integration tunnel is cut into sensible ≤8-node sub-flows with
/// real input/output contracts. Same env gating as dogfood_full_pipeline.
///
/// Run:
///   cargo test -p backend --test dogfood -- --ignored --nocapture dogfood_decomposition
#[tokio::test]
#[ignore]
async fn dogfood_decomposition() {
    use backend_lib::agents::{needs_decomposition, run_decomposer};

    let app = dogfood_state();
    println!("\n========== DOGFOOD: décomposition « chaîne IA » ==========");

    let store = PipelineStore::new();
    let id = store
        .create("dogfood-decomp".to_string(), BRIEF.to_string(), None)
        .await;
    let mut ctx = store.get_ctx(id).await.expect("ctx");

    backend_lib::agents::run_qualifier(&app, &mut ctx).await.expect("qualifier");
    backend_lib::agents::run_research(&app, &mut ctx).await.expect("research");
    ctx.design_attempts += 1;
    backend_lib::agents::run_designer(&app, &mut ctx).await.expect("designer");
    println!("\n----- SOLUTION DESIGN -----\n{}",
        ctx.design_summary.as_deref().unwrap_or("(none)"));

    let gate = needs_decomposition(&ctx);
    println!("\n----- GATE: needs_decomposition = {gate} -----");
    assert!(gate, "the 7-integration AI-video brief must trip the decomposition gate");

    run_decomposer(&app, &mut ctx).await.expect("decomposer");
    println!("\n----- DÉCOMPOSITION: {} sous-flux -----", ctx.sub_workflows.len());
    for (i, wf) in ctx.sub_workflows.iter().enumerate() {
        println!("\n[{}] {}\n  rôle:    {}\n  trigger: {}\n  in:      {}\n  out:     {}",
            i + 1, wf.name, wf.description, wf.trigger, wf.input_contract, wf.output_contract);
    }

    assert!(ctx.sub_workflows.len() >= 2, "a 7-integration tunnel should split into multiple sub-flows");
}

/// Full tranche-2 build path WITHOUT n8n: decompose, then build every sub-flow in
/// sub-flow mode, and verify the chaining convention the deploy wiring relies on —
/// non-first sub-flows trigger on Execute Workflow Trigger, non-last sub-flows end
/// with an Execute Workflow node referencing the NEXT sub-flow by name. (The n8n
/// POST/wiring is unit-tested separately; this checks the model emits the contract.)
///
/// Run:
///   cargo test -p backend --test dogfood -- --ignored --nocapture dogfood_decomposed_build
#[tokio::test]
#[ignore]
async fn dogfood_decomposed_build() {
    use backend_lib::agents::{needs_decomposition, run_builder, run_decomposer};

    let app = dogfood_state();
    println!("\n========== DOGFOOD: build décomposé « chaîne IA » ==========");
    println!("grounding MCP actif: {}", app.n8n_mcp.is_some());

    let store = PipelineStore::new();
    let id = store.create("dogfood-decomp-build".to_string(), BRIEF.to_string(), None).await;
    let mut ctx = store.get_ctx(id).await.expect("ctx");

    backend_lib::agents::run_qualifier(&app, &mut ctx).await.expect("qualifier");
    backend_lib::agents::run_research(&app, &mut ctx).await.expect("research");
    ctx.design_attempts += 1;
    backend_lib::agents::run_designer(&app, &mut ctx).await.expect("designer");
    assert!(needs_decomposition(&ctx), "brief must trip the gate");
    run_decomposer(&app, &mut ctx).await.expect("decomposer");
    let n = ctx.sub_workflows.len();
    println!("\n----- {n} sous-flux à construire -----");

    // Mimic the state machine's per-sub-flow build loop (single build each, no
    // critic retry — we only inspect structure here).
    for cursor in 0..n {
        ctx.build_cursor = cursor;
        run_builder(&app, &mut ctx).await.expect("builder");
        let wf = ctx.workflow_json.take().expect("workflow_json");
        ctx.built_workflows.push(wf);
    }

    let mut chain_ok = true;
    for (i, wf) in ctx.built_workflows.iter().enumerate() {
        let nodes = wf["nodes"].as_array().cloned().unwrap_or_default();
        let types: Vec<String> = nodes.iter()
            .filter_map(|nd| nd["type"].as_str().map(str::to_string)).collect();
        println!("\n[{}] {} — {} nœuds\n  types: {}",
            i + 1, ctx.sub_workflows[i].name, nodes.len(), types.join(", "));

        // non-first → must enter on an Execute Workflow Trigger
        if i > 0 {
            let has_trigger = types.iter().any(|t| t.contains("executeWorkflowTrigger"));
            println!("  entrée executeWorkflowTrigger: {has_trigger}");
            chain_ok &= has_trigger;
        }
        // non-last → must hand off via an Execute Workflow node naming the next sub-flow
        if i + 1 < n {
            let next_name = &ctx.sub_workflows[i + 1].name;
            let refs_next = nodes.iter().any(|nd| {
                nd["type"].as_str().map(|t| t.contains("executeWorkflow") && !t.contains("Trigger")).unwrap_or(false)
                    && nd["parameters"]["workflowId"].as_str() == Some(next_name.as_str())
                    || nd["parameters"]["workflowId"]["value"].as_str() == Some(next_name.as_str())
            });
            println!("  hand-off → '{next_name}': {refs_next}");
            chain_ok &= refs_next;
        }
        assert!(nodes.len() <= 10, "sub-flow {} has {} nodes (>10, budget overrun)", i + 1, nodes.len());
    }

    println!("\n========== chaînage complet émis par le builder: {chain_ok} ==========\n");
    // Soft signal: the model should follow the convention, but a miss is recoverable
    // (deploy logs a warning and the owner wires manually), so we don't hard-fail.
    if !chain_ok {
        println!("⚠️  au moins un maillon de chaînage manque — à durcir côté prompt si récurrent");
    }
}

/// Live deploy of the chained-sub-flow path against the REAL n8n at N8N_URL — the
/// only boundary the other dogfoods can't cover. Uses two tiny synthetic sub-flows
/// built from 100% real node types (no LLM, no cost), so it isolates run_deploy:
/// REST create in reverse order, name→id wiring of the executeWorkflow placeholder,
/// and entry activation. Prints the created ids so they can be inspected/archived.
///
/// Creates REAL workflows named "[TEST] test_pointe — …". Archive them afterwards.
///
/// Run:
///   set -a; . <(grep -E '^(N8N_URL|N8N_API_KEY)=' .env.prod | sed 's/\r$//'); set +a
///   cargo test -p backend --test dogfood -- --ignored --nocapture dogfood_deploy_live
#[tokio::test]
#[ignore]
async fn dogfood_deploy_live() {
    std::env::var("N8N_API_KEY").expect("set N8N_URL + N8N_API_KEY to run the live deploy test");

    // AppState only needs http + anthropic_key (unused here). Reuse the dogfood state
    // but Anthropic key may be absent; fall back to a dummy since run_deploy never calls it.
    let http = reqwest::Client::new();
    let app = Arc::new(AppState {
        anthropic_key: std::env::var("ANTHROPIC_API_KEY").unwrap_or_else(|_| "unused".into()),
        http,
        system_prompt: String::new(),
        langfuse: None,
        sessions: SessionStore::new(),
        pipelines: PipelineStore::new(),
        pending: backend_lib::pending::PendingStore::new(),
        pitches: PitchStore::new(None),
        qdrant: None,
        embeddings: None,
        cloudflare: None,
        n8n_mcp: None,
        stripe: None,
        session_secret: b"dogfood".to_vec(),
        resend_api_key: None,
        base_url: "http://localhost".to_string(),
        owner_email: None,
        db: None,
        admin_ingest_token: None,
    });

    let mut ctx = PipelineContext {
        session_id: "deploy-live".to_string(),
        client_need: "[TEST] test_pointe deploy".to_string(),
        ..Default::default()
    };
    ctx.sub_workflows = vec![
        SubWorkflowPlan {
            name: "[TEST] test_pointe — WF-1 Entry".to_string(),
            description: "schedule → set → call WF-2".to_string(),
            trigger: "scheduleTrigger".to_string(),
            input_contract: String::new(),
            output_contract: "ping".to_string(),
        },
        SubWorkflowPlan {
            name: "[TEST] test_pointe — WF-2 Sink".to_string(),
            description: "execute-workflow-trigger → set".to_string(),
            trigger: "executeWorkflowTrigger".to_string(),
            input_contract: "ping".to_string(),
            output_contract: String::new(),
        },
    ];
    // WF-1: schedule → set → executeWorkflow(workflowId = next sub-flow NAME placeholder)
    ctx.built_workflows = vec![
        serde_json::json!({
            "name": "[TEST] test_pointe — WF-1 Entry",
            "nodes": [
                {"name": "Every day", "type": "n8n-nodes-base.scheduleTrigger", "typeVersion": 1.1, "parameters": {}, "credentials": {}},
                {"name": "Make ping", "type": "n8n-nodes-base.set", "typeVersion": 3, "parameters": {}},
                {"name": "Call WF-2", "type": "n8n-nodes-base.executeWorkflow", "typeVersion": 1,
                 "parameters": {"workflowId": "[TEST] test_pointe — WF-2 Sink"}}
            ],
            "connections": {
                "Every day": {"main": [[{"node": "Make ping", "type": "main", "index": 0}]]},
                "Make ping": {"main": [[{"node": "Call WF-2", "type": "main", "index": 0}]]}
            }
        }),
        serde_json::json!({
            "name": "[TEST] test_pointe — WF-2 Sink",
            "nodes": [
                {"name": "When called", "type": "n8n-nodes-base.executeWorkflowTrigger", "typeVersion": 1, "parameters": {}},
                {"name": "Store", "type": "n8n-nodes-base.set", "typeVersion": 3, "parameters": {}}
            ],
            "connections": {
                "When called": {"main": [[{"node": "Store", "type": "main", "index": 0}]]}
            }
        }),
    ];

    println!("\n========== DOGFOOD: deploy live test_pointe → {} ==========",
        std::env::var("N8N_URL").unwrap_or_default());

    backend_lib::agents::run_deploy(&app, &mut ctx).await.expect("run_deploy");

    println!("\nworkflow ids (entry first): {:?}", ctx.n8n_workflow_ids);
    println!("entry id:  {:?}", ctx.n8n_workflow_id);
    println!("entry url: {:?}", ctx.n8n_workflow_url);
    assert_eq!(ctx.n8n_workflow_ids.len(), 2, "both sub-flows should be created");
    println!("\n⚠️  ARCHIVE these workflows after inspection: {:?}", ctx.n8n_workflow_ids);
}
