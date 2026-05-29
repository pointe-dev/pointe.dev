use leptos::*;
use crate::components::hero::Hero;
use crate::i18n::{Lang, t};

#[component]
pub fn Home(on_chat_click: impl Fn() + Clone + 'static) -> impl IntoView {
    log::info!("🏠 Home page rendering...");

    let lang = use_context::<RwSignal<Lang>>()
        .unwrap_or_else(|| create_rw_signal(Lang::Fr));

    let c_hero = on_chat_click.clone();
    let c_cta  = on_chat_click.clone();
    let c1     = on_chat_click.clone();
    let c2     = on_chat_click.clone();
    let c3     = on_chat_click.clone();

    view! {
        <div class="bg-deep text-primary">

            <Hero on_chat_click=move || c_hero() />

            {/* ── PROCESS STRIP ─────────────────────────────────── */}
            <section class="section-deep border-t border-subtle py-20 px-6">
                <div class="max-w-6xl mx-auto">
                    <p class="eyebrow text-center mb-3">
                        {move || t(lang.get(), "process.eyebrow")}
                    </p>
                    <h2 class="section-title text-center mb-4">
                        {move || t(lang.get(), "process.title")}
                    </h2>
                    <p class="section-sub text-center mb-20 max-w-xl mx-auto">
                        {move || t(lang.get(), "process.sub")}
                    </p>

                    <div class="flex flex-col md:flex-row items-start md:items-center justify-between gap-6 md:gap-2">

                        {/* Step 1 */}
                        <div class="process-step">
                            <div class="process-icon process-icon-red">
                                <svg class="w-5 h-5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z"/>
                                </svg>
                            </div>
                            <p class="process-label">"01"</p>
                            <p class="process-name">
                                {move || t(lang.get(), "process.01.name")}
                            </p>
                            <p class="process-desc">
                                {move || t(lang.get(), "process.01.desc")}
                            </p>
                        </div>

                        <div class="process-connector"></div>

                        {/* Step 2 */}
                        <div class="process-step">
                            <div class="process-icon process-icon-cyan">
                                <svg class="w-5 h-5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z"/>
                                </svg>
                            </div>
                            <p class="process-label">"02"</p>
                            <p class="process-name">
                                {move || t(lang.get(), "process.02.name")}
                            </p>
                            <p class="process-desc">
                                {move || t(lang.get(), "process.02.desc")}
                            </p>
                        </div>

                        <div class="process-connector"></div>

                        {/* Step 3 */}
                        <div class="process-step">
                            <div class="process-icon process-icon-red">
                                <svg class="w-5 h-5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"/>
                                </svg>
                            </div>
                            <p class="process-label">"03"</p>
                            <p class="process-name">
                                {move || t(lang.get(), "process.03.name")}
                            </p>
                            <p class="process-desc">
                                {move || t(lang.get(), "process.03.desc")}
                            </p>
                        </div>

                        <div class="process-connector"></div>

                        {/* Step 4 */}
                        <div class="process-step">
                            <div class="process-icon process-icon-cyan">
                                <svg class="w-5 h-5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z"/>
                                </svg>
                            </div>
                            <p class="process-label">"04"</p>
                            <p class="process-name">
                                {move || t(lang.get(), "process.04.name")}
                            </p>
                            <p class="process-desc">
                                {move || t(lang.get(), "process.04.desc")}
                            </p>
                        </div>

                        <div class="process-connector"></div>

                        {/* Step 5 */}
                        <div class="process-step">
                            <div class="process-icon process-icon-red">
                                <svg class="w-5 h-5" fill="none" stroke="currentColor" stroke-width="2" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M5 3v4M3 5h4M6 17v4m-2-2h4m5-16l2.286 6.857L21 12l-5.714 2.143L13 21l-2.286-6.857L5 12l5.714-2.143L13 3z"/>
                                </svg>
                            </div>
                            <p class="process-label">"05"</p>
                            <p class="process-name">
                                {move || t(lang.get(), "process.05.name")}
                            </p>
                            <p class="process-desc">
                                {move || t(lang.get(), "process.05.desc")}
                            </p>
                        </div>
                    </div>
                </div>
            </section>

            {/* ── APPROCHE B2B ──────────────────────────────────── */}
            <section class="bg-surface border-t border-subtle py-20 px-6">
                <div class="max-w-6xl mx-auto">
                    <p class="eyebrow text-center mb-3">
                        {move || t(lang.get(), "approach.eyebrow")}
                    </p>
                    <h2 class="section-title text-center mb-4">
                        {move || t(lang.get(), "approach.title")}
                    </h2>
                    <p class="section-sub text-center mb-20">
                        {move || t(lang.get(), "approach.sub")}
                    </p>

                    <div class="grid grid-cols-1 md:grid-cols-3 gap-8">

                        <div class="approach-card">
                            <div class="approach-metric">"72h"</div>
                            <h3 class="approach-name">
                                {move || t(lang.get(), "approach.01.name")}
                            </h3>
                            <p class="approach-desc">
                                {move || t(lang.get(), "approach.01.desc")}
                            </p>
                        </div>

                        <div class="approach-card approach-card-featured">
                            <div class="approach-metric approach-metric-crimson">"0"</div>
                            <h3 class="approach-name">
                                {move || t(lang.get(), "approach.02.name")}
                            </h3>
                            <p class="approach-desc">
                                {move || t(lang.get(), "approach.02.desc")}
                            </p>
                        </div>

                        <div class="approach-card">
                            <div class="approach-metric">"99.9%"</div>
                            <h3 class="approach-name">
                                {move || t(lang.get(), "approach.03.name")}
                            </h3>
                            <p class="approach-desc">
                                {move || t(lang.get(), "approach.03.desc")}
                            </p>
                        </div>

                    </div>
                </div>
            </section>

            {/* ── SERVICES ──────────────────────────────────────── */}
            <section class="section-deep border-t border-subtle py-20 px-6">
                <div class="max-w-6xl mx-auto">
                    <p class="eyebrow text-center mb-3">
                        {move || t(lang.get(), "svc.eyebrow")}
                    </p>
                    <h2 class="section-title text-center mb-20">
                        {move || t(lang.get(), "svc.title")}
                    </h2>
                    <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
                        <div class="svc-card">
                            <p class="svc-num">"01"</p>
                            <h4 class="svc-title">
                                {move || t(lang.get(), "svc.01.name")}
                            </h4>
                            <p class="svc-desc">
                                {move || t(lang.get(), "svc.01.desc")}
                            </p>
                        </div>
                        <div class="svc-card svc-card-cyan">
                            <p class="svc-num svc-num-cyan">"02"</p>
                            <h4 class="svc-title">
                                {move || t(lang.get(), "svc.02.name")}
                            </h4>
                            <p class="svc-desc">
                                {move || t(lang.get(), "svc.02.desc")}
                            </p>
                        </div>
                        <div class="svc-card">
                            <p class="svc-num">"03"</p>
                            <h4 class="svc-title">
                                {move || t(lang.get(), "svc.03.name")}
                            </h4>
                            <p class="svc-desc">
                                {move || t(lang.get(), "svc.03.desc")}
                            </p>
                        </div>
                    </div>
                </div>
            </section>

            {/* ── SOLUTIONS TICKER ──────────────────────────────── */}
            <section id="solutions" class="bg-surface border-t border-subtle py-20 px-6 overflow-hidden">
                <div class="max-w-6xl mx-auto mb-20">
                    <p class="eyebrow text-center mb-3">
                        {move || t(lang.get(), "gal.eyebrow")}
                    </p>
                    <h2 class="section-title text-center mb-4">
                        {move || t(lang.get(), "gal.title")}
                    </h2>
                    <p class="section-sub text-center max-w-xl mx-auto">
                        {move || t(lang.get(), "gal.sub")}
                    </p>
                </div>

                <div class="ticker-track">
                    {/* Row 1 — moves left */}
                    <div class="ticker-inner ticker-left">

                        <div class="ticker-card">
                            <span class="ticker-tag">{move || t(lang.get(), "gal.01.tag")}</span>
                            <p class="ticker-name">{move || t(lang.get(), "gal.01.name")}</p>
                            <p class="ticker-desc">{move || t(lang.get(), "gal.01.desc")}</p>
                            <button on:click=move |_| c1() class="ticker-cta">
                                {move || t(lang.get(), "gal.cta")} " →"
                            </button>
                        </div>

                        <div class="ticker-card ticker-card-cyan">
                            <span class="ticker-tag ticker-tag-cyan">{move || t(lang.get(), "gal.02.tag")}</span>
                            <p class="ticker-name">{move || t(lang.get(), "gal.02.name")}</p>
                            <p class="ticker-desc">{move || t(lang.get(), "gal.02.desc")}</p>
                            <button on:click=move |_| c2() class="ticker-cta">
                                {move || t(lang.get(), "gal.cta")} " →"
                            </button>
                        </div>

                        <div class="ticker-card">
                            <span class="ticker-tag">{move || t(lang.get(), "gal.03.tag")}</span>
                            <p class="ticker-name">{move || t(lang.get(), "gal.03.name")}</p>
                            <p class="ticker-desc">{move || t(lang.get(), "gal.03.desc")}</p>
                            <button on:click=move |_| c3() class="ticker-cta">
                                {move || t(lang.get(), "gal.cta")} " →"
                            </button>
                        </div>

                        {/* Duplicate for seamless loop */}
                        <div class="ticker-card">
                            <span class="ticker-tag">{move || t(lang.get(), "gal.01.tag")}</span>
                            <p class="ticker-name">{move || t(lang.get(), "gal.01.name")}</p>
                            <p class="ticker-desc">{move || t(lang.get(), "gal.01.desc")}</p>
                        </div>
                        <div class="ticker-card ticker-card-cyan">
                            <span class="ticker-tag ticker-tag-cyan">{move || t(lang.get(), "gal.02.tag")}</span>
                            <p class="ticker-name">{move || t(lang.get(), "gal.02.name")}</p>
                            <p class="ticker-desc">{move || t(lang.get(), "gal.02.desc")}</p>
                        </div>
                        <div class="ticker-card">
                            <span class="ticker-tag">{move || t(lang.get(), "gal.03.tag")}</span>
                            <p class="ticker-name">{move || t(lang.get(), "gal.03.name")}</p>
                            <p class="ticker-desc">{move || t(lang.get(), "gal.03.desc")}</p>
                        </div>

                    </div>
                </div>
            </section>

            {/* ── CTA FINALE ────────────────────────────────────── */}
            <section class="cta-section border-t border-subtle py-24 px-6 text-center">
                <div class="max-w-2xl mx-auto">
                    <p class="eyebrow mb-4">
                        {move || t(lang.get(), "cta.eyebrow")}
                    </p>
                    <h2 class="section-title mb-6">
                        {move || t(lang.get(), "cta.title")}
                    </h2>
                    <p class="section-sub mb-10 max-w-lg mx-auto">
                        {move || t(lang.get(), "cta.sub")}
                    </p>
                    <button on:click=move |_| c_cta() class="btn-primary mx-auto">
                        {move || t(lang.get(), "hero.cta")}
                    </button>
                </div>
            </section>

        </div>
    }
}
