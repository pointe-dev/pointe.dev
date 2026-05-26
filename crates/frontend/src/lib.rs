use leptos::*;
use leptos_router::*;

mod components;
mod pages;

use components::layout::Layout;
use pages::{home::Home, not_found::NotFound};

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Layout>
            <Router>
                <Routes>
                    <Route path="/" view=Home />
                    <Route path="/*" view=NotFound />
                </Routes>
            </Router>
        </Layout>
    }
}

/// Entry point for WASM — mounts the Leptos app to the DOM in CSR mode
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn main() {
    _ = console_log::init_with_level(log::Level::Debug);
    console_log::log!("🚀 Leptos WASM initializing...");
    
    leptos::mount_to_body(|| {
        console_log::log!("📦 Mounting App component...");
        view! { <App /> }
    });
    
    console_log::log!("✅ Leptos mounted!");
}
