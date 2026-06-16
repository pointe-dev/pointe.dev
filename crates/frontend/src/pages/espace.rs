//! Client dashboard (espace client) — the authenticated home for a paid client.
//! Lists every workflow they own (resolved server-side by the email behind their
//! unlocked session) with status, price, the live link, and the c.1 delivery
//! checklist so they can finish connecting their services any time.

use leptos::*;
use gloo_net::http::Request;
use serde::Deserialize;

use crate::components::delivery::{delivery_row, session_id, DeliveryItem};

#[derive(Deserialize, Clone)]
struct ClientWorkflow {
    pipeline_id: String,
    stage: String,
    client_need: String,
    #[serde(default)]
    price_eur: Option<u32>,
    #[serde(default)]
    workflow_url: Option<String>,
    #[serde(default)]
    delivery: Vec<DeliveryItem>,
}

#[derive(Deserialize, Clone)]
struct ClientWorkflowsResponse {
    email: String,
    workflows: Vec<ClientWorkflow>,
}

/// FR label + colour for a pipeline stage.
fn stage_label(stage: &str) -> (&'static str, &'static str) {
    match stage {
        "live" => ("Actif", "text-green-400"),
        "failed" => ("Échec — équipe notifiée", "text-red-400"),
        "awaiting_payment" => ("En attente de paiement", "text-amber-400"),
        _ => ("En cours de déploiement", "text-amber-400"),
    }
}

#[component]
pub fn Espace() -> impl IntoView {
    // "loading" | "forbidden" | "error" | "ready"
    let state = create_rw_signal::<&'static str>("loading");
    let data = create_rw_signal::<Option<ClientWorkflowsResponse>>(None);

    spawn_local(async move {
        let sid = session_id();
        if sid.is_empty() { state.set("forbidden"); return; }
        match Request::get(&format!("/api/client/workflows?sid={sid}")).send().await {
            Ok(resp) if resp.status() == 200 => match resp.json::<ClientWorkflowsResponse>().await {
                Ok(r) => { data.set(Some(r)); state.set("ready"); }
                Err(_) => state.set("error"),
            },
            Ok(resp) if resp.status() == 403 => state.set("forbidden"),
            _ => state.set("error"),
        }
    });

    view! {
        <div class="min-h-screen bg-deep px-6 py-12">
            <div class="max-w-3xl mx-auto space-y-8">
                <div class="space-y-1">
                    <h1 class="text-2xl font-bold text-primary">"Votre espace"</h1>
                    {move || data.get().map(|d| view! {
                        <p class="text-sm text-muted">{format!("Connecté en tant que {}", d.email)}</p>
                    })}
                </div>

                {move || match state.get() {
                    "loading" => view! { <p class="text-sm text-muted">"Chargement…"</p> }.into_view(),
                    "forbidden" => view! {
                        <div class="glass rounded-2xl p-6 text-sm text-secondary">
                            "Connectez-vous depuis le lien reçu par email pour accéder à vos workflows."
                        </div>
                    }.into_view(),
                    "error" => view! {
                        <p class="text-sm text-red-400">"Impossible de charger vos workflows pour le moment."</p>
                    }.into_view(),
                    _ => {
                        let workflows = data.get().map(|d| d.workflows).unwrap_or_default();
                        if workflows.is_empty() {
                            view! { <p class="text-sm text-muted">"Aucun workflow pour l'instant."</p> }.into_view()
                        } else {
                            view! {
                                <div class="space-y-6">
                                    <For each=move || data.get().map(|d| d.workflows).unwrap_or_default()
                                         key=|w| w.pipeline_id.clone() let:w>
                                        {workflow_card(w)}
                                    </For>
                                </div>
                            }.into_view()
                        }
                    }
                }}
            </div>
        </div>
    }
}

fn workflow_card(w: ClientWorkflow) -> impl IntoView {
    let (label, colour) = stage_label(&w.stage);
    let has_delivery = !w.delivery.is_empty();
    view! {
        <div class="glass rounded-2xl p-6 space-y-4">
            <div class="flex items-start justify-between gap-4">
                <div class="space-y-1">
                    <p class="text-sm font-semibold text-primary">{w.client_need.clone()}</p>
                    <span class=format!("text-xs font-medium {colour}")>{label}</span>
                </div>
                {w.price_eur.map(|p| view! {
                    <span class="text-xs text-secondary whitespace-nowrap">{format!("{p} €")}</span>
                })}
            </div>

            {w.workflow_url.clone().map(|url| view! {
                <a href=url target="_blank" rel="noopener noreferrer"
                   class="inline-flex items-center gap-1 text-xs font-medium text-cyan hover:text-cyan-mid transition-colors">
                    "Voir le workflow"
                </a>
            })}

            {has_delivery.then(|| view! {
                <div class="border-t border-subtle pt-4 space-y-3">
                    <h3 class="text-xs font-semibold text-primary uppercase tracking-wide">"Connexion de vos services"</h3>
                    <div class="space-y-3">
                        <For each={let d = w.delivery.clone(); move || d.clone()}
                             key=|i| i.service.clone() let:item>
                            {delivery_row(item)}
                        </For>
                    </div>
                </div>
            })}
        </div>
    }
}
