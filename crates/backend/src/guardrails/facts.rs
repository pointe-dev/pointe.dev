//! n8n workflow JSON → ASP facts.
//!
//! Pure, deterministic, network-free. Each emitted line is a clingo fact consumed
//! by `policy.lp`. Unknown / unparseable shapes are skipped (fail-open per node):
//! we never fabricate a fact, so a node we can't classify simply yields no rule.

use serde_json::Value;

/// Escape a string for use as a clingo quoted constant.
fn q(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Extracts the host from a URL string. Returns "unknown" for dynamic n8n
/// expressions (`={{…}}`) or anything we can't parse to a bare host.
fn host_of(url: &str) -> String {
    let u = url.trim();
    if u.is_empty() || u.contains("{{") || u.starts_with('=') {
        return "unknown".to_string();
    }
    // Strip scheme.
    let after_scheme = u.split("://").nth(1).unwrap_or(u);
    // Host ends at the first '/', '?', ':' or end.
    let host: String = after_scheme
        .chars()
        .take_while(|&c| c != '/' && c != '?' && c != ':' && c != '#')
        .collect();
    if host.is_empty() || host.contains("{{") {
        "unknown".to_string()
    } else {
        host.to_lowercase()
    }
}

/// Best-effort: a time trigger's period in seconds, if the node is a schedule /
/// interval / cron trigger we can read. Returns None when not a time trigger or
/// the period can't be determined.
fn trigger_seconds(node_type: &str, params: &Value) -> Option<u64> {
    let unit_secs = |unit: &str| -> Option<u64> {
        match unit {
            "seconds" | "second" => Some(1),
            "minutes" | "minute" => Some(60),
            "hours" | "hour" => Some(3600),
            "days" | "day" => Some(86400),
            "weeks" | "week" => Some(604800),
            _ => None,
        }
    };

    match node_type {
        // n8n-nodes-base.interval: { unit, value }
        t if t.ends_with(".interval") => {
            let unit = params.get("unit").and_then(|v| v.as_str()).unwrap_or("seconds");
            let value = params.get("value").and_then(|v| v.as_u64()).unwrap_or(1);
            unit_secs(unit).map(|s| s.saturating_mul(value.max(1)))
        }
        // n8n-nodes-base.scheduleTrigger: { rule: { interval: [ { field, <unit>Interval } ] } }
        t if t.ends_with(".scheduleTrigger") => {
            let interval = params.get("rule")?.get("interval")?.as_array()?;
            // Take the tightest (smallest) period across rules — the worst case.
            let mut best: Option<u64> = None;
            for it in interval {
                let field = it.get("field").and_then(|v| v.as_str()).unwrap_or("seconds");
                let n = it
                    .get(&format!("{field}Interval"))
                    .or_else(|| it.get("value"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1)
                    .max(1);
                if let Some(s) = unit_secs(field).map(|u| u.saturating_mul(n)) {
                    best = Some(best.map_or(s, |b| b.min(s)));
                }
            }
            best
        }
        // n8n-nodes-base.cron — we can't cheaply read arbitrary cron; treat a cron
        // trigger as potentially frequent (conservative: 60s) so it's covered.
        t if t.ends_with(".cron") => Some(60),
        _ => None,
    }
}

/// Returns true for node types that send email.
fn is_email_out(node_type: &str) -> bool {
    let t = node_type.to_lowercase();
    t.ends_with(".emailsend")
        || t.ends_with(".gmail")
        || t.ends_with(".microsoftoutlook")
        || t.ends_with(".sendemail")
        || t.contains("emailsend")
        || (t.contains("smtp"))
}

/// Returns true for batching / loop nodes.
fn is_loop(node_type: &str) -> bool {
    let t = node_type.to_lowercase();
    t.ends_with(".splitinbatches") || t.ends_with(".loop") || t.contains("splitinbatches")
}

/// Build the full fact base for a set of workflows (one ASP fact per line).
pub fn workflow_facts(workflows: &[Value]) -> Vec<String> {
    let mut facts = Vec::new();
    for (w, wf) in workflows.iter().enumerate() {
        let nodes = wf.get("nodes").and_then(|n| n.as_array());
        let Some(nodes) = nodes else { continue };

        for node in nodes {
            let name = node.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let ntype = node.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if name.is_empty() || ntype.is_empty() {
                continue;
            }
            let params = node.get("parameters").cloned().unwrap_or(Value::Null);

            facts.push(format!("node({w}, {}, {}).", q(name), q(ntype)));

            if let Some(secs) = trigger_seconds(ntype, &params) {
                facts.push(format!("trigger_interval({w}, {}, {secs}).", q(name)));
            }

            if ntype.ends_with(".httpRequest") {
                let url = params.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let method = params
                    .get("method")
                    .and_then(|v| v.as_str())
                    .unwrap_or("GET")
                    .to_uppercase();
                facts.push(format!(
                    "http_out({w}, {}, {}, {}).",
                    q(name),
                    q(&host_of(url)),
                    q(&method)
                ));
            }

            if is_email_out(ntype) {
                facts.push(format!("email_out({w}, {}).", q(name)));
            }

            if is_loop(ntype) {
                let batch = params.get("batchSize").and_then(|v| v.as_u64()).unwrap_or(0);
                facts.push(format!("loop_node({w}, {}, {batch}).", q(name)));
            }
        }

        // Edges from n8n connections:
        //   connections[FromName].main[outputIdx][] = { node: ToName, ... }
        if let Some(conns) = wf.get("connections").and_then(|c| c.as_object()) {
            for (from, outputs) in conns {
                let Some(main) = outputs.get("main").and_then(|m| m.as_array()) else { continue };
                for branch in main {
                    let Some(targets) = branch.as_array() else { continue };
                    for t in targets {
                        if let Some(to) = t.get("node").and_then(|v| v.as_str()) {
                            facts.push(format!("edge({w}, {}, {}).", q(from), q(to)));
                        }
                    }
                }
            }
        }
    }
    facts
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn host_of_handles_plain_and_dynamic() {
        assert_eq!(host_of("https://api.example.com/v1/x?a=1"), "api.example.com");
        assert_eq!(host_of("http://KKK.com/submit"), "kkk.com");
        assert_eq!(host_of("={{ $json.url }}"), "unknown");
        assert_eq!(host_of("https://{{ $env.HOST }}/x"), "unknown");
        assert_eq!(host_of(""), "unknown");
    }

    #[test]
    fn schedule_trigger_seconds_takes_tightest() {
        let p = json!({ "rule": { "interval": [ { "field": "minutes", "minutesInterval": 1 } ] } });
        assert_eq!(trigger_seconds("n8n-nodes-base.scheduleTrigger", &p), Some(60));
        let p2 = json!({ "rule": { "interval": [ { "field": "seconds", "secondsInterval": 30 } ] } });
        assert_eq!(trigger_seconds("n8n-nodes-base.scheduleTrigger", &p2), Some(30));
    }

    #[test]
    fn interval_node_seconds() {
        let p = json!({ "unit": "minutes", "value": 2 });
        assert_eq!(trigger_seconds("n8n-nodes-base.interval", &p), Some(120));
    }

    #[test]
    fn extracts_flood_shaped_facts() {
        // "send an email every minute" → schedule(60s) → emailSend
        let wf = json!({
            "nodes": [
                { "name": "Every minute", "type": "n8n-nodes-base.scheduleTrigger",
                  "parameters": { "rule": { "interval": [ { "field": "minutes", "minutesInterval": 1 } ] } } },
                { "name": "Send", "type": "n8n-nodes-base.emailSend", "parameters": {} }
            ],
            "connections": {
                "Every minute": { "main": [[{ "node": "Send", "type": "main", "index": 0 }]] }
            }
        });
        let facts = workflow_facts(&[wf]);
        assert!(facts.iter().any(|f| f.contains("trigger_interval(0, \"Every minute\", 60)")));
        assert!(facts.iter().any(|f| f == "email_out(0, \"Send\")."));
        assert!(facts.iter().any(|f| f == "edge(0, \"Every minute\", \"Send\")."));
    }

    #[test]
    fn extracts_http_post_with_host_and_method() {
        let wf = json!({
            "nodes": [
                { "name": "Submit", "type": "n8n-nodes-base.httpRequest",
                  "parameters": { "url": "https://kkk.com/form", "method": "POST" } }
            ],
            "connections": {}
        });
        let facts = workflow_facts(&[wf]);
        assert!(facts.iter().any(|f| f == "http_out(0, \"Submit\", \"kkk.com\", \"POST\")."));
    }
}
