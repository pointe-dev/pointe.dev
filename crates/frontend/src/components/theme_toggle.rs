use leptos::*;
use crate::components::theme::{use_theme, Theme};

#[component]
pub fn ThemeToggle() -> impl IntoView {
    let theme_ctx = use_theme();
    
    let on_toggle = move |_| {
        let current = theme_ctx.theme.get();
        let next = match current {
            Theme::Light => Theme::Dark,
            Theme::Dark => Theme::Light,
        };
        theme_ctx.set_theme.set(next);
    };
    
    let theme_label = move || {
        match theme_ctx.theme.get() {
            Theme::Light => "🌙",
            Theme::Dark => "☀️",
        }
    };
    
    view! {
        <button
            on:click=on_toggle
            class="p-2 rounded-lg hover:bg-gray-200 dark:hover:bg-gray-800 transition-colors"
            title="Toggle theme"
        >
            {theme_label}
        </button>
    }
}
