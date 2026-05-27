use leptos::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

#[component]
pub fn SliderInput(
    position: ReadSignal<f32>,
    set_position: WriteSignal<f32>,
) -> impl IntoView {
    let (is_dragging, set_is_dragging) = create_signal(false);

    let handle_input = move |ev: web_sys::Event| {
        if let Some(input) = ev.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok()) {
            if let Ok(value) = input.value().parse::<f32>() {
                set_position.set(value);
            }
        }
    };

    view! {
        <div class="absolute inset-0">
            // Invisible range — captures all mouse/touch interaction
            <input
                type="range"
                min="0"
                max="100"
                step="0.5"
                prop:value=move || position.get().to_string()
                on:input=handle_input
                on:mousedown=move |_: web_sys::MouseEvent| set_is_dragging.set(true)
                on:touchstart=move |_: web_sys::TouchEvent| set_is_dragging.set(true)
                on:mouseup=move |_: web_sys::MouseEvent| set_is_dragging.set(false)
                on:touchend=move |_: web_sys::TouchEvent| set_is_dragging.set(false)
                class="absolute inset-0 w-full h-full opacity-0 cursor-col-resize z-20"
            />

            // Divider line — vertical, follows position
            <div
                class="absolute top-0 bottom-0 w-px bg-red-600/50 pointer-events-none z-10"
                style=move || format!("left: {}%", position.get())
            ></div>

            // Knob — centered on the divider line
            <div
                class="absolute top-1/2 pointer-events-none z-10"
                style=move || format!(
                    "left: calc({}% - 14px); transform: translateY(-50%);",
                    position.get()
                )
            >
                <div
                    class="w-7 h-7 rounded-full bg-white dark:bg-black border-2 border-red-600 flex items-center justify-center transition-shadow duration-150"
                    style=move || {
                        let (a, b) = if is_dragging.get() { (28, 0.9) } else { (16, 0.55) };
                        format!("box-shadow: 0 0 {}px rgba(211,47,47,{:.2}), 0 0 {}px rgba(211,47,47,{:.2});",
                            a, b, a * 2, b * 0.4)
                    }
                >
                    <span
                        class="text-red-600 font-bold select-none"
                        style="font-size: 9px; letter-spacing: -1px;"
                    >"⟷"</span>
                </div>
            </div>

            // Side labels — fade with position
            <div
                class="absolute left-4 top-4 text-xs font-medium uppercase tracking-widest pointer-events-none z-10"
                style=move || format!("opacity: {:.2};", 1.0 - position.get() / 100.0)
            >
                <span class="text-gray-400 dark:text-gray-600">"The Art"</span>
            </div>
            <div
                class="absolute right-4 top-4 text-xs font-medium uppercase tracking-widest pointer-events-none z-10"
                style=move || format!("opacity: {:.2};", position.get() / 100.0)
            >
                <span class="text-gray-500">"The Technique"</span>
            </div>
        </div>
    }
}
