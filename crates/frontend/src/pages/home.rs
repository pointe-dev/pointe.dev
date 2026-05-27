use leptos::*;
use crate::components::behind_curtain::BehindCurtain;
use crate::components::hero::Hero;
use crate::i18n::{Lang, t};

#[component]
pub fn Home(on_chat_click: impl Fn() + Clone + 'static) -> impl IntoView {
    log::info!("🏠 Home page rendering...");

    let lang = use_context::<RwSignal<Lang>>()
        .unwrap_or_else(|| create_rw_signal(Lang::Fr));

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
                    <p class="text-xs text-gray-400 uppercase tracking-widest text-center mb-4">
                        {move || t(lang.get(), "curtain.eyebrow")}
                    </p>
                    <h2 class="text-4xl font-bold text-center mb-4 text-gray-900 dark:text-white">
                        {move || t(lang.get(), "curtain.title")}
                    </h2>
                    <p class="text-center text-gray-500 dark:text-gray-400 mb-16 max-w-xl mx-auto font-light">
                        {move || t(lang.get(), "curtain.sub")}
                    </p>
                    <BehindCurtain />
                </div>
            </div>

            {/* Services */}
            <div class="bg-gray-50 dark:bg-gray-950 py-24 border-t border-gray-100 dark:border-gray-900 transition-colors duration-300">
                <div class="max-w-7xl mx-auto px-6">
                    <p class="text-xs text-gray-400 uppercase tracking-widest text-center mb-4">
                        {move || t(lang.get(), "svc.eyebrow")}
                    </p>
                    <h2 class="text-4xl font-bold text-center mb-16 text-gray-900 dark:text-white">
                        {move || t(lang.get(), "svc.title")}
                    </h2>
                    <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                        <div class="border border-gray-200 dark:border-gray-800 p-8 bg-white dark:bg-black hover:border-red-600 dark:hover:border-red-600 transition-colors rounded-lg">
                            <p class="text-red-600 font-bold text-4xl mb-5 font-mono">"01"</p>
                            <h4 class="text-lg font-bold mb-3 text-gray-900 dark:text-white">
                                {move || t(lang.get(), "svc.01.name")}
                            </h4>
                            <p class="text-gray-500 dark:text-gray-400 text-sm leading-relaxed">
                                {move || t(lang.get(), "svc.01.desc")}
                            </p>
                        </div>
                        <div class="border border-gray-200 dark:border-gray-800 p-8 bg-white dark:bg-black hover:border-red-600 dark:hover:border-red-600 transition-colors rounded-lg">
                            <p class="text-red-600 font-bold text-4xl mb-5 font-mono">"02"</p>
                            <h4 class="text-lg font-bold mb-3 text-gray-900 dark:text-white">
                                {move || t(lang.get(), "svc.02.name")}
                            </h4>
                            <p class="text-gray-500 dark:text-gray-400 text-sm leading-relaxed">
                                {move || t(lang.get(), "svc.02.desc")}
                            </p>
                        </div>
                        <div class="border border-gray-200 dark:border-gray-800 p-8 bg-white dark:bg-black hover:border-red-600 dark:hover:border-red-600 transition-colors rounded-lg">
                            <p class="text-red-600 font-bold text-4xl mb-5 font-mono">"03"</p>
                            <h4 class="text-lg font-bold mb-3 text-gray-900 dark:text-white">
                                {move || t(lang.get(), "svc.03.name")}
                            </h4>
                            <p class="text-gray-500 dark:text-gray-400 text-sm leading-relaxed">
                                {move || t(lang.get(), "svc.03.desc")}
                            </p>
                        </div>
                    </div>
                </div>
            </div>

            {/* Solutions gallery */}
            <div id="gallery" class="bg-white dark:bg-black py-24 border-t border-gray-100 dark:border-gray-900 transition-colors duration-300">
                <div class="max-w-7xl mx-auto px-6">
                    <p class="text-xs text-gray-400 uppercase tracking-widest text-center mb-4">
                        {move || t(lang.get(), "gal.eyebrow")}
                    </p>
                    <h2 class="text-4xl font-bold text-center mb-4 text-gray-900 dark:text-white">
                        {move || t(lang.get(), "gal.title")}
                    </h2>
                    <p class="text-center text-gray-500 dark:text-gray-400 mb-16 max-w-xl mx-auto font-light">
                        {move || t(lang.get(), "gal.sub")}
                    </p>
                    <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                        <div class="p-7 bg-gray-50 dark:bg-gray-950 border border-gray-100 dark:border-gray-900 rounded-lg hover:border-red-600 hover:shadow-lg transition-all cursor-pointer">
                            <p class="text-xs text-red-600 font-semibold uppercase tracking-wider mb-3">
                                {move || t(lang.get(), "gal.01.tag")}
                            </p>
                            <h3 class="font-bold text-lg mb-2 text-gray-900 dark:text-white">
                                {move || t(lang.get(), "gal.01.name")}
                            </h3>
                            <p class="text-gray-500 dark:text-gray-400 text-sm leading-relaxed">
                                {move || t(lang.get(), "gal.01.desc")}
                            </p>
                            <button on:click=move |_| c1() class="mt-5 text-sm text-red-600 font-semibold hover:text-red-700 transition-colors">
                                {move || t(lang.get(), "gal.cta")}
                            </button>
                        </div>
                        <div class="p-7 bg-gray-50 dark:bg-gray-950 border border-gray-100 dark:border-gray-900 rounded-lg hover:border-red-600 hover:shadow-lg transition-all cursor-pointer">
                            <p class="text-xs text-red-600 font-semibold uppercase tracking-wider mb-3">
                                {move || t(lang.get(), "gal.02.tag")}
                            </p>
                            <h3 class="font-bold text-lg mb-2 text-gray-900 dark:text-white">
                                {move || t(lang.get(), "gal.02.name")}
                            </h3>
                            <p class="text-gray-500 dark:text-gray-400 text-sm leading-relaxed">
                                {move || t(lang.get(), "gal.02.desc")}
                            </p>
                            <button on:click=move |_| c2() class="mt-5 text-sm text-red-600 font-semibold hover:text-red-700 transition-colors">
                                {move || t(lang.get(), "gal.cta")}
                            </button>
                        </div>
                        <div class="p-7 bg-gray-50 dark:bg-gray-950 border border-gray-100 dark:border-gray-900 rounded-lg hover:border-red-600 hover:shadow-lg transition-all cursor-pointer">
                            <p class="text-xs text-red-600 font-semibold uppercase tracking-wider mb-3">
                                {move || t(lang.get(), "gal.03.tag")}
                            </p>
                            <h3 class="font-bold text-lg mb-2 text-gray-900 dark:text-white">
                                {move || t(lang.get(), "gal.03.name")}
                            </h3>
                            <p class="text-gray-500 dark:text-gray-400 text-sm leading-relaxed">
                                {move || t(lang.get(), "gal.03.desc")}
                            </p>
                            <button on:click=move |_| c3() class="mt-5 text-sm text-red-600 font-semibold hover:text-red-700 transition-colors">
                                {move || t(lang.get(), "gal.cta")}
                            </button>
                        </div>
                    </div>
                </div>
            </div>

        </div>
    }
}
