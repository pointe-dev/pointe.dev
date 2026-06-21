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

/// Normalise a host the same way `host_of` does, so an owned domain compares equal
/// to the host extracted from a workflow URL. A bare domain (`example.com`) or a
/// full URL (`https://example.com/x`) both reduce to `example.com`. `www.` is
/// stripped so `www.example.com` and `example.com` are the same owner.
pub fn normalise_owned(input: &str) -> Option<String> {
    // Give host_of a scheme so its `split("://")` always finds a host part, whether
    // the caller passed a bare domain or a full URL.
    let with_scheme = if input.contains("://") {
        input.to_string()
    } else {
        format!("https://{}", input.trim_start_matches('/'))
    };
    let h = host_of(&with_scheme);
    if h == "unknown" || h.is_empty() {
        return None;
    }
    Some(h.strip_prefix("www.").unwrap_or(&h).to_string())
}

/// Derive the set of domains a client has proven they control, from their verified
/// email address plus any extra **already-proven** hosts. Deterministic, de-duplicated.
/// Free webmail domains are excluded — a gmail.com address does not make gmail.com a
/// domain the client may blast at scale.
///
/// `extra_hosts` must contain ONLY domains whose ownership has been independently
/// proven (e.g. via a future DNS-TXT / .well-known verification flow). It must NOT
/// receive free-text fields the client merely typed (like the n8n URL), or anyone
/// could claim to own an arbitrary target. Today the caller passes `&[]`; the
/// parameter exists so the verification flow can feed proven domains in later.
pub fn owned_domains(email: Option<&str>, extra_hosts: &[String]) -> Vec<String> {
    const WEBMAIL: &[&str] = &[
        "gmail.com", "googlemail.com", "outlook.com", "hotmail.com", "live.com",
        "yahoo.com", "yahoo.fr", "icloud.com", "me.com", "proton.me", "protonmail.com",
        "gmx.com", "gmx.net", "aol.com", "orange.fr", "free.fr", "laposte.net",
    ];
    let mut out: Vec<String> = Vec::new();
    let mut push = |d: String| {
        if !out.contains(&d) {
            out.push(d);
        }
    };
    if let Some(e) = email {
        if let Some(domain) = e.rsplit('@').next().filter(|d| d.contains('.')) {
            if let Some(n) = normalise_owned(domain) {
                if !WEBMAIL.contains(&n.as_str()) {
                    push(n);
                }
            }
        }
    }
    for h in extra_hosts {
        if let Some(n) = normalise_owned(h) {
            push(n);
        }
    }
    out
}

/// Emit `owns_domain("d").` facts for a set of already-normalised owned domains.
pub fn owned_domain_facts(domains: &[String]) -> Vec<String> {
    domains.iter().map(|d| format!("owns_domain({}).", q(d))).collect()
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

    #[test]
    fn normalise_owned_matches_host_of_extraction() {
        assert_eq!(normalise_owned("example.com").as_deref(), Some("example.com"));
        assert_eq!(normalise_owned("https://example.com/x").as_deref(), Some("example.com"));
        assert_eq!(normalise_owned("www.example.com").as_deref(), Some("example.com"));
        assert_eq!(normalise_owned("Example.COM").as_deref(), Some("example.com"));
        assert_eq!(normalise_owned("={{ $json.x }}"), None);
        assert_eq!(normalise_owned(""), None);
    }

    #[test]
    fn owned_domains_from_business_email() {
        let d = owned_domains(Some("alice@acme-corp.com"), &[]);
        assert_eq!(d, vec!["acme-corp.com"]);
    }

    #[test]
    fn owned_domains_excludes_free_webmail() {
        assert!(owned_domains(Some("someone@gmail.com"), &[]).is_empty());
        assert!(owned_domains(Some("someone@outlook.com"), &[]).is_empty());
        assert!(owned_domains(Some("someone@proton.me"), &[]).is_empty());
    }

    #[test]
    fn owned_domains_merges_email_and_extra_hosts_deduped() {
        let d = owned_domains(
            Some("bob@acme.com"),
            &["https://n8n.acme.com".into(), "acme.com".into(), "shop.acme.io".into()],
        );
        // email → acme.com ; n8n host → n8n.acme.com ; "acme.com" is a dup ; shop.acme.io new
        assert_eq!(d, vec!["acme.com", "n8n.acme.com", "shop.acme.io"]);
    }

    #[test]
    fn owned_domains_handles_no_email() {
        assert!(owned_domains(None, &[]).is_empty());
        assert_eq!(owned_domains(None, &["acme.com".into()]), vec!["acme.com"]);
    }

    #[test]
    fn owned_domain_facts_are_quoted() {
        let f = owned_domain_facts(&["acme.com".into(), "n8n.acme.com".into()]);
        assert_eq!(f, vec![
            "owns_domain(\"acme.com\").".to_string(),
            "owns_domain(\"n8n.acme.com\").".to_string(),
        ]);
    }
}
