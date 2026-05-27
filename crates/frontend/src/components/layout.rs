use leptos::*;
use crate::components::contact_modal::ContactModal;
use crate::components::theme_toggle::ThemeToggle;
use crate::pages::home::Home;
use crate::pages::chat::Chat;

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    Chat,
}

#[component]
pub fn Layout() -> impl IntoView {
    let is_contact_open = create_rw_signal(false);
    let active_page = create_rw_signal(Page::Home);

    let on_contact = move |_| is_contact_open.set(true);

    log::info!("Layout rendering...");

    view! {
        <div class="min-h-screen bg-white dark:bg-black text-gray-900 dark:text-white transition-colors duration-300">
            <nav class="sticky top-0 z-40 bg-white dark:bg-black border-b border-gray-200 dark:border-gray-800 backdrop-blur-sm">
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
                            class=move || {
                                let base = "text-sm font-medium transition-colors";
                                if active_page.get() == Page::Home {
                                    format!("{base} text-red-600")
                                } else {
                                    format!("{base} text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white")
                                }
                            }
                        >
                            "Accueil"
                        </button>
                        <button
                            on:click=move |_| active_page.set(Page::Chat)
                            class=move || {
                                let base = "text-sm font-medium transition-colors";
                                if active_page.get() == Page::Chat {
                                    format!("{base} text-red-600")
                                } else {
                                    format!("{base} text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white")
                                }
                            }
                        >
                            "Chat"
                        </button>
                        {move || (active_page.get() == Page::Home).then(|| view! {
                            <a
                                href="#gallery"
                                class="text-sm font-medium text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors"
                            >
                                "Solutions"
                            </a>
                        })}
                        <button
                            on:click=on_contact
                            class="px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 transition-colors text-sm font-medium"
                        >
                            "Discutons"
                        </button>
                        <ThemeToggle />
                    </div>
                </div>
            </nav>

            <main class="flex-1">
                {move || match active_page.get() {
                    Page::Home => view! { <Home /> }.into_view(),
                    Page::Chat => view! { <Chat /> }.into_view(),
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
                                    "Enterprise AI automation. Invisible complexity, visible grace."
                                </p>
                            </div>
                            <div>
                                <h4 class="font-semibold mb-4">"Product"</h4>
                                <ul class="space-y-2 text-sm text-gray-600 dark:text-gray-400">
                                    <li><a href="#gallery" class="hover:text-red-600 transition-colors">"Solutions"</a></li>
                                    <li>
                                        <button class="hover:text-red-600 transition-colors" on:click=on_contact>
                                            "Contact"
                                        </button>
                                    </li>
                                </ul>
                            </div>
                            <div>
                                <h4 class="font-semibold mb-4">"Legal"</h4>
                                <ul class="space-y-2 text-sm text-gray-600 dark:text-gray-400">
                                    <li><a href="#" class="hover:text-red-600 transition-colors">"Privacy"</a></li>
                                    <li><a href="#" class="hover:text-red-600 transition-colors">"Terms"</a></li>
                                </ul>
                            </div>
                        </div>
                        <div class="border-t border-gray-200 dark:border-gray-800 pt-8 text-center text-sm text-gray-600 dark:text-gray-400">
                            <p>"© 2025 pointe.dev. All rights reserved."</p>
                        </div>
                    </div>
                </footer>
            })}
        </div>
    }
}
