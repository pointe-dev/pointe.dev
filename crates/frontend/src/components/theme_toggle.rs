use leptos::*;
use crate::components::theme::{use_theme, Theme};

#[component]
pub fn ThemeToggle() -> impl IntoView {
    let theme_ctx = use_theme();

    let on_toggle = move |_| {
        let next = match theme_ctx.theme.get() {
            Theme::Light => Theme::Dark,
            Theme::Dark  => Theme::Light,
        };
        theme_ctx.set_theme.set(next);
    };

    view! {
        <button
            on:click=on_toggle
            class="theme-icon-btn"
            aria-label=move || match theme_ctx.theme.get() {
                Theme::Dark  => "Passer en mode clair",
                Theme::Light => "Passer en mode sombre",
            }
        >
            {move || match theme_ctx.theme.get() {
                // Dark mode → show sun (click to go light)
                Theme::Dark => view! {
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <circle cx="12" cy="12" r="4"/>
                        <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M6.34 17.66l-1.41 1.41M19.07 4.93l-1.41 1.41"/>
                    </svg>
                }.into_view(),
                // Light mode → show moon (click to go dark)
                Theme::Light => view! {
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                        <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
                    </svg>
                }.into_view(),
            }}
        </button>
    }
}
