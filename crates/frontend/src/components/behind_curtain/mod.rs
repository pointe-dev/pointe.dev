pub mod state;
pub mod panels;
pub mod slider;
pub mod animations;

use leptos::*;
use leptos::ev;
use wasm_bindgen::JsCast;
use panels::{ArtPanel, TechniquePanel};
use slider::SliderInput;
use crate::i18n::{Lang, t};

#[component]
pub fn BehindCurtain() -> impl IntoView {
    let (position, set_position) = create_signal(0.0f32);
    let is_hovering = create_rw_signal(false);
    let lang = use_context::<RwSignal<Lang>>()
        .unwrap_or_else(|| create_rw_signal(Lang::Fr));

    let node_ref = create_node_ref::<leptos::html::Div>();

    // Scroll-driven reveal: reveals as section enters viewport, covers as it exits
    let scroll_handle = window_event_listener(ev::scroll, move |_: web_sys::Event| {
        if is_hovering.get_untracked() { return; }
        let Some(el) = node_ref.get() else { return; };
        let rect = el.get_bounding_client_rect();

        let vh = js_sys::Reflect::get(
            web_sys::window().unwrap().as_ref(),
            &wasm_bindgen::JsValue::from_str("innerHeight"),
        )
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(800.0) as f32;

        let top = rect.top() as f32;
        let bottom = rect.bottom() as f32;

        // Bell-curve: 0 when outside viewport, peaks when centered
        let progress_in = ((vh - top) / vh).clamp(0.0, 1.0);
        let progress_out = (bottom / vh).clamp(0.0, 1.0);
        let progress = progress_in.min(progress_out);

        set_position.set(progress * 100.0);
    });
    on_cleanup(move || drop(scroll_handle));

    // Mouse hover overrides scroll position
    let handle_mousemove = move |ev: web_sys::MouseEvent| {
        if let Some(target) = ev.current_target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        {
            let rect = target.get_bounding_client_rect();
            let x = ev.client_x() as f32 - rect.left() as f32;
            let w = rect.width() as f32;
            if w > 0.0 {
                set_position.set((x / w * 100.0).clamp(0.0, 100.0));
            }
        }
    };

    view! {
        <div class="w-full">
            <div
                node_ref=node_ref
                class="relative w-full overflow-hidden rounded-lg border border-gray-100 dark:border-gray-800 cursor-col-resize"
                style="height: 420px;"
                on:mousemove=handle_mousemove
                on:mouseenter=move |_| is_hovering.set(true)
                on:mouseleave=move |_| is_hovering.set(false)
            >
                {/* Art panel — clips from the right as position increases */}
                <div
                    class="absolute inset-0 transition-all duration-75"
                    style=move || format!("clip-path: inset(0 {}% 0 0);", position.get())
                >
                    <ArtPanel />
                </div>

                {/* Technique panel — clips from the left */}
                <div
                    class="absolute inset-0 transition-all duration-75"
                    style=move || format!("clip-path: inset(0 0 0 {}%);", 100.0 - position.get())
                >
                    <TechniquePanel />
                </div>

                {/* Slider — divider line + knob + labels */}
                <SliderInput position set_position />
            </div>

            {/* Caption */}
            <p class="text-center mt-5 text-xs text-gray-400 dark:text-gray-600 uppercase tracking-widest">
                {move || {
                    let pos = position.get() as u32;
                    let l = lang.get();
                    match pos {
                        0..=4    => t(l, "curtain.start"),
                        96..=100 => t(l, "curtain.end"),
                        _        => t(l, "curtain.mid"),
                    }
                }}
            </p>
        </div>
    }
}
