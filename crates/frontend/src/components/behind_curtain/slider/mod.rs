//! Slider input component.

use leptos::*;
use wasm_bindgen::JsCast;

/// Interactive slider input for the Behind the Curtain component.
#[component]
pub fn SliderInput(
    position: ReadSignal<f32>,
    set_position: WriteSignal<f32>,
) -> impl IntoView {
    let handle_input = move |ev: web_sys::Event| {
        if let Some(input) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()) {
            if let Ok(value) = input.value().parse::<f32>() {
                set_position.set(value);
            }
        }
    };

    view! {
        <div class="absolute inset-0 flex items-center justify-center pointer-events-none">
            <input
                type="range"
                min="0"
                max="100"
                step="1"
                value=move || position.get().to_string()
                on:input=handle_input
                class="absolute w-full h-full opacity-0 cursor-col-resize pointer-events-auto z-10"
            />

            <div class="absolute inset-0 flex items-center h-full w-full pointer-events-none">
                <div class="w-full h-1 bg-gray-700" />
                <div
                    class="absolute h-1 bg-red-600"
                    style:width=move || format!("{}%", position.get())
                />
            </div>

            <div
                class="absolute top-1/2 -translate-y-1/2 w-6 h-6 bg-red-600 rounded-full shadow-lg border-2 border-red-400 pointer-events-none z-20"
                style:left=move || format!("{}%", position.get())
                style:transform="translateX(-50%)"
            />
        </div>
    }
}
