use leptos::*;
use gloo_net::http::Request;
use serde::Deserialize;

/// Admin dossier overview. Token-gated against `/api/admin/dossiers` (the same
/// admin secret as the ingest route). Read-only for v1: lists every prospect
/// pipeline with its stage, the visitor's email, and the published pitch price.

#[derive(Deserialize, Clone)]
struct DossierPitch {
    price_eur_cents: u32,
    manual_quote: bool,
}

#[derive(Deserialize, Clone)]
struct Dossier {
    pipeline_id: String,
    #[serde(default)]
    email: Option<String>,
    client_need: String,
    #[serde(default)]
    summary: Option<String>,
    stage: String,
    #[serde(default)]
    stage_reason: Option<String>,
    updated_at: String,
    #[serde(default)]
    pitch: Option<DossierPitch>,
}

const TOKEN_KEY: &str = "pointe_admin_token";

fn ls_get(key: &str) -> Option<String> {
    web_sys::window()?.local_storage().ok()??.get_item(key).ok()?
}

fn ls_set(key: &str, val: &str) {
    if let Some(Ok(Some(store))) = web_sys::window().map(|w| w.local_storage()) {
        let _ = store.set_item(key, val);
    }
}

async fn fetch_dossiers(token: String) -> Result<Vec<Dossier>, String> {
    let resp = Request::get("/api/admin/dossiers")
        .header("x-admin-token", &token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    match resp.status() {
        200 => resp.json::<Vec<Dossier>>().await.map_err(|e| e.to_string()),
        401 => Err("Token invalide.".to_string()),
        503 => Err("Aucun token admin configuré côté serveur.".to_string()),
        s => Err(format!("Erreur serveur ({s}).")),
    }
}

/// FR label + Tailwind colour classes for a pipeline stage.
fn stage_badge(stage: &str) -> (&'static str, &'static str) {
    match stage {
        "qualifying"         => ("Qualification",          "bg-slate-500/15 text-slate-300"),
        "researching"        => ("Recherche",              "bg-sky-500/15 text-sky-300"),
        "designing"          => ("Conception",             "bg-sky-500/15 text-sky-300"),
        "design_validating"  => ("Validation design",      "bg-sky-500/15 text-sky-300"),
        "decomposing"        => ("Découpage",              "bg-amber-500/15 text-amber-300"),
        "building"           => ("Construction",           "bg-amber-500/15 text-amber-300"),
        "validating"         => ("Validation build",       "bg-amber-500/15 text-amber-300"),
        "pricing"            => ("Chiffrage",              "bg-indigo-500/15 text-indigo-300"),
        "pricing_validating" => ("Validation prix",        "bg-indigo-500/15 text-indigo-300"),
        "awaiting_payment"   => ("En attente de paiement", "bg-amber-500/15 text-amber-300"),
        "deploying"          => ("Déploiement",            "bg-amber-500/15 text-amber-300"),
        "live"               => ("En production",          "bg-green-500/15 text-green-300"),
        "saved_for_human"    => ("À revoir",               "bg-amber-500/15 text-amber-300"),
        "failed"             => ("Échec",                  "bg-red-500/15 text-red-300"),
        _                    => ("—",                      "bg-slate-500/15 text-slate-300"),
    }
}

fn format_price(pitch: &DossierPitch) -> String {
    if pitch.manual_quote || pitch.price_eur_cents == 0 {
        "Sur devis".to_string()
    } else {
        format!("{} €", pitch.price_eur_cents / 100)
    }
}

#[component]
pub fn Admin() -> impl IntoView {
    let token = create_rw_signal(ls_get(TOKEN_KEY).unwrap_or_default());

    let load = create_action(|token: &String| {
        let token = token.clone();
        async move { fetch_dossiers(token).await }
    });

    // Auto-load if a token was already saved from a previous visit.
    create_effect(move |first| {
        if first.is_some() { return; }
        let t = token.get_untracked();
        if !t.is_empty() {
            load.dispatch(t);
        }
    });

    let submit = move |_| {
        let t = token.get();
        if t.is_empty() { return; }
        ls_set(TOKEN_KEY, &t);
        load.dispatch(t);
    };

    let pending = load.pending();
    let result = load.value();

    view! {
        <div class="min-h-screen bg-deep px-6 py-12">
            <div class="max-w-4xl mx-auto space-y-8">

                <div class="flex items-center justify-between flex-wrap gap-4">
                    <h1 class="text-2xl font-bold text-primary">
                        "Dossiers" <span class="text-muted font-normal text-base ml-2">"admin"</span>
                    </h1>
                    <a
                        href="https://dashboard.stripe.com/payments"
                        target="_blank"
                        rel="noopener noreferrer"
                        class="text-sm font-medium text-secondary hover:text-primary transition-colors inline-flex items-center gap-1.5"
                    >
                        "Tableau de bord Stripe"
                        <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"/>
                        </svg>
                    </a>
                </div>

                // Token bar
                <div class="glass rounded-2xl p-4 flex flex-wrap items-center gap-3">
                    <input
                        type="password"
                        placeholder="Token admin"
                        prop:value=move || token.get()
                        on:input=move |ev| token.set(event_target_value(&ev))
                        class="flex-1 min-w-[200px] bg-surface border border-subtle rounded-lg px-3 py-2 text-sm text-primary placeholder:text-muted focus:outline-none focus:border-red-400"
                    />
                    <button
                        on:click=submit
                        class="btn-primary btn-sm"
                        disabled=move || pending.get()
                    >
                        {move || if pending.get() { "Chargement…" } else { "Charger" }}
                    </button>
                </div>

                // Results
                {move || match result.get() {
                    None => view! { <p class="text-sm text-muted">"Entrez votre token pour charger les dossiers."</p> }.into_view(),
                    Some(Err(e)) => view! {
                        <p class="text-sm text-red-400">{e}</p>
                    }.into_view(),
                    Some(Ok(dossiers)) if dossiers.is_empty() => view! {
                        <p class="text-sm text-muted">"Aucun dossier pour le moment."</p>
                    }.into_view(),
                    Some(Ok(dossiers)) => {
                        let count = dossiers.len();
                        view! {
                            <div class="space-y-3">
                                <p class="text-xs text-muted">{format!("{count} dossier(s)")}</p>
                                {dossiers.into_iter().map(|d| dossier_card(d, token, load)).collect_view()}
                            </div>
                        }.into_view()
                    }
                }}

            </div>
        </div>
    }
}

fn dossier_card(
    d: Dossier,
    token: RwSignal<String>,
    load: Action<String, Result<Vec<Dossier>, String>>,
) -> impl IntoView {
    let (stage_label, stage_class) = stage_badge(&d.stage);
    let price = d.pitch.as_ref().map(format_price);
    let updated = d.updated_at.split('T').next().unwrap_or(&d.updated_at).to_string();
    let pid = d.pipeline_id.clone();
    let email = d.email.unwrap_or_else(|| "— pas d'email".to_string());

    // Re-run the dossier's pipeline (recovers a stuck/failed one). Confirmed
    // because publish re-emails the client; on success we refresh the list.
    let respawn = move |_| {
        let confirmed = web_sys::window()
            .and_then(|w| w.confirm_with_message(
                "Relancer ce dossier ? Une nouvelle proposition sera générée et envoyée au client.",
            ).ok())
            .unwrap_or(false);
        if !confirmed { return; }
        let t = token.get_untracked();
        let pid = pid.clone();
        spawn_local(async move {
            let _ = Request::post(&format!("/api/admin/dossiers/{pid}/respawn"))
                .header("x-admin-token", &t)
                .send()
                .await;
            load.dispatch(t);
        });
    };

    view! {
        <div class="glass rounded-2xl p-5 space-y-3">
            <div class="flex items-start justify-between gap-3 flex-wrap">
                <div class="flex items-center gap-2 flex-wrap">
                    <span class=format!("text-xs font-medium px-2.5 py-1 rounded-full {stage_class}")>
                        {stage_label}
                    </span>
                    {d.stage_reason.map(|r| view! {
                        <span class="text-xs text-muted">{r}</span>
                    })}
                </div>
                <div class="text-right">
                    {price.map(|p| view! {
                        <p class="text-sm font-semibold text-primary">{p}</p>
                    })}
                    <p class="text-xs text-muted">{updated}</p>
                </div>
            </div>

            <p class="text-sm text-secondary leading-relaxed">{d.client_need}</p>

            {d.summary.map(|s| view! {
                <p class="text-xs text-muted leading-relaxed">{s}</p>
            })}

            <div class="flex items-center justify-between gap-3 flex-wrap pt-1 text-xs text-muted">
                <span>{email}</span>
                <div class="flex items-center gap-3">
                    <button
                        on:click=respawn
                        class="text-xs font-medium px-2.5 py-1 rounded-md border border-red-500/40 bg-red-500/10 text-red-300 hover:bg-red-500/20 hover:border-red-400 transition-colors"
                    >"↻ Relancer"</button>
                    <span class="font-mono opacity-60">{d.pipeline_id}</span>
                </div>
            </div>
        </div>
    }
}
