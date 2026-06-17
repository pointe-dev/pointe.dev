//! clingo sidecar: feed it `facts + policy` on stdin, read the single answer set,
//! return the `violation(Kind, W, Node)` atoms. Network-free, deterministic.
//!
//! clingo is invoked with `--outf=2` (JSON) so the output is robust to parse. The
//! policy derives violations deterministically (no choice rules), so there is
//! exactly one answer set; we read its witness.

use std::io::Write;
use std::process::{Command, Stdio};

#[derive(Debug, PartialEq, Eq)]
pub enum ClingoError {
    /// The clingo binary could not be found / spawned.
    Unavailable(String),
    /// clingo ran but failed (non-zero, unexpected output, etc.).
    Failed(String),
}

/// One parsed `violation(Kind, W, Node)` atom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViolationAtom {
    pub kind: String,
    pub workflow: u32,
    pub node: String,
}

/// Resolve the clingo binary: `CLINGO_PATH` if set, else `clingo` on PATH.
fn clingo_bin() -> String {
    std::env::var("CLINGO_PATH").unwrap_or_else(|_| "clingo".to_string())
}

/// Run `facts ++ policy` through clingo and return the violation atoms.
pub fn solve(facts: &str, policy: &str) -> Result<Vec<ViolationAtom>, ClingoError> {
    let program = format!("{facts}\n{policy}\n");

    let mut child = Command::new(clingo_bin())
        .args(["--outf=2", "--quiet=1", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ClingoError::Unavailable(format!("spawn clingo: {e}")))?;

    child
        .stdin
        .take()
        .ok_or_else(|| ClingoError::Failed("no stdin".into()))?
        .write_all(program.as_bytes())
        .map_err(|e| ClingoError::Failed(format!("write program: {e}")))?;

    let out = child
        .wait_with_output()
        .map_err(|e| ClingoError::Failed(format!("wait clingo: {e}")))?;

    // clingo exit codes: 10 = SAT, 20 = UNSAT, 30 = SAT+OPTIMUM. Anything that
    // produced JSON we can parse is fine; only treat a missing/garbled body as a
    // failure.
    let stdout = String::from_utf8_lossy(&out.stdout);
    parse_clingo_json(&stdout)
}

/// Parse clingo `--outf=2` JSON, extracting violation atoms from the first witness.
pub fn parse_clingo_json(stdout: &str) -> Result<Vec<ViolationAtom>, ClingoError> {
    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .map_err(|e| ClingoError::Failed(format!("clingo JSON parse: {e}; body={stdout:.200}")))?;

    // Path: Call[].Witnesses[].Value = ["violation(flood,0,\"Send\")", ...]
    let mut atoms = Vec::new();
    if let Some(calls) = v.get("Call").and_then(|c| c.as_array()) {
        for call in calls {
            if let Some(ws) = call.get("Witnesses").and_then(|w| w.as_array()) {
                for w in ws {
                    if let Some(vals) = w.get("Value").and_then(|x| x.as_array()) {
                        for val in vals {
                            if let Some(s) = val.as_str() {
                                if let Some(a) = parse_atom(s) {
                                    atoms.push(a);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(atoms)
}

/// Parse `violation(kind, w, "node name")` into a ViolationAtom.
fn parse_atom(s: &str) -> Option<ViolationAtom> {
    let inner = s.strip_prefix("violation(")?.strip_suffix(')')?;
    // Split on top-level commas (the node string may itself contain commas, so we
    // split into at most 3 fields, last keeps the rest).
    let kind_rest = inner.split_once(',')?;
    let kind = kind_rest.0.trim().to_string();
    let wf_rest = kind_rest.1.split_once(',')?;
    let workflow: u32 = wf_rest.0.trim().parse().ok()?;
    let node = wf_rest.1.trim().trim_matches('"').to_string();
    Some(ViolationAtom { kind, workflow, node })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_violation_atoms_from_json() {
        let json = r#"{
          "Call": [ { "Witnesses": [ { "Value": [
            "violation(flood,0,\"Send\")",
            "violation(mass_post,1,\"Submit form\")"
          ] } ] } ]
        }"#;
        let atoms = parse_clingo_json(json).unwrap();
        assert_eq!(atoms.len(), 2);
        assert_eq!(atoms[0], ViolationAtom { kind: "flood".into(), workflow: 0, node: "Send".into() });
        assert_eq!(atoms[1].kind, "mass_post");
        assert_eq!(atoms[1].node, "Submit form");
    }

    #[test]
    fn empty_witness_means_no_violations() {
        let json = r#"{ "Call": [ { "Witnesses": [ { "Value": [] } ] } ] }"#;
        assert_eq!(parse_clingo_json(json).unwrap().len(), 0);
    }

    #[test]
    fn garbage_output_is_a_failure_not_a_panic() {
        assert!(matches!(parse_clingo_json("not json"), Err(ClingoError::Failed(_))));
    }
}
