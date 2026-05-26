//! Slider input component with Pointelligence particles.

use leptos::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlInputElement;

/// Particle effect for Pointelligence animation
#[component]
fn Particle(x: f32, y: f32, delay: f32) -> impl IntoView {
    let duration = 2.0 + (delay % 1.0);
    let animation_style = format!(
        "@keyframes particle-{} {{ 0% {{ opacity: 0; transform: translate({}px, {}px); }} 50% {{ opacity: 1; }} 100% {{ opacity: 0; transform: translate({}px, {}px); }} }}",
        delay as u32,
        x,
        y,
        (x as i32) * 2,
        (y as i32) * 2
    );

    view! {
        <style>{animation_style}</style>
        <div
            class="particle w-1 h-1 bg-red-600 rounded-full"
            style:left=move || format!("{}%", 50.0 + (delay % 10.0))
            style:top=move || format!("{}%", 50.0 + ((delay * 2.0) % 10.0))
            style:animation=move || format!("particle-{} {}s ease-out forwards", delay as u32, duration)
        />
    }
}

/// Interactive slider input for the Behind the Curtain component with Pointelligence effects.
#[component]
pub fn SliderInput(
    position: ReadSignal<f32>,
    set_position: WriteSignal<f32>,
) -> impl IntoView {
    let (is_dragging, set_is_dragging) = create_signal(false);
    let (particles, set_particles) = create_signal(Vec::new());

    let handle_input = move |ev: web_sys::Event| {
        if let Some(input) = ev.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok()) {
            if let Ok(value) = input.value().parse::<f32>() {
                set_position.set(value);
                
                // Generate particles on drag
                if is_dragging.get() {
                    let new_particle = (value, js_sys::Math::random() as f32 * 100.0);
                    let mut p = particles.get();
                    p.push(new_particle);
                    if p.len() > 15 {
                        p.remove(0);
                    }
                    set_particles(p);
                }
            }
        }
    };

    let handle_mouse_down = move |_: web_sys::MouseEvent| {
        set_is_dragging.set(true);
    };

    let handle_touch_start = move |_: web_sys::TouchEvent| {
        set_is_dragging.set(true);
    };

    let handle_mouse_up = move |_: web_sys::MouseEvent| {
        set_is_dragging.set(false);
    };

    let handle_touch_end = move |_: web_sys::TouchEvent| {
        set_is_dragging.set(false);
    };

    view! {
        <div class="absolute inset-0 flex items-center justify-center pointer-events-none">
            {/* Particle container */}
            <div class="absolute inset-0 overflow-hidden">
                {move || {
                    particles.get().iter().enumerate().map(|(i, (_, delay))| {
                        view! {
                            <Particle 
                                x={(i as f32 - 7.5) * 3.0}
                                y={(js_sys::Math::random() as f32 - 0.5) * 50.0}
                                delay={*delay}
                            />
                        }
                    }).collect_view()
                }}
            </div>

            {/* Slider track */}
            <input
                type="range"
                min="0"
                max="100"
                step="1"
                value=move || position.get().to_string()
                on:input=handle_input
                on:mousedown=handle_mouse_down
                on:touchstart=handle_touch_start
                on:mouseup=handle_mouse_up
                on:touchend=handle_touch_end
                class="absolute w-full h-full opacity-0 cursor-col-resize pointer-events-auto z-10"
            />

            <div class="absolute inset-0 flex items-center h-full w-full pointer-events-none">
                <div class="w-full h-1 bg-gray-700 rounded" />
                <div
                    class="slider-track slider-track-active absolute h-1 bg-gradient-to-r from-red-600 to-red-500 rounded"
                    style:width=move || format!("{}%", position.get())
                />
            </div>

            {/* Slider handle - the pointe dancer */}
            <div
                class="slider-handle absolute top-1/2 -translate-y-1/2 w-6 h-6 bg-red-600 rounded-full shadow-lg border-2 border-red-400 pointer-events-none z-20 transition-all"
                style:left=move || format!("{}%", position.get())
                style:transform="translateX(-50%)"
                style:box-shadow=move || {
                    let intensity = if is_dragging.get() { 1.2 } else { 0.8 };
                    format!(
                        "0 0 {}px rgba(211, 47, 47, 0.8), 0 0 {}px rgba(211, 47, 47, 0.4)",
                        20.0 * intensity,
                        40.0 * intensity
                    )
                }
            />
        </div>
    }
}
