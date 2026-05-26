//! Behind the Curtain interactive component.
//!
//! Demonstrates pointe.dev's philosophy: invisible complexity hidden behind
//! effortless grace. Users drag a slider to reveal the technical internals.

pub mod state;
pub mod panels;
pub mod slider;
pub mod animations;

use leptos::*;
use panels::{ArtPanel, TechniquePanel};
use slider::SliderInput;

/// Main Behind the Curtain component.
///
/// Features:
/// - Interactive slider to toggle between "The Art" and "The Technique"
/// - Smooth opacity transitions
/// - Fine-grained reactive updates
#[component]
pub fn BehindCurtain() -> impl IntoView {
    let (position, set_position) = create_signal(0.0);
    let art_visible = create_memo(move |_| animations::art_opacity(position.get()));
    let technique_visible = create_memo(move |_| animations::technique_opacity(position.get()));

    view! {
        <div class="behind-curtain-container w-full min-h-screen bg-black">
            <div class="text-center py-12">
                <h2 class="text-5xl font-bold text-white mb-4">
                    <span class="text-red-600">"Invisible"</span>" Complexity."
                </h2>
                <p class="text-gray-400 text-lg max-w-2xl mx-auto">
                    "Drag the slider to see the engineering behind the elegance."
                </p>
            </div>

            <div class="panel relative w-full overflow-hidden rounded-lg border border-gray-800">
                <div
                    class="absolute inset-0 transition-opacity duration-200"
                    style:opacity=move || format!("{}", art_visible.get())
                >
                    <ArtPanel />
                </div>

                <div
                    class="absolute inset-0 transition-opacity duration-200"
                    style:opacity=move || format!("{}", technique_visible.get())
                >
                    <TechniquePanel />
                </div>

                <SliderInput position set_position />
            </div>

            <div class="text-center mt-12 text-gray-400">
                <p>
                    <span class="text-red-600 font-bold">{move || format!("{}%", position.get() as u32)}</span>
                    {" → "}
                    {move || if position.get() < 50.0 { "The Art" } else { "The Technique" }}
                </p>
            </div>
        </div>
    }
}
