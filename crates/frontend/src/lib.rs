use leptos::*;
use log::info;

mod components;
mod pages;

use components::layout::Layout;
use pages::home::Home;

#[component]
pub fn App() -> impl IntoView {
    info!("🔄 App component rendering...");
    
    view! {
        <Layout>
            <Home />
        </Layout>
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
