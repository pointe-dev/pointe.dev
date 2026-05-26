use leptos::*;
use leptos_meta::*;
use leptos_router::*;

mod components;
mod pages;

use components::layout::Layout;
use pages::{home::Home, not_found::NotFound};

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="leptos" href="/pkg/frontend.css" />
        <Html class="scroll-smooth" />
        <Title text="pointe.dev | AI Product Commercialization" />
        <Meta name="description" content="High-end AI agency turning complex engineering into effortless elegance." />
        <Meta name="charset" attr:charset="utf-8" />
        <Meta name="viewport" attr:content="width=device-width, initial-scale=1" />

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

/// Entry point for WASM — mounts the Leptos app to the DOM
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn main() {
    _ = console_log::init_with_level(log::Level::Debug);
    leptos::mount_to_body(|| view! { <App /> });
}
