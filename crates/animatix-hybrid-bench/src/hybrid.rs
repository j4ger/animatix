//! Hybrid backend: `vello_hybrid` on WebGL2, rendered straight to the canvas.

use vello_hybrid::{RenderSize, Resources, Scene, WebGlRenderer};
use wasm_bindgen::JsCast;
use web_sys::{HtmlCanvasElement, WebGl2RenderingContext};

use crate::bench::{BenchResult, now_ms};

pub struct HybridRunner {
    renderer: WebGlRenderer,
    resources: Resources,
    gl: WebGl2RenderingContext,
    width: u32,
    height: u32,
}

pub fn init(canvas: HtmlCanvasElement, width: u32, height: u32) -> Result<HybridRunner, String> {
    let width = width.max(1);
    let height = height.max(1);
    canvas.set_width(width);
    canvas.set_height(height);
    // `WebGlRenderer::new` gets the WebGL2 context itself (with antialias:false,
    // depth:true) and panics if unavailable — the page checks support on a
    // throwaway canvas first, so this only runs when WebGL2 exists.
    let renderer = WebGlRenderer::new(&canvas);
    let gl = canvas
        .get_context("webgl2")
        .map_err(|e| format!("get_context: {e:?}"))?
        .ok_or_else(|| "no webgl2 context".to_string())?
        .dyn_into::<WebGl2RenderingContext>()
        .map_err(|_| "webgl2 context cast failed".to_string())?;
    Ok(HybridRunner {
        renderer,
        resources: Resources::new(),
        gl,
        width,
        height,
    })
}

impl HybridRunner {
    pub fn render(&mut self, scene: &Scene) -> Result<(), String> {
        self.renderer
            .render(
                scene,
                &mut self.resources,
                &RenderSize {
                    width: self.width,
                    height: self.height,
                },
            )
            .map_err(|e| format!("render: {e}"))?;
        Ok(())
    }

    /// WebGL2 has no `onSubmittedWorkDone`; `readPixels` of a single pixel is the
    /// reliable "GPU finished" sync point.
    pub fn drain(&self) {
        let _ = self.gl.finish();
        let mut px = [0u8; 4];
        let _ = self.gl.read_pixels_with_opt_u8_array(
            0,
            0,
            1,
            1,
            WebGl2RenderingContext::RGBA,
            WebGl2RenderingContext::UNSIGNED_BYTE,
            Some(&mut px),
        );
    }
}

pub fn build_hybrid(kind: &str, t: f64, width: u32, height: u32) -> Scene {
    crate::scene::build_hybrid(kind, t, width, height)
}

pub fn run(
    canvas: HtmlCanvasElement,
    scene_kind: &str,
    width: u32,
    height: u32,
    frames: u32,
    warmup: u32,
    reps: u32,
) -> Result<BenchResult, String> {
    let mut runner = init(canvas, width, height)?;
    let frames = frames.max(1);
    let reps = reps.max(1);

    for i in 0..warmup {
        let s = build_hybrid(scene_kind, i as f64 / 60.0, width, height);
        runner.render(&s)?;
    }
    runner.drain();

    let empty = Scene::new(width.max(1) as u16, height.max(1) as u16);
    let mut clear_only = f64::INFINITY;
    for _ in 0..reps {
        let t0 = now_ms();
        for _ in 0..frames {
            runner.render(&empty)?;
        }
        runner.drain();
        clear_only = clear_only.min((now_ms() - t0) / frames as f64);
    }

    let mut best_total = f64::INFINITY;
    let mut best_submit = f64::INFINITY;
    for _ in 0..reps {
        let t0 = now_ms();
        for i in 0..frames {
            let s = build_hybrid(scene_kind, i as f64 / 60.0, width, height);
            runner.render(&s)?;
        }
        let t_submit = now_ms();
        runner.drain();
        let t_end = now_ms();
        best_submit = best_submit.min((t_submit - t0) / frames as f64);
        best_total = best_total.min((t_end - t0) / frames as f64);
    }

    Ok(BenchResult {
        backend: "hybrid".to_string(),
        scene: scene_kind.to_string(),
        width,
        height,
        frames,
        reps,
        per_frame_ms: best_total,
        submit_ms: best_submit,
        gpu_ms: (best_total - best_submit).max(0.0),
        clear_only_ms: clear_only,
    })
}
