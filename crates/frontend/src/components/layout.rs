use leptos::*;
use crate::components::contact_modal::ContactModal;
use crate::components::theme_toggle::ThemeToggle;

#[component]
pub fn Layout(children: Children) -> impl IntoView {
    let is_contact_open = create_rw_signal(false);
    
    let on_contact = move |_| {
        is_contact_open.set(true);
    };
    
    log::info!("Layout rendering...");
    
    view! {
        <div class="min-h-screen bg-white dark:bg-black text-gray-900 dark:text-white transition-colors duration-300">
            {/* Navigation */}
            <nav class="sticky top-0 z-40 bg-white dark:bg-black border-b border-gray-200 dark:border-gray-800 backdrop-blur-sm">
                <div class="max-w-7xl mx-auto px-6 py-4 flex justify-between items-center">
                    <h1 class="text-2xl font-bold tracking-tight">
                        <span class="text-red-600">"pointe"</span>
                        <span class="text-gray-900 dark:text-white">"."</span>
                        <span class="text-gray-900 dark:text-white">"dev"</span>
                    </h1>
                    <div class="flex items-center gap-4">
                        <a href="#gallery" class="text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors">
                            "Gallery"
                        </a>
                        <button
                            on:click=on_contact
                            class="px-4 py-2 bg-red-600 text-white rounded-lg hover:bg-red-700 transition-colors"
                        >
                            "Let's Talk"
                        </button>
                        <ThemeToggle />
                    </div>
                </div>
            </nav>

            {/* Main content */}
            <main class="flex-1">
                {children()}
            </main>

            {/* Contact Modal */}
            <ContactModal is_open=is_contact_open />

            {/* Footer */}
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
                                <li><a href="#" class="hover:text-red-600 transition-colors" on:click=on_contact>"Contact"</a></li>
                                <li><a href="#" class="hover:text-red-600 transition-colors">"Docs"</a></li>
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
                        <p>"© 2024 pointe.dev. All rights reserved."</p>
                    </div>
                </div>
            </footer>
        </div>
    }
}
