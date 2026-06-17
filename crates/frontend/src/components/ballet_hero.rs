//! BalletHero — a discreet, mouse-reactive particle field rendered with three-d
//! (pure-Rust, WebGL2) behind the hero content. A soft, slightly-blurred drift of
//! brand-coloured points that gently parallaxes toward the cursor — present but
//! never loud. Pause/resume freezes the drift; `prefers-reduced-motion` starts
//! paused. The mouse parallax stays responsive.
//!
//! All WebGL / three-d code lives in the `scene` submodule, gated to
//! `target_arch = "wasm32"` (glow's `from_webgl2_context` only exists on wasm, and
//! the frontend only ever *runs* on wasm). On other targets (e.g. the host build
//! used by `cargo test --all --lib`) the component still renders its markup.

use leptos::*;

#[component]
pub fn BalletHero() -> impl IntoView {
    let canvas_ref = create_node_ref::<html::Canvas>();
    let (paused, set_paused) = create_signal(prefers_reduced_motion());

    #[cfg(target_arch = "wasm32")]
    scene::mount(canvas_ref, paused);

    // Background layer: soft blur + reduced opacity push the field behind the
    // content. pointer-events:none lets the page receive the mouse (the field
    // listens on window). Inlined so it renders before the Tailwind CSS rebuilds.
    view! {
        <div
            class="ballet-hero-3d"
            style="position:absolute; inset:0; pointer-events:none;"
        >
            <canvas
                node_ref=canvas_ref
                style="width:100%; height:100%; display:block; filter:blur(1.4px); opacity:0.7;"
                aria-hidden="true"
            ></canvas>
            <button
                class="ballet-hero-toggle"
                on:click=move |_| set_paused.update(|p| *p = !*p)
                aria-label=move || if paused.get() { "Reprendre l'animation" } else { "Mettre l'animation en pause" }
                style="position:absolute; bottom:1.25rem; right:1.25rem; pointer-events:auto; z-index:20; \
                       width:2.25rem; height:2.25rem; border-radius:9999px; \
                       display:flex; align-items:center; justify-content:center; \
                       font-size:0.85rem; line-height:1; color:rgba(255,255,255,0.85); \
                       background:rgba(255,255,255,0.06); border:1px solid rgba(255,255,255,0.14); \
                       backdrop-filter:blur(6px); cursor:pointer; transition:background 0.2s;"
            >
                {move || if paused.get() { "▶" } else { "⏸" }}
            </button>
        </div>
    }
}

/// Reads the OS "reduce motion" preference; true → start paused.
fn prefers_reduced_motion() -> bool {
    web_sys::window()
        .and_then(|w| w.match_media("(prefers-reduced-motion: reduce)").ok().flatten())
        .map(|m| m.matches())
        .unwrap_or(false)
}

#[cfg(target_arch = "wasm32")]
mod scene {
    use leptos::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::Arc;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    use three_d::{
        vec3, ClearState, ColorMaterial, CpuMesh, Gm, InstancedMesh, Instances, Mat4, Srgba, Vec3,
        Viewport,
    };

    /// Number of particles in the field.
    const PARTICLES: usize = 620;

    fn request_animation_frame(cb: &Closure<dyn FnMut(f64)>) {
        let _ = web_sys::window()
            .unwrap()
            .request_animation_frame(cb.as_ref().unchecked_ref());
    }

    /// Deterministic pseudo-random in [0,1) from an integer seed (no rng dep).
    fn hash01(n: u32) -> f32 {
        let mut x = n.wrapping_mul(0x9E37_79B1);
        x ^= x >> 15;
        x = x.wrapping_mul(0x85EB_CA77);
        x ^= x >> 13;
        (x & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
    }

    /// Scattered rest position of particle `i` in a wide, shallow slab.
    fn particle_pos(i: usize) -> Vec3 {
        let i = i as u32;
        let rx = hash01(i * 3) * 2.0 - 1.0;
        let ry = hash01(i * 3 + 1) * 2.0 - 1.0;
        let rz = hash01(i * 3 + 2) * 2.0 - 1.0;
        vec3(rx * 5.2, ry * 3.2, rz * 2.4 - 0.4)
    }

    /// Discreet brand colour for particle `i`: a dim red↔teal mix.
    fn particle_color(i: usize) -> Srgba {
        let t = hash01(i as u32 * 7 + 11);
        let lerp = |a: f32, b: f32| (a + (b - a) * t) as u8;
        // dim red (200,52,56) ↔ dim teal (32,168,150)
        Srgba::new(lerp(200.0, 32.0), lerp(52.0, 168.0), lerp(56.0, 150.0), 255)
    }

    pub fn mount(canvas_ref: NodeRef<html::Canvas>, paused: ReadSignal<bool>) {
        create_effect(move |_| {
            let Some(canvas) = canvas_ref.get() else { return };
            let canvas: web_sys::HtmlCanvasElement = (*canvas).clone();

            let mut attrs = web_sys::WebGlContextAttributes::new();
            let _ = attrs.alpha(true).antialias(true);
            let gl = match canvas.get_context_with_context_options("webgl2", attrs.as_ref()) {
                Ok(Some(obj)) => obj.unchecked_into::<web_sys::WebGl2RenderingContext>(),
                _ => {
                    leptos::logging::warn!("[hero] WebGL2 unavailable — skipping 3D");
                    return;
                }
            };
            let glow = three_d::context::Context::from_webgl2_context(gl);
            let context = match three_d::Context::from_gl_context(Arc::new(glow)) {
                Ok(c) => c,
                Err(e) => {
                    leptos::logging::warn!("[hero] three-d context failed: {e:?}");
                    return;
                }
            };

            // The particle field: one instanced sphere mesh, dim brand colours,
            // sizes varied slightly for depth.
            let transformations: Vec<Mat4> = (0..PARTICLES)
                .map(|i| {
                    // Tiny — reads as dots, not spheres.
                    let s = 0.006 + hash01(i as u32 * 13 + 3) * 0.006;
                    Mat4::from_translation(particle_pos(i)) * Mat4::from_scale(s)
                })
                .collect();
            let colors: Vec<Srgba> = (0..PARTICLES).map(particle_color).collect();
            let instances = Instances {
                transformations,
                colors: Some(colors),
                ..Default::default()
            };
            let mut field = Gm::new(
                InstancedMesh::new(&context, &instances, &CpuMesh::sphere(6)),
                ColorMaterial::default(),
            );

            let mut camera = three_d::Camera::new_perspective(
                Viewport::new_at_origo(1, 1),
                vec3(0.0, 0.0, 7.0),
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 1.0, 0.0),
                three_d::degrees(34.0),
                0.1,
                100.0,
            );

            // Raw mouse (normalised −1..1) updated by a window listener; smoothed
            // each frame for a gentle, discreet parallax.
            let mouse = Rc::new(Cell::new((0.0f32, 0.0f32)));
            let smooth = Rc::new(Cell::new((0.0f32, 0.0f32)));
            let mouse_cb = {
                let mouse = mouse.clone();
                Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
                    let w = web_sys::window().unwrap();
                    let iw = w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1.0).max(1.0) as f32;
                    let ih = w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(1.0).max(1.0) as f32;
                    let nx = (e.client_x() as f32 / iw) * 2.0 - 1.0;
                    let ny = (e.client_y() as f32 / ih) * 2.0 - 1.0;
                    mouse.set((nx, ny));
                }) as Box<dyn FnMut(web_sys::MouseEvent)>)
            };
            let window = web_sys::window().unwrap();
            let _ = window.add_event_listener_with_callback(
                "mousemove",
                mouse_cb.as_ref().unchecked_ref(),
            );

            let clock = Rc::new(Cell::new(0.0_f64));
            let last = Rc::new(Cell::new(None::<f64>));
            let alive = Rc::new(Cell::new(true));

            let cb: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
            let cb2 = cb.clone();
            let alive2 = alive.clone();

            *cb.borrow_mut() = Some(Closure::wrap(Box::new(move |ts: f64| {
                if !alive2.get() {
                    return;
                }

                let dt = last.get().map(|p| ts - p).unwrap_or(0.0);
                last.set(Some(ts));
                if !paused.get_untracked() {
                    clock.set(clock.get() + dt);
                }
                let time = (clock.get() / 1000.0) as f32;

                // Ease the smoothed mouse toward the raw position (stays responsive
                // even while paused — it's interaction, not animation).
                let (mx, my) = mouse.get();
                let (sx, sy) = smooth.get();
                let sx = sx + (mx - sx) * 0.06;
                let sy = sy + (my - sy) * 0.06;
                smooth.set((sx, sy));

                let dpr = web_sys::window().unwrap().device_pixel_ratio() as f32;
                let w = (canvas.client_width().max(1) as f32 * dpr) as u32;
                let h = (canvas.client_height().max(1) as f32 * dpr) as u32;
                if canvas.width() != w {
                    canvas.set_width(w);
                }
                if canvas.height() != h {
                    canvas.set_height(h);
                }

                // Camera parallax: the field gently leans toward the cursor.
                camera.set_view(
                    vec3(sx * 1.3, -sy * 0.9, 7.0),
                    vec3(sx * 0.4, -sy * 0.3, 0.0),
                    vec3(0.0, 1.0, 0.0),
                );
                camera.set_viewport(Viewport::new_at_origo(w, h));

                // A very slow drift gives life without drawing attention.
                field.set_transformation(Mat4::from_angle_y(three_d::Rad(time * 0.05)));

                three_d::RenderTarget::screen(&context, w, h)
                    .clear(ClearState::color_and_depth(0.0, 0.0, 0.0, 0.0, 1.0))
                    .render(&camera, &field, &[]);

                request_animation_frame(cb2.borrow().as_ref().unwrap());
            }) as Box<dyn FnMut(f64)>));

            request_animation_frame(cb.borrow().as_ref().unwrap());

            // Keep closures alive for the component's lifetime; tear down on unmount.
            // `mouse_cb` is moved into the cleanup so it stays alive until then,
            // then the listener is removed and the closure dropped.
            let keep = cb.clone();
            on_cleanup(move || {
                alive.set(false);
                if let Some(w) = web_sys::window() {
                    let _ = w.remove_event_listener_with_callback(
                        "mousemove",
                        mouse_cb.as_ref().unchecked_ref(),
                    );
                }
                drop(keep.borrow_mut().take());
                drop(mouse_cb);
            });
        });
    }
}
