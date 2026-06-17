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
            other => other,
        };
        format!("[{}] node \"{}\" — {what}", self.kind, self.node)
    }
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

/// Evaluate the built workflows against the abuse policy.
///
/// Pure w.r.t. the workflows; the only side effect is spawning clingo. Safe on any
/// input — unparseable workflow shapes simply yield fewer facts.
pub fn evaluate(workflows: &[Value]) -> Verdict {
    let facts = facts::workflow_facts(workflows);
    if facts.is_empty() {
        return Verdict::Allowed;
    }
    let fact_block = facts.join("\n");

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
}
