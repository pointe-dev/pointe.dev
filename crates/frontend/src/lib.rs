use leptos::*;
use log::info;

mod components;
mod pages;
pub mod i18n;

use components::layout::Layout;
use components::theme::ThemeProvider;

#[component]
pub fn App() -> impl IntoView {
    info!("🔄 App component rendering...");

    view! {
        <ThemeProvider>
            <Layout />
        </ThemeProvider>
    }
}

/// Entry point for WASM — mounts the Leptos app to the DOM in CSR mode
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn main() {
    _ = console_log::init_with_level(log::Level::Debug);
    info!("🚀 Leptos WASM initializing...");
    
    leptos::mount_to_body(|| {
        info!("📦 Mounting App component...");
        App()
    });
    
    info!("✅ Leptos mounted!");
}
