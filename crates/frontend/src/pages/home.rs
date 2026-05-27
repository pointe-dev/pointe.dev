use leptos::*;
use crate::components::behind_curtain::BehindCurtain;
use crate::components::hero::Hero;

#[component]
pub fn Home(on_chat_click: impl Fn() + Clone + 'static) -> impl IntoView {
    log::info!("🏠 Home page rendering...");

    let c_hero = on_chat_click.clone();
    let c1     = on_chat_click.clone();
    let c2     = on_chat_click.clone();
    let c3     = on_chat_click.clone();

    view! {
        <div class="bg-white dark:bg-black text-gray-900 dark:text-white transition-colors duration-300">

            <Hero on_chat_click=move || c_hero() />

            {/* Behind the Curtain */}
            <div class="bg-white dark:bg-black transition-colors duration-300 border-t border-gray-100 dark:border-gray-900">
                <div class="max-w-5xl mx-auto px-6 py-24">
                    <p class="text-xs text-gray-400 uppercase tracking-widest text-center mb-4">"Comment ça marche"</p>
                    <h2 class="text-4xl font-bold text-center mb-4 text-gray-900 dark:text-white">
                        "L'élégance cache la complexité."
                    </h2>
                    <p class="text-center text-gray-500 dark:text-gray-400 mb-16 max-w-xl mx-auto font-light">
                        "Glissez pour voir l'ingénierie derrière la grace."
                    </p>
                    <BehindCurtain />
                </div>
            </div>

            {/* Services */}
            <div class="bg-gray-50 dark:bg-gray-950 py-24 border-t border-gray-100 dark:border-gray-900 transition-colors duration-300">
                <div class="max-w-7xl mx-auto px-6">
                    <p class="text-xs text-gray-400 uppercase tracking-widest text-center mb-4">"Ce que nous construisons"</p>
                    <h2 class="text-4xl font-bold text-center mb-16 text-gray-900 dark:text-white">
                        "Trois disciplines. Une promesse."
                    </h2>
                    <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                        <div class="border border-gray-200 dark:border-gray-800 p-8 bg-white dark:bg-black hover:border-red-600 dark:hover:border-red-600 transition-colors rounded-lg">
                            <p class="text-red-600 font-bold text-4xl mb-5 font-mono">"01"</p>
                            <h4 class="text-lg font-bold mb-3 text-gray-900 dark:text-white">"AI Product Commercialization"</h4>
                            <p class="text-gray-500 dark:text-gray-400 text-sm leading-relaxed">
                                "Vos modèles IA passent en production. Nous construisons l'interface, l'API, et le pipeline qui les rendent vendables."
                            </p>
                        </div>
                        <div class="border border-gray-200 dark:border-gray-800 p-8 bg-white dark:bg-black hover:border-red-600 dark:hover:border-red-600 transition-colors rounded-lg">
                            <p class="text-red-600 font-bold text-4xl mb-5 font-mono">"02"</p>
                            <h4 class="text-lg font-bold mb-3 text-gray-900 dark:text-white">"Business Process Automation"</h4>
                            <p class="text-gray-500 dark:text-gray-400 text-sm leading-relaxed">
                                "Les tâches répétitives deviennent des agents autonomes. Reporting, qualification, routing — sans supervision humaine."
                            </p>
                        </div>
                        <div class="border border-gray-200 dark:border-gray-800 p-8 bg-white dark:bg-black hover:border-red-600 dark:hover:border-red-600 transition-colors rounded-lg">
                            <p class="text-red-600 font-bold text-4xl mb-5 font-mono">"03"</p>
                            <h4 class="text-lg font-bold mb-3 text-gray-900 dark:text-white">"High-Performance Infrastructure"</h4>
                            <p class="text-gray-500 dark:text-gray-400 text-sm leading-relaxed">
                                "Backends Rust conçus pour des latences inférieures à 10ms. Fiables, scalables, maintenables."
                            </p>
                        </div>
                    </div>
                </div>
            </div>

            {/* Solutions gallery */}
            <div id="gallery" class="bg-white dark:bg-black py-24 border-t border-gray-100 dark:border-gray-900 transition-colors duration-300">
                <div class="max-w-7xl mx-auto px-6">
                    <p class="text-xs text-gray-400 uppercase tracking-widest text-center mb-4">"Livrables concrets"</p>
                    <h2 class="text-4xl font-bold text-center mb-4 text-gray-900 dark:text-white">
                        "Prêt à déployer."
                    </h2>
                    <p class="text-center text-gray-500 dark:text-gray-400 mb-16 max-w-xl mx-auto font-light">
                        "Trois solutions qui sortent directement de nos trois disciplines."
                    </p>
                    <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                        <div class="p-7 bg-gray-50 dark:bg-gray-950 border border-gray-100 dark:border-gray-900 rounded-lg hover:border-red-600 hover:shadow-lg transition-all cursor-pointer">
                            <p class="text-xs text-red-600 font-semibold uppercase tracking-wider mb-3">"01 — AI Product"</p>
                            <h3 class="font-bold text-lg mb-2 text-gray-900 dark:text-white">"AI Qualification Engine"</h3>
                            <p class="text-gray-500 dark:text-gray-400 text-sm leading-relaxed">
                                "Scoring et qualification de leads en temps réel, connecté à votre CRM. Déployé en deux semaines."
                            </p>
                            <button on:click=move |_| c1() class="mt-5 text-sm text-red-600 font-semibold hover:text-red-700 transition-colors">
                                "En savoir plus →"
                            </button>
                        </div>
                        <div class="p-7 bg-gray-50 dark:bg-gray-950 border border-gray-100 dark:border-gray-900 rounded-lg hover:border-red-600 hover:shadow-lg transition-all cursor-pointer">
                            <p class="text-xs text-red-600 font-semibold uppercase tracking-wider mb-3">"02 — Automation"</p>
                            <h3 class="font-bold text-lg mb-2 text-gray-900 dark:text-white">"Autonomous Ops Hub"</h3>
                            <p class="text-gray-500 dark:text-gray-400 text-sm leading-relaxed">
                                "Reporting hebdo, chaînes d'approbation, routage de tickets — des agents qui tournent sans vous."
                            </p>
                            <button on:click=move |_| c2() class="mt-5 text-sm text-red-600 font-semibold hover:text-red-700 transition-colors">
                                "En savoir plus →"
                            </button>
                        </div>
                        <div class="p-7 bg-gray-50 dark:bg-gray-950 border border-gray-100 dark:border-gray-900 rounded-lg hover:border-red-600 hover:shadow-lg transition-all cursor-pointer">
                            <p class="text-xs text-red-600 font-semibold uppercase tracking-wider mb-3">"03 — Infrastructure"</p>
                            <h3 class="font-bold text-lg mb-2 text-gray-900 dark:text-white">"High-Speed API Layer"</h3>
                            <p class="text-gray-500 dark:text-gray-400 text-sm leading-relaxed">
                                "Microservices Rust reliant vos outils SaaS avec moins de 10ms de latence et zéro temps d'arrêt."
                            </p>
                            <button on:click=move |_| c3() class="mt-5 text-sm text-red-600 font-semibold hover:text-red-700 transition-colors">
                                "En savoir plus →"
                            </button>
                        </div>
                    </div>
                </div>
            </div>

        </div>
    }
}
