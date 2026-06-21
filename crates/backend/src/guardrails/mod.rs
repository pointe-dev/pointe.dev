//! Abuse guardrails — a deterministic, explainable check run on a built workflow
//! *before it is activated*. The workflow JSON is translated to ASP facts
//! ([`facts`]) and evaluated against a fixed policy ([`policy.lp`]) by clingo
//! ([`clingo`]). Violations block auto-activation and route the pipeline to human
//! review — we never silently deploy a spam/flood/scraping workflow.
//!
//! Why ASP and not an LLM judge: the abuse signal lives in the *structure* of the
//! workflow (trigger frequency, loops, outbound targets), not in prose. ASP gives
//! a complete, reproducible, auditable verdict — and adding a policy is one rule,
//! not a fragile re-prompt. The LLM still has its place upstream (intent at the
//! qualify stage); this layer is the hard structural gate.

pub mod clingo;
pub mod facts;

use serde_json::Value;

/// The embedded ASP policy. Versioned with the code; one rule per abuse class.
const POLICY: &str = include_str!("policy.lp");

/// A single policy violation, ready to show a human reviewer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub kind: String,
    pub workflow: u32,
    pub node: String,
}

impl Violation {
    /// One-line, human-readable explanation.
    pub fn explain(&self) -> String {
        let what = match self.kind.as_str() {
            "flood" => "high-frequency trigger feeding an outbound action (spam/flood risk)",
            "mass_post" => "loop feeding an external HTTP POST (mass-submission risk)",
            "scrape_loop" => "loop feeding repeated external HTTP GETs (scraping risk)",
            "unowned_bulk_post" => "loop feeding HTTP POSTs to a domain the client does not own (mass submission against a third party)",
            "unowned_flood" => "high-frequency trigger hitting a domain the client does not own (flooding a third-party service)",
            other => other,
        };
        format!("[{}] node \"{}\" — {what}", self.kind, self.node)
    }
}

/// Inputs that depend on *who* the build is for — chiefly the domains the client has
/// proven they control. Used to tell "hammer my own CRM" (fine) apart from "hammer a
/// third party" (abuse). Already-normalised hosts (see `facts::owned_domains`).
#[derive(Debug, Clone, Default)]
pub struct GuardrailContext {
    pub owned_domains: Vec<String>,
}

/// The outcome of a guardrail evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// No policy violation — safe to deploy/activate.
    Allowed,
    /// One or more violations — must NOT auto-activate; route to human review.
    NeedsReview(Vec<Violation>),
    /// The engine could not run (clingo unavailable / errored). The caller decides
    /// whether to proceed (fail-open) or block (fail-closed) based on config.
    Skipped(String),
}

impl Verdict {
    /// Whether deployment may proceed automatically, given the fail mode.
    /// `fail_closed` only affects the `Skipped` case.
    pub fn allows_auto_deploy(&self, fail_closed: bool) -> bool {
        match self {
            Verdict::Allowed => true,
            Verdict::NeedsReview(_) => false,
            Verdict::Skipped(_) => !fail_closed,
        }
    }

    /// A human-readable reason, for the review queue / owner notification.
    pub fn reason(&self) -> String {
        match self {
            Verdict::Allowed => "no guardrail violations".to_string(),
            Verdict::NeedsReview(vs) => {
                let lines: Vec<String> = vs.iter().map(|v| v.explain()).collect();
                format!("guardrail review required:\n- {}", lines.join("\n- "))
            }
            Verdict::Skipped(why) => format!("guardrails skipped: {why}"),
        }
    }
}

/// Reads `GUARDRAILS_FAIL_CLOSED` (default false). When true, a `Skipped` verdict
/// blocks auto-deploy instead of allowing it.
pub fn fail_closed() -> bool {
    std::env::var("GUARDRAILS_FAIL_CLOSED")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE"))
        .unwrap_or(false)
}

/// Evaluate the built workflows against the abuse policy, with no ownership context.
/// Equivalent to `evaluate_with_context` with an empty context: structural rules
/// (flood/mass_post/scrape_loop) fire, ownership rules can't (every known domain is
/// treated as third-party only when ownership facts exist — with none, the
/// `unowned_*` rules still fire on any known third-party domain, which is the safe
/// default when we genuinely don't know what the client owns).
///
/// Pure w.r.t. the workflows; the only side effect is spawning clingo. Safe on any
/// input — unparseable workflow shapes simply yield fewer facts.
///
/// The pipeline always goes through `evaluate_with_context` (it has the client's
/// ownership info); this no-context entry point is kept for callers/tests that have
/// no ownership data.
#[allow(dead_code)]
pub fn evaluate(workflows: &[Value]) -> Verdict {
    evaluate_with_context(workflows, &GuardrailContext::default())
}

/// Assemble the fact block (workflow facts + ownership facts) handed to clingo.
/// Returns None when there are no workflow facts at all (nothing to check).
/// Split out so the exact clingo input is unit-testable without the binary.
fn build_fact_block(workflows: &[Value], ctx: &GuardrailContext) -> Option<String> {
    let mut facts = facts::workflow_facts(workflows);
    if facts.is_empty() {
        return None;
    }
    facts.extend(facts::owned_domain_facts(&ctx.owned_domains));
    Some(facts.join("\n"))
}

/// Evaluate the built workflows against the abuse policy, given what the client owns.
pub fn evaluate_with_context(workflows: &[Value], ctx: &GuardrailContext) -> Verdict {
    let Some(fact_block) = build_fact_block(workflows, ctx) else {
        return Verdict::Allowed;
    };

    match clingo::solve(&fact_block, POLICY) {
        Ok(atoms) if atoms.is_empty() => Verdict::Allowed,
        Ok(atoms) => {
            let mut violations: Vec<Violation> = atoms
                .into_iter()
                .map(|a| Violation { kind: a.kind, workflow: a.workflow, node: a.node })
                .collect();
            // Stable, de-duplicated output.
            violations.sort_by(|a, b| (&a.kind, a.workflow, &a.node).cmp(&(&b.kind, b.workflow, &b.node)));
            violations.dedup();
            Verdict::NeedsReview(violations)
        }
        Err(clingo::ClingoError::Unavailable(e)) => {
            tracing::warn!("[guardrails] clingo unavailable ({e}) — verdict skipped");
            Verdict::Skipped(format!("clingo unavailable: {e}"))
        }
        Err(clingo::ClingoError::Failed(e)) => {
            tracing::error!("[guardrails] clingo failed: {e}");
            Verdict::Skipped(format!("clingo failed: {e}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn flood_workflow() -> Value {
        json!({
            "nodes": [
                { "name": "Every minute", "type": "n8n-nodes-base.scheduleTrigger",
                  "parameters": { "rule": { "interval": [ { "field": "minutes", "minutesInterval": 1 } ] } } },
                { "name": "Send", "type": "n8n-nodes-base.emailSend", "parameters": {} }
            ],
            "connections": { "Every minute": { "main": [[{ "node": "Send", "type": "main", "index": 0 }]] } }
        })
    }

    #[test]
    fn verdict_auto_deploy_semantics() {
        assert!(Verdict::Allowed.allows_auto_deploy(false));
        assert!(!Verdict::NeedsReview(vec![]).allows_auto_deploy(false));
        assert!(Verdict::Skipped("x".into()).allows_auto_deploy(false)); // fail-open
        assert!(!Verdict::Skipped("x".into()).allows_auto_deploy(true)); // fail-closed
    }

    #[test]
    fn empty_workflows_are_allowed_without_clingo() {
        assert_eq!(evaluate(&[]), Verdict::Allowed);
    }

    #[test]
    fn fact_block_includes_ownership_facts() {
        // The clingo input must carry the owns_domain facts alongside workflow facts,
        // so the ASP ownership rules can see them. Verified without the binary.
        let ctx = GuardrailContext { owned_domains: vec!["acme.com".into()] };
        let block = build_fact_block(&[flood_workflow()], &ctx).expect("non-empty");
        assert!(block.contains("owns_domain(\"acme.com\")."), "block was:\n{block}");
        assert!(block.contains("trigger_interval(0, \"Every minute\", 60)."));
    }

    #[test]
    fn fact_block_is_none_when_nothing_to_check() {
        // No nodes → no facts → None → caller short-circuits to Allowed.
        let empty = json!({ "nodes": [], "connections": {} });
        assert!(build_fact_block(&[empty], &GuardrailContext::default()).is_none());
    }

    #[test]
    fn fact_block_without_ownership_has_no_owns_domain() {
        let block = build_fact_block(&[flood_workflow()], &GuardrailContext::default())
            .expect("non-empty");
        assert!(!block.contains("owns_domain("), "block was:\n{block}");
    }

    // End-to-end against the real clingo binary. #[ignore]d because clingo may not
    // be installed in CI; run with:  cargo test -p backend -- --ignored guardrails
    #[test]
    #[ignore = "needs the clingo binary on PATH (or CLINGO_PATH)"]
    fn flood_workflow_needs_review_with_clingo() {
        let verdict = evaluate(&[flood_workflow()]);
        match verdict {
            Verdict::NeedsReview(vs) => {
                assert!(vs.iter().any(|v| v.kind == "flood"), "expected a flood violation, got {vs:?}");
            }
            other => panic!("expected NeedsReview(flood), got {other:?}"),
        }
    }

    #[test]
    #[ignore = "needs the clingo binary on PATH (or CLINGO_PATH)"]
    fn benign_workflow_is_allowed_with_clingo() {
        // A daily digest: schedule(24h) → email. Not high-frequency → no violation.
        let wf = json!({
            "nodes": [
                { "name": "Daily", "type": "n8n-nodes-base.scheduleTrigger",
                  "parameters": { "rule": { "interval": [ { "field": "days", "daysInterval": 1 } ] } } },
                { "name": "Digest", "type": "n8n-nodes-base.emailSend", "parameters": {} }
            ],
            "connections": { "Daily": { "main": [[{ "node": "Digest", "type": "main", "index": 0 }]] } }
        });
        assert_eq!(evaluate(&[wf]), Verdict::Allowed);
    }

    // ── v2 ownership rules ───────────────────────────────────────────────────────

    /// A loop feeding an HTTP POST to an external domain.
    fn bulk_post_workflow(target_url: &str) -> Value {
        json!({
            "nodes": [
                { "name": "Batch", "type": "n8n-nodes-base.splitInBatches",
                  "parameters": { "batchSize": 50 } },
                { "name": "Post", "type": "n8n-nodes-base.httpRequest",
                  "parameters": { "url": target_url, "method": "POST" } }
            ],
            "connections": { "Batch": { "main": [[{ "node": "Post", "type": "main", "index": 0 }]] } }
        })
    }

    #[test]
    #[ignore = "needs the clingo binary on PATH (or CLINGO_PATH)"]
    fn bulk_post_to_third_party_is_flagged_unowned() {
        // No ownership context → competitor.com is third-party → unowned_bulk_post.
        let verdict = evaluate(&[bulk_post_workflow("https://competitor.com/api")]);
        match verdict {
            Verdict::NeedsReview(vs) => {
                assert!(vs.iter().any(|v| v.kind == "unowned_bulk_post"),
                    "expected unowned_bulk_post, got {vs:?}");
            }
            other => panic!("expected NeedsReview, got {other:?}"),
        }
    }

    #[test]
    #[ignore = "needs the clingo binary on PATH (or CLINGO_PATH)"]
    fn bulk_post_to_own_domain_is_allowed() {
        // Same workflow, but acme.com is proven-owned → not third-party → no
        // unowned_bulk_post. (mass_post is ownership-agnostic and would still fire;
        // this asserts the ownership rule specifically does NOT add a violation.)
        let ctx = GuardrailContext { owned_domains: vec!["acme.com".into()] };
        let verdict = evaluate_with_context(&[bulk_post_workflow("https://acme.com/leads")], &ctx);
        let unowned = match &verdict {
            Verdict::NeedsReview(vs) => vs.iter().any(|v| v.kind == "unowned_bulk_post"),
            _ => false,
        };
        assert!(!unowned, "owned-domain POST must NOT raise unowned_bulk_post; got {verdict:?}");
    }

    #[test]
    #[ignore = "needs the clingo binary on PATH (or CLINGO_PATH)"]
    fn high_freq_hitting_third_party_is_unowned_flood() {
        let wf = json!({
            "nodes": [
                { "name": "Every 30s", "type": "n8n-nodes-base.scheduleTrigger",
                  "parameters": { "rule": { "interval": [ { "field": "seconds", "secondsInterval": 30 } ] } } },
                { "name": "Ping", "type": "n8n-nodes-base.httpRequest",
                  "parameters": { "url": "https://victim.example/health", "method": "GET" } }
            ],
            "connections": { "Every 30s": { "main": [[{ "node": "Ping", "type": "main", "index": 0 }]] } }
        });
        match evaluate(&[wf]) {
            Verdict::NeedsReview(vs) => {
                assert!(vs.iter().any(|v| v.kind == "unowned_flood"),
                    "expected unowned_flood, got {vs:?}");
            }
            other => panic!("expected NeedsReview, got {other:?}"),
        }
    }

    #[test]
    #[ignore = "needs the clingo binary on PATH (or CLINGO_PATH)"]
    fn dynamic_url_is_not_treated_as_unowned() {
        // A dynamic ({{…}}) URL resolves to host "unknown"; ownership rules must NOT
        // fire on it (we never block on a domain we couldn't read).
        let ctx = GuardrailContext::default();
        let verdict = evaluate_with_context(&[bulk_post_workflow("={{ $json.endpoint }}")], &ctx);
        let unowned = match &verdict {
            Verdict::NeedsReview(vs) => vs.iter().any(|v| v.kind.starts_with("unowned_")),
            _ => false,
        };
        assert!(!unowned, "dynamic URL must not raise an unowned_* violation; got {verdict:?}");
    }
}
