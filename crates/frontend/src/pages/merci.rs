use leptos::*;
use gloo_net::http::Request;
use serde::Deserialize;

#[derive(Deserialize, Clone)]
struct PipelineStatus {
    stage: serde_json::Value,
    #[serde(default)]
    price_quote: Option<u32>,
    #[serde(default)]
    price_monthly: Option<u32>,
    #[serde(default)]
    price_justification: Option<String>,
    #[serde(default)]
    n8n_workflow_url: Option<String>,
}

fn get_pipeline_id_from_url() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    let search = search.trim_start_matches('?');
    for pair in search.split('&') {
        let mut parts = pair.splitn(2, '=');
        if parts.next() == Some("pipeline") {
            return parts.next().map(|v| v.to_string());
        }
    }
    None
}

#[component]
pub fn Merci(#[prop(into)] on_home_click: Callback<()>) -> impl IntoView {
    let pipeline_id = get_pipeline_id_from_url();
    let status: RwSignal<Option<PipelineStatus>> = create_rw_signal(None);
    let error = create_rw_signal(false);

    // Poll pipeline status every 4s until live or failed
    if let Some(pid) = pipeline_id.clone() {
        let pid = pid.clone();
        create_effect(move |_| {
            let pid = pid.clone();
            spawn_local(async move {
                loop {
                    match Request::get(&format!("/api/pipeline/{pid}")).send().await {
                        Ok(resp) if resp.status() == 200 => {
                            if let Ok(s) = resp.json::<PipelineStatus>().await {
                                let stage = s.stage["stage"].as_str().unwrap_or("").to_string();
                                status.set(Some(s));
                                if stage == "live" || stage == "failed" {
                                    break;
                                }
                            }
                        }
                        _ => { error.set(true); break; }
                    }
                    // Sleep 4s via a JS Promise
                    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
                        let _ = web_sys::window().unwrap()
                            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 4000);
                    });
                    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                }
            });
        });
    }

    view! {
        <div class="min-h-screen flex flex-col items-center justify-center px-6 py-16 bg-deep">
            <div class="max-w-lg w-full text-center space-y-8">

                // Icon
                <div class="flex justify-center">
                    <div class="w-20 h-20 rounded-full glass-cyan flex items-center justify-center shadow-cyan-glow">
                        <svg class="w-10 h-10 text-cyan" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                d="M5 13l4 4L19 7"/>
                        </svg>
                    </div>
                </div>

                // Title
                <div class="space-y-2">
                    <h1 class="text-3xl font-bold text-primary">
                        "Paiement confirmé"
                    </h1>
                    <p class="text-secondary">
                        "Votre workflow est en cours de déploiement."
                    </p>
                </div>

                // Pipeline status card
                {move || status.get().map(|s| {
                    let stage = s.stage["stage"].as_str().unwrap_or("").to_string();
                    view! {
                        <div class="glass rounded-2xl p-6 text-left space-y-4">

                            // Stage indicator
                            <div class="flex items-center gap-3">
                                {match stage.as_str() {
                                    "live" => view! {
                                        <span class="w-2.5 h-2.5 rounded-full bg-green-400"></span>
                                        <span class="text-sm font-medium text-green-400">"Workflow actif"</span>
                                    }.into_view(),
                                    "failed" => view! {
                                        <span class="w-2.5 h-2.5 rounded-full bg-red-500"></span>
                                        <span class="text-sm font-medium text-red-400">"Déploiement échoué — notre équipe a été notifiée"</span>
                                    }.into_view(),
                                    _ => view! {
                                        <span class="w-2.5 h-2.5 rounded-full bg-amber-400 animate-pulse"></span>
                                        <span class="text-sm font-medium text-amber-400">"Déploiement en cours…"</span>
                                    }.into_view(),
                                }}
                            </div>

                            // Pricing recap
                            {s.price_quote.map(|setup| view! {
                                <div class="border-t border-subtle pt-4 space-y-1">
                                    <div class="flex justify-between text-sm">
                                        <span class="text-secondary">"Mise en place (one-time)"</span>
                                        <span class="font-semibold text-primary">{format!("{}€", setup)}</span>
                                    </div>
                                    {s.price_monthly.map(|mo| view! {
                                        <div class="flex justify-between text-sm">
                                            <span class="text-secondary">"Abonnement mensuel"</span>
                                            <span class="font-semibold text-primary">{format!("{}€/mois", mo)}</span>
                                        </div>
                                    })}
                                </div>
                            })}

                            // Justification
                            {s.price_justification.map(|j| view! {
                                <p class="text-xs text-muted leading-relaxed">{j}</p>
                            })}

                            // Workflow link when live
                            {(stage == "live").then(|| s.n8n_workflow_url.map(|url| view! {
                                <a
                                    href=url
                                    target="_blank"
                                    rel="noopener noreferrer"
                                    class="inline-flex items-center gap-2 text-sm font-medium text-cyan hover:text-cyan-mid transition-colors"
                                >
                                    "Voir le workflow dans n8n"
                                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                            d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14"/>
                                    </svg>
                                </a>
                            }))}
                        </div>
                    }
                })}

                // No pipeline ID fallback
                {pipeline_id.is_none().then(|| view! {
                    <p class="text-sm text-muted">"Votre workflow sera actif sous quelques minutes."</p>
                })}

                // Next steps
                <div class="glass rounded-2xl p-6 text-left space-y-3">
                    <h3 class="text-sm font-semibold text-primary">"Prochaines étapes"</h3>
                    <ul class="space-y-2 text-sm text-secondary">
                        <li class="flex items-start gap-2">
                            <span class="text-red-400 mt-0.5">"→"</span>
                            "Vous recevrez une facture par email."
                        </li>
                        <li class="flex items-start gap-2">
                            <span class="text-red-400 mt-0.5">"→"</span>
                            "Notre équipe vous contactera pour finaliser les accès et tester le workflow."
                        </li>
                        <li class="flex items-start gap-2">
                            <span class="text-red-400 mt-0.5">"→"</span>
                            "Votre workflow sera opérationnel sous 24h."
                        </li>
                    </ul>
                </div>

                // Back to home
                <button
                    on:click=move |_| {
                        on_home_click.call(());
                        let _ = js_sys::Function::new_no_args(
                            "window.history.pushState(null,'','/');window.scrollTo({top:0})"
                        ).call0(&wasm_bindgen::JsValue::NULL);
                    }
                    class="text-sm text-muted hover:text-secondary transition-colors underline underline-offset-4"
                >
                    "← Retour à l'accueil"
                </button>

            </div>
        </div>
    }
}
