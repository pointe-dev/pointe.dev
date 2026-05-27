pub mod state;
pub mod panels;
pub mod slider;
pub mod animations;

use leptos::*;
use wasm_bindgen::JsCast;
use panels::{ArtPanel, TechniquePanel};
use slider::SliderInput;
use crate::i18n::{Lang, t};

#[component]
pub fn BehindCurtain() -> impl IntoView {
    let (position, set_position) = create_signal(0.0f32);
    let lang = use_context::<RwSignal<Lang>>()
        .unwrap_or_else(|| create_rw_signal(Lang::Fr));

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
                class="relative w-full overflow-hidden rounded-lg border border-gray-100 dark:border-gray-800 cursor-col-resize"
                style="height: 420px;"
                on:mousemove=handle_mousemove
            >
                {/* Art panel — clips from the right as slider moves right */}
                <div
                    class="absolute inset-0"
                    style=move || format!("clip-path: inset(0 {}% 0 0);", position.get())
                >
                    <ArtPanel />
                </div>

                {/* Technique panel — clips from the left as slider moves left */}
                <div
                    class="absolute inset-0"
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
