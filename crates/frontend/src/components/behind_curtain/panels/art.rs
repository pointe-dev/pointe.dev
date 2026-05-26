//! "The Art" panel - clean, minimal dashboard.

use leptos::*;

/// The "Art" panel - displays clean metrics and dashboard.
#[component]
pub fn ArtPanel() -> impl IntoView {
    view! {
        <div class="w-full h-full bg-white p-12 flex flex-col justify-center items-center">
            <div class="text-center max-w-md">
                <h3 class="text-4xl font-bold text-black mb-4">
                    "Effortless Grace"
                </h3>

                <div class="space-y-6">
                    <div class="bg-gray-50 p-6 rounded-lg border border-gray-200">
                        <p class="text-gray-600 text-sm uppercase tracking-wide">"Automations Running"</p>
                        <p class="text-3xl font-bold text-green-600">"✓ 24"</p>
                    </div>

                    <div class="bg-gray-50 p-6 rounded-lg border border-gray-200">
                        <p class="text-gray-600 text-sm uppercase tracking-wide">"Uptime"</p>
                        <p class="text-3xl font-bold text-blue-600">99.9%</p>
                    </div>

                    <div class="bg-gray-50 p-6 rounded-lg border border-gray-200">
                        <p class="text-gray-600 text-sm uppercase tracking-wide">"Response Time"</p>
                        <p class="text-3xl font-bold text-purple-600">"<50ms"</p>
                    </div>
                </div>

                <p class="text-gray-500 mt-8 text-sm">
                    "Everything you need. Nothing you don't."
                </p>
            </div>
        </div>
    }
}
