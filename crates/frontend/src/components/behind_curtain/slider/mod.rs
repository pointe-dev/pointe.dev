use leptos::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;
use crate::i18n::{Lang, t};

#[component]
pub fn SliderInput(
    position: ReadSignal<f32>,
    set_position: WriteSignal<f32>,
) -> impl IntoView {
    let lang = use_context::<RwSignal<Lang>>()
        .unwrap_or_else(|| create_rw_signal(Lang::Fr));

    let handle_input = move |ev: web_sys::Event| {
        if let Some(input) = ev.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok()) {
            if let Ok(value) = input.value().parse::<f32>() {
                set_position.set(value);
            }
        }
    };

    view! {
        <div class="absolute inset-0 pointer-events-none">
            {/* Range input — captures touch interaction; mouse handled by container mousemove */}
            <input
                type="range"
                min="0"
                max="100"
                step="0.5"
                prop:value=move || position.get().to_string()
                on:input=handle_input
                class="absolute inset-0 w-full h-full opacity-0 cursor-col-resize z-20 pointer-events-auto"
            />

            {/* Divider line */}
            <div
                class="absolute top-0 bottom-0 w-px bg-red-600/50 pointer-events-none z-10"
                style=move || format!("left: {}%", position.get())
            ></div>

            {/* Knob */}
            <div
                class="absolute top-1/2 pointer-events-none z-10"
                style=move || format!(
                    "left: calc({}% - 14px); transform: translateY(-50%);",
                    position.get()
                )
            >
                <div
                    class="w-7 h-7 rounded-full bg-white dark:bg-black border-2 border-red-600 flex items-center justify-center"
                    style="box-shadow: 0 0 16px rgba(211,47,47,0.55), 0 0 32px rgba(211,47,47,0.22);"
                >
                    <span
                        class="text-red-600 font-bold select-none"
                        style="font-size: 9px; letter-spacing: -1px;"
                    >"⟷"</span>
                </div>
            </div>

            {/* Side labels */}
            <div
                class="absolute left-4 top-4 text-xs font-medium uppercase tracking-widest pointer-events-none z-10"
                style=move || format!("opacity: {:.2};", 1.0 - position.get() / 100.0)
            >
                <span class="text-gray-400 dark:text-gray-600">{move || t(lang.get(), "curtain.art")}</span>
            </div>
            <div
                class="absolute right-4 top-4 text-xs font-medium uppercase tracking-widest pointer-events-none z-10"
                style=move || format!("opacity: {:.2};", position.get() / 100.0)
            >
                <span class="text-gray-500">{move || t(lang.get(), "curtain.tech")}</span>
            </div>
        </div>
    }
}
