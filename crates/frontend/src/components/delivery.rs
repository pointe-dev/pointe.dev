//! Shared delivery-checklist UI (c.1): one row per integration the client must
//! connect. Reused by the post-payment page (merci) and the client dashboard (espace).

use leptos::*;
use gloo_net::http::Request;
use serde::Deserialize;

/// Backend response for `POST /api/oauth/start`. We only act on the `consent` shape
/// (redirect the browser to the provider); other shapes surface a guided note.
#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum OauthStart {
    Consent { consent_url: String },
    NotOauth { message: String },
    NotWired { message: String },
}

/// One integration in a delivery checklist, as returned by the backend
/// (`/api/pipeline/:id/delivery` and `/api/client/workflows`).
#[derive(Deserialize, Clone)]
pub struct DeliveryItem {
    pub service: String,
    pub tier: String, // "native" | "http" | "managed"
    pub auth: String, // "none" | "api_key" | "oauth2"
    pub provisionable: bool,
    #[serde(default)]
    pub prerequisite: Option<String>,
}

/// Stable session id (set during the chat funnel), needed to authenticate
/// credential provisioning and the client dashboard. Empty string if absent.
pub fn session_id() -> String {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("_sid").ok().flatten())
        .unwrap_or_default()
}

/// One row of the delivery checklist. API-key services get an inline field +
/// "Connecter" button (calls /api/credentials/provision); OAuth is a guided note;
/// no-credential services show as ready; bespoke ones are flagged as team-handled.
pub fn delivery_row(item: DeliveryItem) -> impl IntoView {
    let status = create_rw_signal::<&'static str>("idle"); // idle | saving | ok | err
    let secret = create_rw_signal(String::new());
    let service = item.service.clone();

    let on_connect = {
        let service = service.clone();
        move |_| {
            let key = secret.get();
            if key.trim().is_empty() { return; }
            let service = service.clone();
            status.set("saving");
            spawn_local(async move {
                // Send the value under the common secret field names; the backend
                // keeps only the one in the credential's schema (drops the rest).
                let body = serde_json::json!({
                    "session_id": session_id(),
                    "service": service,
                    "secrets": { "apiKey": key, "accessToken": key, "token": key }
                });
                let ok = match Request::post("/api/credentials/provision").json(&body) {
                    Ok(req) => req.send().await.map(|r| r.status() == 200).unwrap_or(false),
                    Err(_) => false,
                };
                status.set(if ok { "ok" } else { "err" });
            });
        }
    };

    // OAuth consent: ask the backend for the provider consent URL, then send the
    // browser there. n8n's callback finishes the token exchange; nothing to store here.
    let oauth_err = create_rw_signal(String::new());
    let on_authorize = {
        let service = service.clone();
        move |_| {
            let service = service.clone();
            oauth_err.set(String::new());
            status.set("saving");
            spawn_local(async move {
                let body = serde_json::json!({ "session_id": session_id(), "service": service });
                let parsed = match Request::post("/api/oauth/start").json(&body) {
                    Ok(req) => match req.send().await {
                        Ok(r) if r.status() == 200 => r.json::<OauthStart>().await.ok(),
                        _ => None,
                    },
                    Err(_) => None,
                };
                match parsed {
                    Some(OauthStart::Consent { consent_url }) => {
                        // Full-page navigation to the provider's consent screen.
                        if let Some(w) = web_sys::window() {
                            let _ = w.location().set_href(&consent_url);
                        }
                    }
                    Some(OauthStart::NotOauth { message }) | Some(OauthStart::NotWired { message }) => {
                        status.set("err");
                        oauth_err.set(message);
                    }
                    None => {
                        status.set("err");
                        oauth_err.set("Connexion impossible — notre équipe vous guide.".into());
                    }
                }
            });
        }
    };

    let label = item.service.clone();
    let prereq = item.prerequisite.clone();

    view! {
        <div class="border-t border-subtle pt-3 first:border-t-0 first:pt-0">
            <div class="flex items-center justify-between gap-2">
                <span class="text-sm font-medium text-primary">{label}</span>
                {move || match status.get() {
                    "ok" => view! { <span class="text-xs font-medium text-green-400">"✓ connecté"</span> }.into_view(),
                    "saving" => view! { <span class="text-xs text-amber-400">"…"</span> }.into_view(),
                    "err" => view! { <span class="text-xs text-red-400">"échec — vérifiez la clé"</span> }.into_view(),
                    _ => ().into_view(),
                }}
            </div>

            {
                let tier = item.tier.clone();
                let auth = item.auth.clone();
                if item.provisionable {
                    view! {
                        <div class="mt-2 flex gap-2">
                            <input
                                type="password"
                                placeholder="Clé API / token"
                                class="flex-1 text-xs px-3 py-2 rounded-lg bg-deep border border-subtle text-primary placeholder:text-muted"
                                prop:value=move || secret.get()
                                on:input=move |e| secret.set(event_target_value(&e))
                            />
                            <button
                                on:click=on_connect
                                disabled=move || status.get() == "saving" || status.get() == "ok"
                                class="text-xs font-medium px-3 py-2 rounded-lg glass-cyan text-cyan hover:text-cyan-mid transition-colors disabled:opacity-50"
                            >"Connecter"</button>
                        </div>
                    }.into_view()
                } else if auth == "oauth2" {
                    view! {
                        <div class="mt-2">
                            <button
                                on:click=on_authorize
                                disabled=move || status.get() == "saving" || status.get() == "ok"
                                class="text-xs font-medium px-3 py-2 rounded-lg glass-cyan text-cyan hover:text-cyan-mid transition-colors disabled:opacity-50"
                            >"Autoriser la connexion"</button>
                            {move || {
                                let e = oauth_err.get();
                                (!e.is_empty()).then(|| view! {
                                    <p class="mt-1 text-xs text-amber-400/90">{e}</p>
                                })
                            }}
                        </div>
                    }.into_view()
                } else if tier == "managed" {
                    view! {
                        <p class="mt-1 text-xs text-muted">
                            "Intégration sur mesure — réalisée par notre équipe."
                        </p>
                    }.into_view()
                } else {
                    view! {
                        <p class="mt-1 text-xs text-green-400/80">"Aucun identifiant requis."</p>
                    }.into_view()
                }
            }
            {prereq.map(|p| view! { <p class="mt-1 text-[11px] text-muted italic">{p}</p> })}
        </div>
    }
}
