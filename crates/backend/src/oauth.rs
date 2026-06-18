//! OAuth2 consent orchestration — delegated to n8n.
//!
//! n8n owns the full OAuth2 dance (code→token exchange, refresh, encrypted token
//! storage). We never see or store the client's access/refresh tokens. Our only job
//! is to: (1) create the credential *shell* (clientId/clientSecret/scopes) via the
//! public REST API — done by [`crate::credentials`] — then (2) hand the client the
//! provider's consent URL so they click "authorize" once.
//!
//! The wrinkle that shapes this module: the endpoint that *mints* that consent URL,
//! `GET /rest/oauth2-credential/auth?id=<credId>`, lives under n8n's `/rest/` UI API,
//! which is gated by a **login session cookie** — NOT the public `/api/v1` API key.
//! (Probed against prod: `/api/v1/*` → 401 without key; `/rest/oauth2-credential/auth`
//! → 401 without a UI session.) So to drive consent programmatically we log in once
//! with the owner n8n account, reuse the session cookie to fetch the consent URL, and
//! redirect the client there. n8n's own `/rest/oauth2-credential/callback` finishes the
//! exchange — we write no callback handler and hold no tokens.
//!
//! Trade-off accepted by the owner (2026-06-18): this needs `N8N_OWNER_EMAIL` /
//! `N8N_OWNER_PASSWORD` in the env; it breaks if 2FA/SSO is enabled on n8n. Documented
//! as a secret to protect.

use reqwest::cookie::CookieStore;
use std::sync::Arc;

/// Owner-login config for n8n's `/rest` UI API. Separate from [`crate::credentials::N8nRestConfig`]
/// (which uses the `/api/v1` API key) because consent-URL minting is session-gated.
#[derive(Clone)]
pub struct N8nOwnerLogin {
    base_url: String,
    email: String,
    password: String,
}

impl N8nOwnerLogin {
    /// Reads `N8N_URL` (shared with the rest of the backend) plus the owner
    /// credentials `N8N_OWNER_EMAIL` / `N8N_OWNER_PASSWORD`. `None` if any is unset —
    /// the caller then degrades to the guided-handoff path (OauthRequired).
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("N8N_URL").ok().filter(|s| !s.is_empty())?;
        let email = std::env::var("N8N_OWNER_EMAIL").ok().filter(|s| !s.is_empty())?;
        let password = std::env::var("N8N_OWNER_PASSWORD").ok().filter(|s| !s.is_empty())?;
        Some(Self { base_url: base_url.trim_end_matches('/').to_string(), email, password })
    }

    pub fn new(base_url: impl Into<String>, email: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            email: email.into(),
            password: password.into(),
        }
    }

    /// Log in to n8n's UI API and return the consent URL for an already-created
    /// credential shell. One short-lived cookie jar per call: we don't cache the
    /// session (logins are rare — once per credential connect), which keeps this
    /// stateless and avoids holding the owner cookie in memory between requests.
    ///
    /// `http` is the shared client; we layer a per-call cookie store on top via a
    /// dedicated client so the global client stays cookie-free.
    pub async fn consent_url(&self, cred_id: &str) -> Result<String, String> {
        let jar = Arc::new(reqwest::cookie::Jar::default());
        let client = reqwest::Client::builder()
            .cookie_provider(jar.clone())
            .build()
            .map_err(|e| format!("oauth client build: {e}"))?;

        // 1. Owner login → session cookie lands in the jar.
        let login = client
            .post(format!("{}/rest/login", self.base_url))
            .json(&serde_json::json!({ "emailOrLdapLoginId": self.email, "password": self.password }))
            .send()
            .await
            .map_err(|e| format!("n8n login request: {e}"))?;
        if !login.status().is_success() {
            return Err(format!(
                "n8n login → HTTP {}: {}",
                login.status(),
                login.text().await.unwrap_or_default().chars().take(200).collect::<String>()
            ));
        }
        // Guard against an n8n that 200s the login but sets no cookie (misconfig).
        let url = self.base_url.parse::<reqwest::Url>().map_err(|e| format!("base url: {e}"))?;
        if jar.cookies(&url).is_none() {
            return Err("n8n login succeeded but set no session cookie — 2FA/SSO enabled?".into());
        }

        // 2. Ask n8n for the provider consent URL for this credential shell.
        let resp = client
            .get(format!("{}/rest/oauth2-credential/auth", self.base_url))
            .query(&[("id", cred_id)])
            .send()
            .await
            .map_err(|e| format!("consent-url request: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!(
                "consent-url id={cred_id} → HTTP {}: {}",
                resp.status(),
                resp.text().await.unwrap_or_default().chars().take(200).collect::<String>()
            ));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| format!("consent-url parse: {e}"))?;
        extract_consent_url(&v)
            .ok_or_else(|| format!("consent-url: no auth url in response: {v}"))
    }
}

/// The owner's registered OAuth app keys for a provider, resolved from the env.
/// These are *our* secrets (the OAuth app pointe.dev registered with the provider) —
/// never the client's — so they live server-side and never reach the browser.
pub struct OauthAppKeys {
    pub client_id: String,
    pub client_secret: String,
}

/// Map an n8n credential type to its owner OAuth-app env vars and read them.
///
/// Convention: `<PROVIDER>_OAUTH_CLIENT_ID` / `_CLIENT_SECRET`. Providers that share
/// one Google Cloud OAuth app (Gmail, Drive, Calendar, Sheets, YouTube) all read the
/// `GOOGLE_*` pair, so the owner registers a single Google app. Returns `None` when the
/// keys aren't configured — the caller degrades to the guided-handoff path.
pub fn app_keys_for(cred_type: &str) -> Option<OauthAppKeys> {
    let prefix = match cred_type {
        "gmailOAuth2"
        | "googleDriveOAuth2Api"
        | "googleCalendarOAuth2Api"
        | "googleSheetsOAuth2Api"
        | "youTubeOAuth2Api" => "GOOGLE",
        "microsoftOutlookOAuth2Api" => "MICROSOFT",
        "slackOAuth2Api" => "SLACK",
        "hubspotOAuth2Api" => "HUBSPOT",
        "salesforceOAuth2Api" => "SALESFORCE",
        "zohoOAuth2Api" => "ZOHO",
        "mailchimpOAuth2Api" => "MAILCHIMP",
        "asanaOAuth2Api" => "ASANA",
        "shopifyOAuth2Api" => "SHOPIFY",
        "twitterOAuth2Api" => "TWITTER",
        "linkedInOAuth2Api" => "LINKEDIN",
        _ => return None,
    };
    let client_id = std::env::var(format!("{prefix}_OAUTH_CLIENT_ID")).ok().filter(|s| !s.is_empty())?;
    let client_secret = std::env::var(format!("{prefix}_OAUTH_CLIENT_SECRET")).ok().filter(|s| !s.is_empty())?;
    Some(OauthAppKeys { client_id, client_secret })
}

/// n8n returns the consent URL as `{"data": "https://accounts.google.com/..."}`
/// on the UI API. Tolerate a bare string or a `{data:{authUri}}` shape too, since the
/// envelope has shifted across n8n versions.
fn extract_consent_url(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    let data = v.get("data")?;
    if let Some(s) = data.as_str() {
        return Some(s.to_string());
    }
    data.get("authUri")
        .or_else(|| data.get("url"))
        .and_then(|x| x.as_str())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_consent_url_from_data_string() {
        let v = json!({ "data": "https://accounts.google.com/o/oauth2/v2/auth?x=1" });
        assert_eq!(
            extract_consent_url(&v).unwrap(),
            "https://accounts.google.com/o/oauth2/v2/auth?x=1"
        );
    }

    #[test]
    fn extracts_consent_url_from_bare_string() {
        let v = json!("https://login.salesforce.com/services/oauth2/authorize?y=2");
        assert!(extract_consent_url(&v).unwrap().contains("salesforce.com"));
    }

    #[test]
    fn extracts_consent_url_from_nested_authuri() {
        let v = json!({ "data": { "authUri": "https://app.hubspot.com/oauth/authorize?z=3" } });
        assert!(extract_consent_url(&v).unwrap().contains("hubspot.com"));
    }

    #[test]
    fn returns_none_when_no_url_present() {
        assert!(extract_consent_url(&json!({ "data": { "foo": "bar" } })).is_none());
        assert!(extract_consent_url(&json!({ "error": "nope" })).is_none());
    }

    #[test]
    fn from_env_requires_all_three_vars() {
        // Missing owner vars → None (degrade to guided handoff). We don't set/unset
        // process env here (would race other tests); just assert the constructor wires
        // the fields it's given.
        let l = N8nOwnerLogin::new("https://n8n.pointe.dev/", "o@x.io", "pw");
        assert_eq!(l.base_url, "https://n8n.pointe.dev"); // trailing slash trimmed
        assert_eq!(l.email, "o@x.io");
    }
}
