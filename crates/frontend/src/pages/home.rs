use leptos::*;
use crate::components::behind_curtain::BehindCurtain;

#[component]
pub fn Home() -> impl IntoView {
    log::info!("🏠 Home page rendering...");
    
    view! {
        <div class="bg-black text-white">
            {/* Hero + Behind the Curtain */}
            <div class="max-w-7xl mx-auto px-6">
                <BehindCurtain />
            </div>

            {/* Services Section */}
            <div class="bg-black py-20 border-t border-gray-800">
                <div class="max-w-7xl mx-auto px-6">
                    <h2 class="text-4xl font-bold text-center mb-16 text-white">
                        "Our Services"
                    </h2>

                    <section class="grid grid-cols-1 md:grid-cols-3 gap-8">
                        <div class="card border border-gray-800 p-8 hover:border-red-600 transition-colors">
                            <h3 class="card-title text-red-600 font-bold">01</h3>
                            <h4 class="text-xl font-bold mb-4">AI Product Commercialization</h4>
                            <p class="card-description text-gray-400">
                                Transform raw AI models into production-grade SaaS applications.
                            </p>
                        </div>

                        <div class="card border border-gray-800 p-8 hover:border-red-600 transition-colors">
                            <h3 class="card-title text-red-600 font-bold">02</h3>
                            <h4 class="text-xl font-bold mb-4">Business Process Automation</h4>
                            <p class="card-description text-gray-400">
                                Replace manual spreadsheets with autonomous AI agent systems.
                            </p>
                        </div>

                        <div class="card border border-gray-800 p-8 hover:border-red-600 transition-colors">
                            <h3 class="card-title text-red-600 font-bold">03</h3>
                            <h4 class="text-xl font-bold mb-4">High-Performance Systems</h4>
                            <p class="card-description text-gray-400">
                                Rust backends designed for microsecond latencies and absolute reliability.
                            </p>
                        </div>
                    </section>
                </div>
            </div>
        </div>
    }
}
