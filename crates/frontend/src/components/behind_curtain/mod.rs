pub mod state;
pub mod panels;
pub mod slider;
pub mod animations;

use leptos::*;
use panels::{ArtPanel, TechniquePanel};
use slider::SliderInput;

#[component]
pub fn BehindCurtain() -> impl IntoView {
    let (position, set_position) = create_signal(0.0f32);

    view! {
        <div class="w-full">
            // Reveal container — fixed height, clip-path curtain split
            <div
                class="relative w-full overflow-hidden rounded-lg border border-gray-100 dark:border-gray-800"
                style="height: 420px;"
            >
                // Art panel — clips from the right as slider moves right
                <div
                    class="absolute inset-0"
                    style=move || format!("clip-path: inset(0 {}% 0 0);", position.get())
                >
                    <ArtPanel />
                </div>

                // Technique panel — clips from the left as slider moves left
                <div
                    class="absolute inset-0"
                    style=move || format!("clip-path: inset(0 0 0 {}%);", 100.0 - position.get())
                >
                    <TechniquePanel />
                </div>

                // Slider — divider line + knob + labels
                <SliderInput position set_position />
            </div>

            // Caption
            <p class="text-center mt-5 text-xs text-gray-400 dark:text-gray-600 uppercase tracking-widest">
                {move || match position.get() as u32 {
                    0..=4  => "← Glisser pour révéler",
                    96..=100 => "La complexité invisible",
                    _ => "Invisible complexity, visible grace",
                }}
            </p>
        </div>
    }
}
