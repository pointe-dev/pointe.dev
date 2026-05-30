use leptos::*;
use crate::components::consent_banner::ConsentBanner;
use crate::components::contact_modal::ContactModal;
use crate::components::theme_toggle::ThemeToggle;
use crate::pages::home::Home;
use crate::pages::chat::Chat;
use crate::pages::merci::Merci;
use crate::i18n::{Lang, t};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    Chat,
    Merci,
}

fn detect_initial_lang() -> Lang {
    let lang = js_sys::eval("(navigator.language || navigator.userLanguage || '')")
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
        .to_lowercase();
    if lang.starts_with("en") { Lang::En }
    else if lang.starts_with("de") { Lang::De }
    else { Lang::Fr }
}

fn detect_initial_page() -> Page {
    web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .map(|p| if p.starts_with("/merci") { Page::Merci } else { Page::Home })
        .unwrap_or(Page::Home)
}

fn scroll_to_top() {
    let _ = js_sys::Function::new_no_args(
        "window.scrollTo({top:0,behavior:'smooth'})"
    ).call0(&wasm_bindgen::JsValue::NULL);
}


#[component]
pub fn Layout() -> impl IntoView {
    let is_contact_open = create_rw_signal(false);
    let active_page = create_rw_signal(detect_initial_page());
    let lang = create_rw_signal(detect_initial_lang());

    provide_context(lang);

    let on_contact = move |_| is_contact_open.set(true);

    log::info!("Layout rendering...");

    // If the URL has ?_sid=... the user arrived from the email confirmation link.
    // Auto-navigate to Chat so they land directly in the unlocked conversation.
    create_effect(move |first| {
        if first.is_none() { return; }
        let has_sid = js_sys::eval(
            "(function(){\
               try{return new URLSearchParams(window.location.search).has('_sid');}catch(e){return false;}\
             })()"
        ).ok().and_then(|v| v.as_bool()).unwrap_or(false);
        if has_sid {
            active_page.set(Page::Chat);
        }
    });

    let lang_btn_class = move |l: Lang| {
        if lang.get() == l {
            "text-xs font-semibold text-red-400"
        } else {
            "text-xs font-medium text-muted hover:text-secondary transition-colors"
        }
    };

    view! {
        <div class="min-h-screen bg-deep text-primary">

            {/* ── NAV ──────────────────────────────────────────── */}
            <nav class="nav-glass sticky top-0 z-40">
                <div class="max-w-7xl mx-auto px-6 py-4 flex justify-between items-center">

                    {/* Logo */}
                    <button
                        on:click=move |_| { active_page.set(Page::Home); scroll_to_top(); }
                        class="text-2xl font-bold tracking-tight"
                    >
                        <span class="text-gradient">"pointe"</span>
                        <span class="text-primary">"."</span>
                        <span class="text-primary">"dev"</span>
                    </button>

                    <div class="flex items-center gap-6">
                        {/* Nav links */}
                        <button
                            on:click=move |_| { active_page.set(Page::Home); scroll_to_top(); }
                            class=move || {
                                if active_page.get() == Page::Home {
                                    "text-sm font-medium text-red-400 transition-colors"
                                } else {
                                    "text-sm font-medium text-secondary hover:text-primary transition-colors"
                                }
                            }
                        >
                            {move || t(lang.get(), "nav.home")}
                        </button>
                        <button
                            on:click=move |_| active_page.set(Page::Chat)
                            class="btn-primary btn-sm"
                        >
                            {move || t(lang.get(), "nav.talk")}
                        </button>

                        {/* Language switcher */}
                        <div class="flex items-center gap-1 glass rounded-md px-2 py-1">
                            <button
                                on:click=move |_| lang.set(Lang::Fr)
                                class=move || lang_btn_class(Lang::Fr)
                            >"FR"</button>
                            <span class="text-muted text-xs">"·"</span>
                            <button
                                on:click=move |_| lang.set(Lang::En)
                                class=move || lang_btn_class(Lang::En)
                            >"EN"</button>
                            <span class="text-muted text-xs">"·"</span>
                            <button
                                on:click=move |_| lang.set(Lang::De)
                                class=move || lang_btn_class(Lang::De)
                            >"DE"</button>
                        </div>

                        <ThemeToggle />
                    </div>
                </div>
            </nav>

            {/* ── PAGE CONTENT ─────────────────────────────────── */}
            <main class="flex-1">
                {move || match active_page.get() {
                    Page::Home => view! {
                        <div class="page-transition">
                            <Home on_chat_click=move || active_page.set(Page::Chat) />
                        </div>
                    }.into_view(),
                    Page::Chat => view! {
                        <div class="page-transition">
                            <Chat />
                        </div>
                    }.into_view(),
                    Page::Merci => view! {
                        <div class="page-transition">
                            <Merci on_home_click=move |_| active_page.set(Page::Home) />
                        </div>
                    }.into_view(),
                }}
            </main>

            <ContactModal is_open=is_contact_open on_chat=move || active_page.set(Page::Chat) />
            <ConsentBanner />

            {/* ── FOOTER (home only) ───────────────────────────── */}
            {move || (active_page.get() == Page::Home).then(|| view! {
                <footer class="bg-surface border-t border-subtle py-20 px-6">
                    <div class="max-w-7xl mx-auto">
                        <div class="grid grid-cols-1 md:grid-cols-3 gap-8 mb-10">
                            <div>
                                <p class="text-xl font-bold mb-3">
                                    <span class="text-gradient">"pointe"</span>
                                    <span class="text-primary">".dev"</span>
                                </p>
                                <p class="text-sm text-secondary leading-relaxed">
                                    {move || t(lang.get(), "footer.tagline")}
                                </p>
                            </div>
                            <div>
                                <h4 class="text-sm font-semibold text-primary mb-4">
                                    {move || t(lang.get(), "footer.product")}
                                </h4>
                                <ul class="space-y-2 text-sm text-secondary">
                                    <li>
                                        <button class="hover:text-red-400 transition-colors" on:click=on_contact>
                                            {move || t(lang.get(), "footer.contact")}
                                        </button>
                                    </li>
                                </ul>
                            </div>
                            <div>
                                <h4 class="text-sm font-semibold text-primary mb-4">
                                    {move || t(lang.get(), "footer.legal")}
                                </h4>
                                <ul class="space-y-2 text-sm text-secondary">
                                    <li>
                                        <a href="#" class="hover:text-red-400 transition-colors">
                                            {move || t(lang.get(), "footer.privacy")}
                                        </a>
                                    </li>
                                    <li>
                                        <a href="#" class="hover:text-red-400 transition-colors">
                                            {move || t(lang.get(), "footer.terms")}
                                        </a>
                                    </li>
                                </ul>
                            </div>
                        </div>
                        <div class="border-t border-subtle pt-8 text-center text-xs text-muted">
                            <p>{move || t(lang.get(), "footer.rights")}</p>
                        </div>
                    </div>
                </footer>
            })}
        </div>
    }
}
