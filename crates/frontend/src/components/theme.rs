use leptos::*;
use log::info;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Theme {
    Light,
    Dark,
}

#[component]
pub fn ThemeProvider(children: Children) -> impl IntoView {
    let (theme, set_theme) = create_signal(Theme::Dark);
    
    // Initialize theme on mount - check system preference
    create_effect(move |_| {
        // Try to detect system preference via CSS media query simulation
        // For now, default to dark mode (brand standard)
        set_theme.set(Theme::Dark);
        apply_theme(Theme::Dark);
    });
    
    // Apply theme when it changes
    create_effect(move |_| {
        apply_theme(theme.get());
    });
    
    provide_context(ThemeContext {
        theme,
        set_theme,
    });
    
    children()
}

#[derive(Clone, Copy)]
pub struct ThemeContext {
    pub theme: ReadSignal<Theme>,
    pub set_theme: WriteSignal<Theme>,
}

pub fn use_theme() -> ThemeContext {
    use_context().expect("ThemeContext not provided")
}

fn apply_theme(theme: Theme) {
    if let Ok(window) = web_sys::window().ok_or("no window") {
        if let Some(doc) = window.document() {
            if let Some(html) = doc.document_element() {
                match theme {
                    Theme::Dark => {
                        let _ = html.class_list().add_1("dark");
                    }
                    Theme::Light => {
                        let _ = html.class_list().remove_1("dark");
                    }
                }
                
                info!("Theme applied: {:?}", theme);
            }
        }
    }
}
