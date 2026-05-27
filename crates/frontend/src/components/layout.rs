use leptos::*;
use crate::components::contact_modal::ContactModal;
use crate::components::theme_toggle::ThemeToggle;
use crate::pages::home::Home;
use crate::pages::chat::Chat;
use crate::i18n::{Lang, t};

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    Chat,
}

fn scroll_to_gallery() {
    let _ = web_sys::window().and_then(|w| {
        let f = js_sys::Function::new_no_args(
            "document.getElementById('gallery')?.scrollIntoView({behavior:'smooth'})"
        );
        w.set_timeout_with_callback_and_timeout_and_arguments_0(&f, 50).ok()
    });
}

#[component]
pub fn Layout() -> impl IntoView {
    let is_contact_open = create_rw_signal(false);
    let active_page = create_rw_signal(Page::Home);
    let lang = create_rw_signal(Lang::Fr);

    provide_context(lang);

    let on_contact = move |_| is_contact_open.set(true);

    log::info!("Layout rendering...");

    let nav_btn_class = move |page: Page| {
        let base = "text-sm font-medium transition-colors";
        if active_page.get() == page {
            format!("{base} text-red-600")
        } else {
            format!("{base} text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white")
        }
    };

    let lang_btn_class = move |l: Lang| {
        if lang.get() == l {
            "text-xs font-semibold text-red-600"
        } else {
            "text-xs font-medium text-gray-400 hover:text-gray-600 dark:hover:text-gray-300 transition-colors"
        }
    };

    view! {
        <div class="min-h-screen bg-white dark:bg-black text-gray-900 dark:text-white transition-colors duration-300">
            <nav class="sticky top-0 z-40 bg-white/95 dark:bg-black/95 border-b border-gray-200 dark:border-gray-800 backdrop-blur-sm">
                <div class="max-w-7xl mx-auto px-6 py-4 flex justify-between items-center">
                    <button
                        on:click=move |_| active_page.set(Page::Home)
                        class="text-2xl font-bold tracking-tight"
                    >
                        <span class="text-red-600">"pointe"</span>
                        <span class="text-gray-900 dark:text-white">"."</span>
                        <span class="text-gray-900 dark:text-white">"dev"</span>
                    </button>
                    <div class="flex items-center gap-6">
                        <button
                            on:click=move |_| active_page.set(Page::Home)
                            class=move || nav_btn_class(Page::Home)
                        >
                            {move || t(lang.get(), "nav.home")}
                        </button>
                        <button
                            on:click=move |_| {
                                active_page.set(Page::Home);
                                scroll_to_gallery();
                            }
                            class="text-sm font-medium text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors"
                        >
                            {move || t(lang.get(), "nav.solutions")}
                        </button>
                        <button
                            on:click=move |_| active_page.set(Page::Chat)
                            class="px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 transition-colors text-sm font-medium"
                        >
                            {move || t(lang.get(), "nav.talk")}
                        </button>

                        {/* Language switcher */}
                        <div class="flex items-center gap-1 border border-gray-200 dark:border-gray-700 rounded-md px-2 py-1">
                            <button
                                on:click=move |_| lang.set(Lang::Fr)
                                class=move || lang_btn_class(Lang::Fr)
                            >"FR"</button>
                            <span class="text-gray-300 dark:text-gray-700 text-xs">"|"</span>
                            <button
                                on:click=move |_| lang.set(Lang::En)
                                class=move || lang_btn_class(Lang::En)
                            >"EN"</button>
                            <span class="text-gray-300 dark:text-gray-700 text-xs">"|"</span>
                            <button
                                on:click=move |_| lang.set(Lang::De)
                                class=move || lang_btn_class(Lang::De)
                            >"DE"</button>
                        </div>

                        <ThemeToggle />
                    </div>
                </div>
            </nav>

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
                }}
            </main>

            <ContactModal is_open=is_contact_open />

            {move || (active_page.get() == Page::Home).then(|| view! {
                <footer class="bg-gray-50 dark:bg-gray-900 border-t border-gray-200 dark:border-gray-800 py-12 px-6">
                    <div class="max-w-7xl mx-auto">
                        <div class="grid grid-cols-1 md:grid-cols-3 gap-8 mb-8">
                            <div>
                                <h3 class="font-bold mb-4">"pointe.dev"</h3>
                                <p class="text-sm text-gray-600 dark:text-gray-400">
                                    {move || t(lang.get(), "footer.tagline")}
                                </p>
                            </div>
                            <div>
                                <h4 class="font-semibold mb-4">{move || t(lang.get(), "footer.product")}</h4>
                                <ul class="space-y-2 text-sm text-gray-600 dark:text-gray-400">
                                    <li>
                                        <button
                                            on:click=move |_| {
                                                active_page.set(Page::Home);
                                                scroll_to_gallery();
                                            }
                                            class="hover:text-red-600 transition-colors"
                                        >
                                            {move || t(lang.get(), "footer.solutions")}
                                        </button>
                                    </li>
                                    <li>
                                        <button class="hover:text-red-600 transition-colors" on:click=on_contact>
                                            {move || t(lang.get(), "footer.contact")}
                                        </button>
                                    </li>
                                </ul>
                            </div>
                            <div>
                                <h4 class="font-semibold mb-4">{move || t(lang.get(), "footer.legal")}</h4>
                                <ul class="space-y-2 text-sm text-gray-600 dark:text-gray-400">
                                    <li><a href="#" class="hover:text-red-600 transition-colors">{move || t(lang.get(), "footer.privacy")}</a></li>
                                    <li><a href="#" class="hover:text-red-600 transition-colors">{move || t(lang.get(), "footer.terms")}</a></li>
                                </ul>
                            </div>
                        </div>
                        <div class="border-t border-gray-200 dark:border-gray-800 pt-8 text-center text-sm text-gray-600 dark:text-gray-400">
                            <p>{move || t(lang.get(), "footer.rights")}</p>
                        </div>
                    </div>
                </footer>
            })}
        </div>
    }
}
