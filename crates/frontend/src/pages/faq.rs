//! Client-facing FAQ / docs page (trilingue FR/EN/DE).
//!
//! Plain, non-technical answers: what we deliver, how to connect accounts (the
//! "Authorize" OAuth flow), security ("we never see your tokens"), pricing. All copy
//! lives in `i18n` under `faq.*`; this component is pure layout. Accordions use native
//! `<details>`/`<summary>` so they work without JS and stay keyboard-accessible.

use leptos::*;

use crate::i18n::{t, Lang};

/// One question/answer pair, keyed by its i18n suffix (e.g. ("faq.q1", "faq.a1")).
/// Reactive on the shared `lang` signal so the language switcher re-renders the copy.
fn qa(lang: RwSignal<Lang>, q_key: &'static str, a_key: &'static str) -> impl IntoView {
    view! {
        <details class="group border-t border-subtle py-4 first:border-t-0">
            <summary class="flex items-center justify-between gap-4 cursor-pointer list-none text-sm font-medium text-primary marker:hidden">
                <span>{move || t(lang.get(), q_key)}</span>
                <span class="text-muted text-lg leading-none transition-transform group-open:rotate-45 shrink-0">"+"</span>
            </summary>
            <p class="mt-3 text-sm text-secondary leading-relaxed">
                {move || t(lang.get(), a_key)}
            </p>
        </details>
    }
}

/// A titled group of Q/A pairs.
fn section(lang: RwSignal<Lang>, title_key: &'static str, qas: Vec<(&'static str, &'static str)>) -> impl IntoView {
    view! {
        <div class="mb-10">
            <h2 class="text-xs font-semibold uppercase tracking-wider text-red-400 mb-3">
                {move || t(lang.get(), title_key)}
            </h2>
            <div class="glass rounded-2xl px-5 py-2">
                {qas.into_iter().map(|(q, a)| qa(lang, q, a).into_view()).collect_view()}
            </div>
        </div>
    }
}

#[component]
pub fn Faq(#[prop(into)] on_talk: Callback<()>) -> impl IntoView {
    // Language comes from the Layout-provided context, like every other page.
    let lang = use_context::<RwSignal<Lang>>()
        .unwrap_or_else(|| create_rw_signal(Lang::Fr));

    view! {
        <section class="max-w-3xl mx-auto px-6 py-16 md:py-24">
            {/* Header */}
            <div class="text-center mb-12">
                <p class="text-xs font-semibold uppercase tracking-wider text-muted mb-3">
                    {move || t(lang.get(), "faq.eyebrow")}
                </p>
                <h1 class="text-3xl md:text-4xl font-bold text-gradient mb-4">
                    {move || t(lang.get(), "faq.title")}
                </h1>
                <p class="text-secondary leading-relaxed">
                    {move || t(lang.get(), "faq.sub")}
                </p>
            </div>

            {/* Sections */}
            {section(lang, "faq.sec.start", vec![
                ("faq.q1", "faq.a1"),
                ("faq.q2", "faq.a2"),
                ("faq.q3", "faq.a3"),
            ])}
            {section(lang, "faq.sec.connect", vec![
                ("faq.q4", "faq.a4"),
                ("faq.q5", "faq.a5"),
                ("faq.q6", "faq.a6"),
            ])}
            {section(lang, "faq.sec.security", vec![
                ("faq.q7", "faq.a7"),
                ("faq.q8", "faq.a8"),
                ("faq.q9", "faq.a9"),
            ])}
            {section(lang, "faq.sec.billing", vec![
                ("faq.q10", "faq.a10"),
                ("faq.q11", "faq.a11"),
                ("faq.q12", "faq.a12"),
            ])}

            {/* Bottom CTA */}
            <div class="text-center mt-14 pt-10 border-t border-subtle">
                <p class="text-secondary mb-4">{move || t(lang.get(), "faq.cta.q")}</p>
                <button
                    on:click=move |_| on_talk.call(())
                    class="btn-primary btn-sm"
                >
                    {move || t(lang.get(), "faq.cta.btn")}
                </button>
            </div>
        </section>
    }
}
