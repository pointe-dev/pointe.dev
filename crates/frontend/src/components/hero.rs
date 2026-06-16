use leptos::*;
use crate::i18n::{Lang, t};

#[component]
pub fn Hero(on_chat_click: impl Fn() + 'static) -> impl IntoView {
    let lang = use_context::<RwSignal<Lang>>()
        .unwrap_or_else(|| create_rw_signal(Lang::Fr));

    view! {
        <section class="hero-ambient min-h-screen relative flex flex-col justify-center items-center text-center py-20 px-6 overflow-hidden bg-deep">

            {/* Ballet silhouettes — red ambient blurs */}
            <div class="absolute inset-0 pointer-events-none select-none overflow-hidden">
                <div
                    class="absolute bottom-0 origin-bottom animate-pointe-rise"
                    style="left: 8%; width: 92px; height: 580px; \
                           animation-duration: 14s; animation-delay: -3s; \
                           --da: 12px; --db: -6px; --dc: -14px; --rt: 1.2deg; --lift-y: -70px; \
                           background: rgba(220, 38, 38, 0.14); \
                           filter: blur(38px); \
                           border-radius: 48% 52% 44% 56% / 55% 45% 60% 40%;"
                ></div>
                <div
                    class="absolute bottom-0 origin-bottom animate-arabesque"
                    style="left: 26%; width: 68px; height: 420px; \
                           animation-duration: 11s; animation-delay: -7s; \
                           --da: 28px; --db: -16px; --dc: 18px; \
                           background: rgba(0, 229, 204, 0.07); \
                           filter: blur(44px); \
                           border-radius: 42% 58% 52% 48% / 50% 52% 48% 50%;"
                ></div>
                <div
                    class="absolute bottom-0 origin-bottom animate-pointe-rise"
                    style="left: 44%; width: 108px; height: 680px; \
                           animation-duration: 10s; animation-delay: -1s; \
                           --da: 8px; --db: -5px; --dc: 10px; --rt: 0.5deg; --lift-y: -85px; \
                           background: rgba(220, 38, 38, 0.11); \
                           filter: blur(48px); \
                           border-radius: 46% 54% 50% 50% / 52% 48% 54% 46%;"
                ></div>
                <div
                    class="absolute bottom-0 origin-bottom animate-arabesque"
                    style="left: 62%; width: 76px; height: 450px; \
                           animation-duration: 13s; animation-delay: -5s; \
                           --da: -22px; --db: 14px; --dc: -16px; \
                           background: rgba(220, 38, 38, 0.09); \
                           filter: blur(48px); \
                           border-radius: 54% 46% 48% 52% / 46% 54% 46% 54%;"
                ></div>
                <div
                    class="absolute bottom-0 origin-bottom animate-pointe-rise"
                    style="right: 8%; width: 88px; height: 540px; \
                           animation-duration: 12s; animation-delay: -9s; \
                           --da: -14px; --db: 8px; --dc: -20px; --rt: -1.0deg; --lift-y: -62px; \
                           background: rgba(0, 229, 204, 0.06); \
                           filter: blur(42px); \
                           border-radius: 52% 48% 56% 44% / 42% 58% 42% 58%;"
                ></div>
            </div>

            {/* Content */}
            <div class="relative z-10 max-w-5xl w-full">

                {/* Eyebrow badge */}
                <div class="flex justify-center mb-10 animate-fade-up">
                    <span class="inline-flex items-center gap-2 px-4 py-2 glass-red text-red-400 text-xs font-semibold tracking-widest uppercase rounded-full">
                        <span class="w-2 h-2 rounded-full bg-red-500 animate-pulse inline-block"></span>
                        {move || t(lang.get(), "hero.badge")}
                    </span>
                </div>

                {/* Headline */}
                <h1 class="font-bold tracking-tight leading-none mb-4 animate-fade-up stagger-1">
                    <span class="text-5xl md:text-7xl text-gradient block">
                        {move || t(lang.get(), "hero.line1")}
                    </span>
                    <span
                        class="text-5xl md:text-7xl relative block overflow-hidden"
                        style="height: 1.15em; margin-top: 0.05em; color: var(--red-bright); -webkit-text-fill-color: var(--red-bright);"
                    >
                        <span class="word-slot word-slot-1">{move || t(lang.get(), "hero.w1")}</span>
                        <span class="word-slot word-slot-2">{move || t(lang.get(), "hero.w2")}</span>
                        <span class="word-slot word-slot-3">{move || t(lang.get(), "hero.w3")}</span>
                        <span class="word-slot word-slot-4">{move || t(lang.get(), "hero.w4")}</span>
                        <span class="word-slot word-slot-5">{move || t(lang.get(), "hero.w5")}</span>
                    </span>
                </h1>

                {/* Sub-copy */}
                <p class="text-xl md:text-2xl text-secondary mt-8 max-w-2xl mx-auto font-light leading-relaxed animate-fade-up stagger-2">
                    {move || t(lang.get(), "hero.sub")}
                </p>

                {/* Use-case ticker */}
                <div class="hero-use-case-strip animate-fade-up stagger-2">
                    <span class="hero-use-case-label">
                        {move || t(lang.get(), "hero.cases.label")}
                    </span>
                    <div class="hero-use-case-track-wrap">
                        <div class="hero-use-case-track">
                            <span class="hero-use-case-pill">
                                <span class="hero-use-case-dot"></span>
                                {move || t(lang.get(), "hero.case1.name")}
                                <span class="hero-use-case-time">{move || t(lang.get(), "hero.case1.time")}</span>
                            </span>
                            <span class="hero-use-case-pill">
                                <span class="hero-use-case-dot"></span>
                                {move || t(lang.get(), "hero.case2.name")}
                                <span class="hero-use-case-time">{move || t(lang.get(), "hero.case2.time")}</span>
                            </span>
                            <span class="hero-use-case-pill">
                                <span class="hero-use-case-dot"></span>
                                {move || t(lang.get(), "hero.case3.name")}
                                <span class="hero-use-case-time">{move || t(lang.get(), "hero.case3.time")}</span>
                            </span>
                            <span class="hero-use-case-pill">
                                <span class="hero-use-case-dot"></span>
                                {move || t(lang.get(), "hero.case4.name")}
                                <span class="hero-use-case-time">{move || t(lang.get(), "hero.case4.time")}</span>
                            </span>
                            <span class="hero-use-case-pill">
                                <span class="hero-use-case-dot"></span>
                                {move || t(lang.get(), "hero.case5.name")}
                                <span class="hero-use-case-time">{move || t(lang.get(), "hero.case5.time")}</span>
                            </span>
                            {/* duplicate for seamless loop */}
                            <span class="hero-use-case-pill">
                                <span class="hero-use-case-dot"></span>
                                {move || t(lang.get(), "hero.case1.name")}
                                <span class="hero-use-case-time">{move || t(lang.get(), "hero.case1.time")}</span>
                            </span>
                            <span class="hero-use-case-pill">
                                <span class="hero-use-case-dot"></span>
                                {move || t(lang.get(), "hero.case2.name")}
                                <span class="hero-use-case-time">{move || t(lang.get(), "hero.case2.time")}</span>
                            </span>
                            <span class="hero-use-case-pill">
                                <span class="hero-use-case-dot"></span>
                                {move || t(lang.get(), "hero.case3.name")}
                                <span class="hero-use-case-time">{move || t(lang.get(), "hero.case3.time")}</span>
                            </span>
                            <span class="hero-use-case-pill">
                                <span class="hero-use-case-dot"></span>
                                {move || t(lang.get(), "hero.case4.name")}
                                <span class="hero-use-case-time">{move || t(lang.get(), "hero.case4.time")}</span>
                            </span>
                            <span class="hero-use-case-pill">
                                <span class="hero-use-case-dot"></span>
                                {move || t(lang.get(), "hero.case5.name")}
                                <span class="hero-use-case-time">{move || t(lang.get(), "hero.case5.time")}</span>
                            </span>
                        </div>
                    </div>
                </div>

                {/* CTA */}
                <div class="flex flex-col items-center gap-3 animate-fade-up stagger-3">
                    <button
                        on:click=move |_| (on_chat_click)()
                        class="btn-primary w-full max-w-xs"
                    >
                        {move || t(lang.get(), "hero.cta")}
                    </button>
                    {/* Demo-proof microcopy — the moat competitors can't show */}
                    <p class="text-sm text-muted max-w-md mx-auto flex items-center gap-2 justify-center">
                        <span class="text-cyan">"✓"</span>
                        {move || t(lang.get(), "hero.proof")}
                    </p>
                </div>
            </div>
        </section>
    }
}
