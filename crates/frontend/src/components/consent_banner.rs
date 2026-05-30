use leptos::*;

pub fn is_consent_given() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("consent").ok().flatten())
        .map(|v| v == "1")
        .unwrap_or(false)
}

fn read_decided() -> bool {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item("consent").ok().flatten())
        .is_some()
}

fn write_consent(accepted: bool) {
    if let Some(w) = web_sys::window() {
        if let Ok(Some(s)) = w.local_storage() {
            let _ = s.set_item("consent", if accepted { "1" } else { "0" });
        }
    }
}

#[component]
pub fn ConsentBanner() -> impl IntoView {
    let decided = create_rw_signal(read_decided());

    let accept = move |_| {
        write_consent(true);
        decided.set(true);
    };

    let refuse = move |_| {
        write_consent(false);
        // Remove any history that may have been written before decision
        if let Some(w) = web_sys::window() {
            if let Ok(Some(s)) = w.local_storage() {
                let _ = s.remove_item("chat_msgs");
            }
        }
        decided.set(true);
    };

    view! {
        {move || (!decided.get()).then(|| view! {
            <div class="consent-banner">
                <p class="consent-text">
                    "pointe.dev utilise le stockage local du navigateur pour sauvegarder votre session et l'historique de conversation. Aucune donnée n'est partagée avec des tiers."
                </p>
                <div class="consent-actions">
                    <button on:click=refuse class="consent-btn-refuse">"Refuser"</button>
                    <button on:click=accept class="consent-btn-accept">"Accepter"</button>
                </div>
            </div>
        })}
    }
}
