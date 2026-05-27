use leptos::*;
use crate::i18n::{Lang, t};

#[component]
pub fn ArtPanel() -> impl IntoView {
    let lang = use_context::<RwSignal<Lang>>()
        .unwrap_or_else(|| create_rw_signal(Lang::Fr));

    view! {
        <div class="w-full h-full bg-white flex flex-col justify-center px-7 py-6">
            <p class="text-xs text-gray-400 uppercase tracking-widest mb-5">
                {move || t(lang.get(), "art.eyebrow")}
            </p>

            {/* n8n-style workflow */}
            <div class="flex flex-col flex-1 justify-center">

                {/* Node 1 — done */}
                <div class="flex items-center gap-3 px-3 py-2.5 rounded-lg border border-gray-100 bg-white">
                    <div class="w-5 h-5 rounded-full bg-gray-100 flex items-center justify-center shrink-0">
                        <span class="text-gray-400 text-xs leading-none font-bold">"✓"</span>
                    </div>
                    <div class="min-w-0">
                        <p class="text-xs font-semibold text-gray-700">"Lead FR-0847"</p>
                        <p class="text-xs text-gray-400">"CRM · entrant · 09:16:09"</p>
                    </div>
                </div>

                <div class="w-px h-3 bg-gray-200 ml-5 shrink-0"></div>

                {/* Node 2 — done */}
                <div class="flex items-center gap-3 px-3 py-2.5 rounded-lg border border-gray-100 bg-white">
                    <div class="w-5 h-5 rounded-full bg-gray-100 flex items-center justify-center shrink-0">
                        <span class="text-gray-400 text-xs leading-none font-bold">"✓"</span>
                    </div>
                    <div class="min-w-0">
                        <p class="text-xs font-semibold text-gray-700">"Score IA · 94 / 100"</p>
                        <p class="text-xs text-gray-400">"Qualification · 09:16:10"</p>
                    </div>
                </div>

                <div class="w-px h-3 bg-red-200 ml-5 shrink-0"></div>

                {/* Node 3 — running */}
                <div class="flex items-center gap-3 px-3 py-2.5 rounded-lg border border-red-100 bg-red-50">
                    <div class="w-5 h-5 rounded-full bg-red-100 flex items-center justify-center shrink-0">
                        <span class="w-2 h-2 rounded-full bg-red-500 animate-pulse inline-block"></span>
                    </div>
                    <div class="min-w-0">
                        <p class="text-xs font-semibold text-gray-900">"Assigné · Alice"</p>
                        <p class="text-xs text-red-400">"En cours..."</p>
                    </div>
                </div>

                <div class="w-px h-3 bg-gray-200 ml-5 shrink-0 opacity-40"></div>

                {/* Node 4 — pending */}
                <div class="flex items-center gap-3 px-3 py-2.5 rounded-lg border border-gray-100 opacity-40">
                    <div class="w-5 h-5 rounded-full border border-gray-300 shrink-0"></div>
                    <div class="min-w-0">
                        <p class="text-xs font-semibold text-gray-500">"Slack · #sales"</p>
                        <p class="text-xs text-gray-400">"En attente"</p>
                    </div>
                </div>
            </div>

            {/* Metrics */}
            <div class="grid grid-cols-2 gap-3 mt-5 pt-4 border-t border-gray-100">
                <div>
                    <p class="text-xs text-gray-400 uppercase tracking-wider mb-0.5">
                        {move || t(lang.get(), "art.uptime")}
                    </p>
                    <p class="text-lg font-bold text-gray-900">"99.9%"</p>
                </div>
                <div>
                    <p class="text-xs text-gray-400 uppercase tracking-wider mb-0.5">
                        {move || t(lang.get(), "art.latency")}
                    </p>
                    <p class="text-lg font-bold text-gray-900">"< 50ms"</p>
                </div>
            </div>
        </div>
    }
}
