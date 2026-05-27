use leptos::*;
use crate::i18n::{Lang, t};

#[component]
pub fn ArtPanel() -> impl IntoView {
    let lang = use_context::<RwSignal<Lang>>()
        .unwrap_or_else(|| create_rw_signal(Lang::Fr));

    view! {
        <div class="w-full h-full bg-white flex flex-col justify-center items-center px-10">
            <div class="w-full max-w-xs">
                <p class="text-xs text-gray-400 uppercase tracking-widest text-center mb-7">
                    {move || t(lang.get(), "art.eyebrow")}
                </p>

                <div class="space-y-2">
                    <div class="border border-gray-100 rounded-lg px-5 py-4">
                        <p class="text-xs text-gray-400 uppercase tracking-wider mb-1">
                            {move || t(lang.get(), "art.active")}
                        </p>
                        <div class="flex items-baseline gap-3">
                            <p class="text-3xl font-bold text-gray-900">"24"</p>
                            <p class="text-xs text-red-600 font-medium">{move || t(lang.get(), "art.trend")}</p>
                        </div>
                    </div>

                    <div class="border border-gray-100 rounded-lg px-5 py-4">
                        <p class="text-xs text-gray-400 uppercase tracking-wider mb-1">
                            {move || t(lang.get(), "art.uptime")}
                        </p>
                        <p class="text-3xl font-bold text-gray-900">"99.9%"</p>
                    </div>

                    <div class="border border-gray-100 rounded-lg px-5 py-4">
                        <p class="text-xs text-gray-400 uppercase tracking-wider mb-1">
                            {move || t(lang.get(), "art.latency")}
                        </p>
                        <p class="text-3xl font-bold text-gray-900">"< 50ms"</p>
                    </div>
                </div>

                <p class="text-xs text-gray-300 text-center mt-7">
                    {move || t(lang.get(), "art.caption")}
                </p>
            </div>
        </div>
    }
}
