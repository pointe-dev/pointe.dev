//! BalletHero — a three-d (pure-Rust, WebGL2) hero animation embedded in a
//! Leptos-owned <canvas>. The brand thesis made visible: classical dance *en
//! pointe* — lightness and grace tracing a slow parametric "pirouette", over a
//! deep dark field, above a faint counter-rotating icosahedral cage (the hidden
//! complexity under the grace). Brand palette: red (#dc2626) lerped to teal.
//!
//! All WebGL / three-d code lives in the `scene` submodule, gated to
//! `target_arch = "wasm32"`: glow's `from_webgl2_context` only exists on wasm, and
//! the frontend only ever *runs* on wasm. On other targets (e.g. the host build
//! used by `cargo test --all --lib`) the component still renders its markup, just
//! without mounting the 3D scene.

use leptos::*;

#[component]
pub fn BalletHero() -> impl IntoView {
    let canvas_ref = create_node_ref::<html::Canvas>();

    // `paused` is user-facing (button); it also starts true when the OS asks for
    // reduced motion.
    let (paused, set_paused) = create_signal(prefers_reduced_motion());

    // Mount the WebGL scene only on wasm; on the host build this is a no-op.
    #[cfg(target_arch = "wasm32")]
    scene::mount(canvas_ref, paused);

    // Critical layout is inlined so the overlay + toggle render correctly even
    // before the Tailwind CSS is rebuilt.
    view! {
        <div
            class="ballet-hero-3d"
            style="position:absolute; inset:0; pointer-events:none;"
        >
            <canvas
                node_ref=canvas_ref
                style="width:100%; height:100%; display:block;"
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
        vec3, ClearState, ColorMaterial, CpuMesh, Gm, InnerSpace, InstancedMesh, Instances, Mat4,
        Quaternion, Srgba, Vec3, Viewport,
    };

    /// Number of particles tracing the curve.
    const POINTS: usize = 240;

    fn request_animation_frame(cb: &Closure<dyn FnMut(f64)>) {
        let _ = web_sys::window()
            .unwrap()
            .request_animation_frame(cb.as_ref().unchecked_ref());
    }

    /// Position on the 3D "pirouette" curve at parameter `u` ∈ [0,1).
    /// A 3-petal rose swept on a graceful vertical sine — reads as an arabesque arc.
    fn curve_point(u: f32) -> Vec3 {
        let t = u * std::f32::consts::TAU;
        let r = 2.0_f32;
        let x = r * (3.0 * t).sin() * t.cos();
        let z = r * (3.0 * t).sin() * t.sin();
        let y = 1.4 * (2.0 * t).sin();
        vec3(x, y, z)
    }

    /// Brand gradient red(#dc2626) → teal(#00e5cc) by `u` ∈ [0,1].
    fn gradient(u: f32) -> Srgba {
        let lerp = |a: f32, b: f32| (a + (b - a) * u) as u8;
        Srgba::new(lerp(220.0, 0.0), lerp(38.0, 229.0), lerp(38.0, 204.0), 255)
    }

    /// The 12 vertices + edges of a regular icosahedron (edge length 2), scaled to
    /// enclose the pirouette curve — the "hidden complexity" cage under the grace.
    fn icosahedron(scale: f32) -> (Vec<Vec3>, Vec<(usize, usize)>) {
        let p = (1.0 + 5f32.sqrt()) / 2.0; // golden ratio
        let raw = [
            (-1.0, p, 0.0), (1.0, p, 0.0), (-1.0, -p, 0.0), (1.0, -p, 0.0),
            (0.0, -1.0, p), (0.0, 1.0, p), (0.0, -1.0, -p), (0.0, 1.0, -p),
            (p, 0.0, -1.0), (p, 0.0, 1.0), (-p, 0.0, -1.0), (-p, 0.0, 1.0),
        ];
        let verts: Vec<Vec3> = raw.iter().map(|&(x, y, z)| vec3(x, y, z) * scale).collect();
        let target = 4.0 * scale * scale;
        let mut edges = Vec::new();
        for i in 0..verts.len() {
            for j in (i + 1)..verts.len() {
                if ((verts[i] - verts[j]).magnitude2() - target).abs() < 1e-3 * scale * scale {
                    edges.push((i, j));
                }
            }
        }
        (verts, edges)
    }

    /// Transform a unit cylinder (along +X, range [0,1], radius 1) into the segment
    /// p1→p2 with the given radius.
    fn edge_transform(p1: Vec3, p2: Vec3, radius: f32) -> Mat4 {
        let dir = p2 - p1;
        let len = dir.magnitude();
        let rot: Mat4 = Quaternion::from_arc(vec3(1.0, 0.0, 0.0), dir.normalize(), None).into();
        Mat4::from_translation(p1) * rot * Mat4::from_nonuniform_scale(len, radius, radius)
    }

    /// Build the WebGL2 context from the Leptos-owned canvas and run the render loop.
    pub fn mount(canvas_ref: NodeRef<html::Canvas>, paused: ReadSignal<bool>) {
        create_effect(move |_| {
            let Some(canvas) = canvas_ref.get() else { return };
            let canvas: web_sys::HtmlCanvasElement = (*canvas).clone();

            // WebGL2 context (alpha so it overlays the existing hero background).
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

            // Particle curve: one instanced sphere mesh, gradient-coloured.
            let transformations: Vec<Mat4> = (0..POINTS)
                .map(|i| {
                    let u = i as f32 / POINTS as f32;
                    Mat4::from_translation(curve_point(u)) * Mat4::from_scale(0.045)
                })
                .collect();
            let colors: Vec<Srgba> = (0..POINTS).map(|i| gradient(i as f32 / POINTS as f32)).collect();
            let instances = Instances {
                transformations,
                colors: Some(colors),
                ..Default::default()
            };
            let mut particles = Gm::new(
                InstancedMesh::new(&context, &instances, &CpuMesh::sphere(8)),
                ColorMaterial::default(),
            );

            // Hidden-complexity cage: faint icosahedral wireframe (edges = thin
            // instanced cylinders) enclosing the curve, counter-rotating behind it.
            let (verts, edges) = icosahedron(1.4);
            let cage_instances = Instances {
                transformations: edges
                    .iter()
                    .map(|&(a, b)| edge_transform(verts[a], verts[b], 0.012))
                    .collect(),
                colors: Some(vec![Srgba::new(44, 78, 74, 255); edges.len()]),
                ..Default::default()
            };
            let mut cage = Gm::new(
                InstancedMesh::new(&context, &cage_instances, &CpuMesh::cylinder(6)),
                ColorMaterial::default(),
            );

            let mut camera = three_d::Camera::new_perspective(
                Viewport::new_at_origo(1, 1),
                vec3(0.0, 1.2, 6.5),
                vec3(0.0, 0.0, 0.0),
                vec3(0.0, 1.0, 0.0),
                three_d::degrees(32.0),
                0.1,
                100.0,
            );

            // Animation clock that only advances while not paused.
            let clock = Rc::new(Cell::new(0.0_f64));
            let last = Rc::new(Cell::new(None::<f64>));
            let alive = Rc::new(Cell::new(true));

            let cb: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
            let cb2 = cb.clone();
            let alive2 = alive.clone();

            *cb.borrow_mut() = Some(Closure::wrap(Box::new(move |ts: f64| {
                if !alive2.get() {
                    return; // unmounted — stop the loop, drop the closure.
                }

                let dt = last.get().map(|p| ts - p).unwrap_or(0.0);
                last.set(Some(ts));
                if !paused.get_untracked() {
                    clock.set(clock.get() + dt);
                }
                let time = (clock.get() / 1000.0) as f32;

                let dpr = web_sys::window().unwrap().device_pixel_ratio() as f32;
                let w = (canvas.client_width().max(1) as f32 * dpr) as u32;
                let h = (canvas.client_height().max(1) as f32 * dpr) as u32;
                if canvas.width() != w {
                    canvas.set_width(w);
                }
                if canvas.height() != h {
                    canvas.set_height(h);
                }
                camera.set_viewport(Viewport::new_at_origo(w, h));

                // Grace turns one way; the hidden mechanism counter-rotates, slower.
                particles.set_transformation(Mat4::from_angle_y(three_d::Rad(time * 0.35)));
                cage.set_transformation(Mat4::from_angle_y(three_d::Rad(time * -0.18)));

                three_d::RenderTarget::screen(&context, w, h)
                    .clear(ClearState::color_and_depth(0.0, 0.0, 0.0, 0.0, 1.0))
                    .render(&camera, (&cage).into_iter().chain(&particles), &[]);

                request_animation_frame(cb2.borrow().as_ref().unwrap());
            }) as Box<dyn FnMut(f64)>));

            request_animation_frame(cb.borrow().as_ref().unwrap());

            // Keep the closure alive for the component's lifetime; stop on unmount.
            let keep = cb.clone();
            on_cleanup(move || {
                alive.set(false);
                drop(keep.borrow_mut().take());
            });
        });
    }
}
