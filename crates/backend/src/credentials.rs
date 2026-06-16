//! In-app credential provisioning (offer tier "Assisté"). Creates a client's
//! service credential in n8n via its REST API so `create_from_code` auto-wires it
//! at deploy (see [`crate::mcp`] + the n8n-mcp-credentials memory).
//!
//! Why REST and not the n8n UI: the UI refuses to SAVE a service credential whose
//! live connection test fails — so a client can't pre-create one with a key we
//! haven't validated. The public REST `POST /api/v1/credentials` does NOT run that
//! test, so it's the right path for guided self-serve provisioning.
//!
//! v1 = API-key credentials only. OAuth2 needs a per-provider OAuth app + consent
//! redirect (deferred); [`crate::capabilities::Auth`] tells the caller which is which.

use serde_json::{json, Map, Value};

/// n8n public REST API config. Reads the SAME `N8N_URL` + `N8N_API_KEY` the rest of
/// the backend uses (in prod these are the live server values; the local `.env` key
/// is stale — inject the server one for local tests).
#[derive(Clone)]
pub struct N8nRestConfig {
    base_url: String,
    api_key: String,
}

impl N8nRestConfig {
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("N8N_URL").ok().filter(|s| !s.is_empty())?;
        let api_key = std::env::var("N8N_API_KEY").ok().filter(|s| !s.is_empty())?;
        Some(Self { base_url: base_url.trim_end_matches('/').to_string(), api_key })
    }

    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self { base_url: base_url.into().trim_end_matches('/').to_string(), api_key: api_key.into() }
    }

    fn req(&self, http: &reqwest::Client, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        http.request(method, format!("{}/api/v1{path}", self.base_url))
            .header("X-N8N-API-KEY", &self.api_key)
            .header("Content-Type", "application/json")
    }

    /// Fetch the data JSON-schema for a credential type (its fields + conditional
    /// required rules). Used to build a payload n8n will accept.
    pub async fn credential_schema(&self, http: &reqwest::Client, cred_type: &str) -> Result<Value, String> {
        let resp = self.req(http, reqwest::Method::GET, &format!("/credentials/schema/{cred_type}"))
            .send().await.map_err(|e| format!("schema request: {e}"))?;
        if !resp.status().is_success() {
            let s = resp.status();
            return Err(format!("schema {cred_type} → HTTP {s}: {}",
                resp.text().await.unwrap_or_default().chars().take(200).collect::<String>()));
        }
        resp.json().await.map_err(|e| format!("schema parse: {e}"))
    }

    /// Create a credential of `cred_type` named `name`, filling the client-`provided`
    /// secret fields and safe defaults for n8n's conditional plumbing fields.
    /// Returns the new credential id. Idempotency is the caller's concern (n8n allows
    /// duplicates; auto-assign then picks one — see the credentials memory).
    pub async fn create_credential(
        &self, http: &reqwest::Client, cred_type: &str, name: &str, provided: &Map<String, Value>,
    ) -> Result<String, String> {
        let schema = self.credential_schema(http, cred_type).await?;
        let data = build_credential_data(&schema, provided);
        let body = json!({ "name": name, "type": cred_type, "data": data });
        let resp = self.req(http, reqwest::Method::POST, "/credentials")
            .json(&body).send().await.map_err(|e| format!("create request: {e}"))?;
        if !resp.status().is_success() {
            let s = resp.status();
            return Err(format!("create {cred_type} → HTTP {s}: {}",
                resp.text().await.unwrap_or_default().chars().take(300).collect::<String>()));
        }
        let v: Value = resp.json().await.map_err(|e| format!("create parse: {e}"))?;
        v["id"].as_str().map(str::to_string)
            .ok_or_else(|| format!("create: no id in response: {v}"))
    }

    pub async fn delete_credential(&self, http: &reqwest::Client, id: &str) -> Result<(), String> {
        let resp = self.req(http, reqwest::Method::DELETE, &format!("/credentials/{id}"))
            .send().await.map_err(|e| format!("delete request: {e}"))?;
        if resp.status().is_success() { Ok(()) }
        else { Err(format!("delete {id} → HTTP {}", resp.status())) }
    }
}

/// Build the `data` object for `POST /credentials`: keep only `provided` fields that
/// exist in the schema (n8n sets `additionalProperties:false`), then add safe defaults
/// for the conditional plumbing fields n8n credentials carry.
///
/// The one quirk that bites every service: a credential's `allOf` makes `allowedDomains`
/// REQUIRED unless `allowedHttpRequestDomains` is set to a non-`domains` value — and an
/// ABSENT `allowedHttpRequestDomains` matches the `if` vacuously, triggering the
/// requirement. So whenever the schema declares it, we pin it to `"all"`.
fn build_credential_data(schema: &Value, provided: &Map<String, Value>) -> Value {
    let props = schema.get("properties").and_then(Value::as_object);
    let mut data = Map::new();
    if let Some(props) = props {
        for (k, v) in provided {
            if props.contains_key(k) {
                data.insert(k.clone(), v.clone());
            }
        }
        if props.contains_key("allowedHttpRequestDomains") {
            data.insert("allowedHttpRequestDomains".to_string(), json!("all"));
        }
    }
    Value::Object(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notion_schema() -> Value {
        json!({
            "additionalProperties": false, "type": "object",
            "properties": {
                "apiKey": {"type": "string"},
                "allowedHttpRequestDomains": {"type": "string", "enum": ["all","domains","none"]},
                "allowedDomains": {"type": "string"}
            },
            "required": []
        })
    }

    #[test]
    fn keeps_provided_field_and_pins_allowed_domains_all() {
        let mut provided = Map::new();
        provided.insert("apiKey".into(), json!("secret_test"));
        let data = build_credential_data(&notion_schema(), &provided);
        assert_eq!(data["apiKey"], json!("secret_test"));
        // the quirk: must be pinned to "all" to avoid the allowedDomains requirement
        assert_eq!(data["allowedHttpRequestDomains"], json!("all"));
        assert!(data.get("allowedDomains").is_none());
    }

    #[test]
    fn drops_fields_absent_from_schema() {
        let mut provided = Map::new();
        provided.insert("apiKey".into(), json!("k"));
        provided.insert("bogus".into(), json!("x")); // additionalProperties:false → must be dropped
        let data = build_credential_data(&notion_schema(), &provided);
        assert!(data.get("bogus").is_none());
        assert!(data.get("apiKey").is_some());
    }

    #[test]
    fn no_allowed_domains_key_when_schema_lacks_it() {
        let schema = json!({"properties": {"accessToken": {"type": "string"}}, "required": []});
        let mut provided = Map::new();
        provided.insert("accessToken".into(), json!("t"));
        let data = build_credential_data(&schema, &provided);
        assert!(data.get("allowedHttpRequestDomains").is_none());
    }
}
