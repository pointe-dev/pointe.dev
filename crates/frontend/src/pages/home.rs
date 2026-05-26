use leptos::*;
use crate::components::behind_curtain::BehindCurtain;
use crate::components::hero::Hero;

#[component]
pub fn Home() -> impl IntoView {
    
    log::info!("🏠 Home page rendering...");
    
    view! {
        <div class="bg-white dark:bg-black text-gray-900 dark:text-white transition-colors duration-300">
            {/* Hero Section */}
            <Hero on_contact_click=move || {
                if let Some(elem) = web_sys::window()
                    .and_then(|w| w.document())
                    .and_then(|d| d.body())
                {
                    let _ = elem.class_list().add_1("contact-modal-open");
                }
            } />

            {/* Behind the Curtain Section */}
            <div class="bg-white dark:bg-black transition-colors duration-300">
                <div class="max-w-7xl mx-auto px-6 py-20">
                    <h2 class="text-4xl font-bold text-center mb-8 text-gray-900 dark:text-white">
                        "See The Magic Behind the Scenes"
                    </h2>
                    <p class="text-center text-gray-600 dark:text-gray-400 mb-16 max-w-2xl mx-auto">
                        "Slide to reveal the complexity hidden beneath our elegant interface"
                    </p>
                    <BehindCurtain />
                </div>
            </div>

            {/* Services Section */}
            <div class="bg-gray-50 dark:bg-gray-900 py-20 border-t border-gray-200 dark:border-gray-800 transition-colors duration-300">
                <div class="max-w-7xl mx-auto px-6">
                    <h2 class="text-4xl font-bold text-center mb-16 text-gray-900 dark:text-white">
                        "Our Services"
                    </h2>

                    <section class="grid grid-cols-1 md:grid-cols-3 gap-8">
                        <div class="card border border-gray-200 dark:border-gray-800 p-8 bg-white dark:bg-gray-800 hover:border-red-600 dark:hover:border-red-600 transition-colors rounded-lg">
                            <h3 class="card-title text-red-600 font-bold text-3xl mb-4">"01"</h3>
                            <h4 class="text-xl font-bold mb-4 text-gray-900 dark:text-white">"AI Product Commercialization"</h4>
                            <p class="card-description text-gray-600 dark:text-gray-400">
                                "Transform raw AI models into production-grade SaaS applications."
                            </p>
                        </div>

                        <div class="card border border-gray-200 dark:border-gray-800 p-8 bg-white dark:bg-gray-800 hover:border-red-600 dark:hover:border-red-600 transition-colors rounded-lg">
                            <h3 class="card-title text-red-600 font-bold text-3xl mb-4">"02"</h3>
                            <h4 class="text-xl font-bold mb-4 text-gray-900 dark:text-white">"Business Process Automation"</h4>
                            <p class="card-description text-gray-600 dark:text-gray-400">
                                "Replace manual spreadsheets with autonomous AI agent systems."
                            </p>
                        </div>

                        <div class="card border border-gray-200 dark:border-gray-800 p-8 bg-white dark:bg-gray-800 hover:border-red-600 dark:hover:border-red-600 transition-colors rounded-lg">
                            <h3 class="card-title text-red-600 font-bold text-3xl mb-4">"03"</h3>
                            <h4 class="text-xl font-bold mb-4 text-gray-900 dark:text-white">"High-Performance Systems"</h4>
                            <p class="card-description text-gray-600 dark:text-gray-400">
                                "Rust backends designed for microsecond latencies and absolute reliability."
                            </p>
                        </div>
                    </section>
                </div>
            </div>

            {/* Gallery Section (Placeholder) */}
            <div id="gallery" class="bg-white dark:bg-black py-20 border-t border-gray-200 dark:border-gray-800 transition-colors duration-300">
                <div class="max-w-7xl mx-auto px-6">
                    <h2 class="text-4xl font-bold text-center mb-4 text-gray-900 dark:text-white">
                        "Ready-to-Use Solutions"
                    </h2>
                    <p class="text-center text-gray-600 dark:text-gray-400 mb-16">
                        "Choose from our gallery of pre-built automation templates"
                    </p>
                    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                        {vec![
                            ("Lead Qualification", "Automatically qualify and score leads with AI"),
                            ("Email Automation", "Smart email workflows triggered by customer behavior"),
                            ("Support Ticketing", "Auto-route and prioritize support tickets"),
                        ].into_iter().map(|(title, desc)| {
                            view! {
                                <div class="p-6 bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-800 rounded-lg hover:border-red-600 hover:shadow-lg transition-all cursor-pointer">
                                    <h3 class="font-bold text-lg mb-2">{title}</h3>
                                    <p class="text-gray-600 dark:text-gray-400 text-sm">{desc}</p>
                                    <button class="mt-4 px-4 py-2 text-red-600 hover:text-red-700 font-semibold text-sm">
                                        "Learn More →"
                                    </button>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                </div>
            </div>
        </div>
    }
}
