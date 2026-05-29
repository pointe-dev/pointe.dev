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
                    <p class="section-sub text-center mb-16 max-w-xl mx-auto">
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

            {/* ── CONSOLE / MCP SECTION ─────────────────────────── */}
            <section class="bg-surface border-t border-subtle py-20 px-6">
                <div class="max-w-6xl mx-auto grid grid-cols-1 lg:grid-cols-2 gap-12 items-center">

                    {/* Text side */}
                    <div>
                        <p class="eyebrow mb-3">
                            {move || t(lang.get(), "console.eyebrow")}
                        </p>
                        <h2 class="section-title mb-4">
                            {move || t(lang.get(), "console.title")}
                        </h2>
                        <p class="section-sub mb-6 leading-relaxed">
                            {move || t(lang.get(), "console.sub")}
                        </p>
                        <ul class="space-y-3">
                            <li class="flex items-start gap-3 text-sm text-secondary">
                                <span class="text-cyan mt-0.5 shrink-0">"▸"</span>
                                {move || t(lang.get(), "console.feat1")}
                            </li>
                            <li class="flex items-start gap-3 text-sm text-secondary">
                                <span class="text-cyan mt-0.5 shrink-0">"▸"</span>
                                {move || t(lang.get(), "console.feat2")}
                            </li>
                            <li class="flex items-start gap-3 text-sm text-secondary">
                                <span class="text-cyan mt-0.5 shrink-0">"▸"</span>
                                {move || t(lang.get(), "console.feat3")}
                            </li>
                        </ul>
                    </div>

                    {/* Console terminal */}
                    <div class="console-window">
                        <div class="console-bar">
                            <span class="console-dot console-dot-red"></span>
                            <span class="console-dot console-dot-yellow"></span>
                            <span class="console-dot console-dot-green"></span>
                            <span class="ml-2 text-xs text-muted font-mono">"pointe.dev — MCP"</span>
                        </div>
                        <div class="p-5 font-mono text-xs leading-relaxed space-y-1">
                            <div class="c-line">
                                <span class="c-prompt">"❯"</span>
                                <span class="c-cmd">" qualify"</span>
                                <span class="c-arg">" --sector=ecommerce --pain=manual-orders"</span>
                            </div>
                            <div class="c-line">
                                <span class="c-ok">"✓"</span>
                                <span class="c-detail">" Prospect qualifié — démarrage pipeline"</span>
                            </div>
                            <div class="c-line">
                                <span class="c-stage">"[research]"</span>
                                <span class="c-detail">" Analyse des outils existants…"</span>
                            </div>
                            <div class="c-line">
                                <span class="c-stage">"[build]"</span>
                                <span class="c-detail">" Génération workflow n8n…"</span>
                            </div>
                            <div class="c-line">
                                <span class="c-ok">"✓"</span>
                                <span class="c-detail">" 7 nœuds, 3 intégrations (Shopify → Slack → ERP)"</span>
                            </div>
                            <div class="c-line">
                                <span class="c-stage">"[deploy]"</span>
                                <span class="c-url">" POST n8n.pointe.dev/api/v1/workflows"</span>
                            </div>
                            <div class="c-line">
                                <span class="c-ok">"✓"</span>
                                <span class="c-live">" Workflow actif — 200 OK"</span>
                            </div>
                            <div class="c-line">
                                <span class="c-prompt">"❯"</span>
                                <span class="c-cmd">" _"</span>
                                <span class="c-cursor"></span>
                            </div>
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
                    <h2 class="section-title text-center mb-16">
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
                <div class="max-w-6xl mx-auto mb-12">
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
