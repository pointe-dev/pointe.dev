use leptos::*;

#[component]
pub fn Home() -> impl IntoView {
    view! {
        <div class="max-w-7xl mx-auto px-6 py-20">
            <section class="text-center mb-20">
                <h1 class="text-6xl font-bold mb-4 tracking-tight">
                    <span class="text-red-600">Invisible</span> Complexity.
                </h1>
                <h2 class="text-6xl font-bold mb-8 text-gray-400">Effortless Grace.</h2>
                <p class="text-xl text-gray-300 max-w-2xl mx-auto">
                    We turn bleeding-edge AI engineering into breathtaking user experiences.
                    From product commercialization to business automation, we hide the pain
                    so your operations feel weightless.
                </p>
            </section>

            <section class="grid grid-cols-1 md:grid-cols-3 gap-8 mb-20">
                <div class="border border-gray-800 p-8 hover:border-red-600 transition-colors">
                    <h3 class="text-red-600 font-bold text-lg mb-2">01</h3>
                    <h4 class="text-xl font-bold mb-4">AI Product Commercialization</h4>
                    <p class="text-gray-400">
                        Transform raw AI models into production-grade SaaS applications.
                    </p>
                </div>

                <div class="border border-gray-800 p-8 hover:border-red-600 transition-colors">
                    <h3 class="text-red-600 font-bold text-lg mb-2">02</h3>
                    <h4 class="text-xl font-bold mb-4">Business Process Automation</h4>
                    <p class="text-gray-400">
                        Replace manual spreadsheets with autonomous AI agent systems.
                    </p>
                </div>

                <div class="border border-gray-800 p-8 hover:border-red-600 transition-colors">
                    <h3 class="text-red-600 font-bold text-lg mb-2">03</h3>
                    <h4 class="text-xl font-bold mb-4">High-Performance Systems</h4>
                    <p class="text-gray-400">
                        Rust backends designed for microsecond latencies and absolute reliability.
                    </p>
                </div>
            </section>
        </div>
    }
}
