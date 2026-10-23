//! Playback state entry points: duration, pause points, scrubbing, render
//! scale, and CPU frame benchmarks.

use wasm_bindgen::prelude::*;

use super::{AmxPlayer, MAX_RENDER_SCALE, MIN_RENDER_SCALE, raster_dims};

#[wasm_bindgen]
impl AmxPlayer {
    /// Whether a document is currently loaded and renderable.
    pub fn has_document(&self) -> bool {
        self.target.is_some()
    }

    pub fn duration_s(&self) -> f64 {
        self.duration_s
    }

    /// Interactive pause points across the document in seconds.
    pub fn pause_points(&self) -> Vec<f64> {
        self.target
            .as_ref()
            .map(|t| t.pause_points().iter().map(|&ms| ms as f64 / 1000.0).collect())
            .unwrap_or_default()
    }

    /// Returns the next pause point strictly after `current_time_s` in seconds.
    pub fn step_next(&self, current_time_s: f64) -> Option<f64> {
        let current_ms = (current_time_s * 1000.0).round() as u64;
        self.target
            .as_ref()
            .and_then(|t| t.step_next(current_ms))
            .map(|ms| ms as f64 / 1000.0)
    }

    /// Returns the previous pause point strictly before `current_time_s` in seconds.
    pub fn step_prev(&self, current_time_s: f64) -> Option<f64> {
        let current_ms = (current_time_s * 1000.0).round() as u64;
        self.target
            .as_ref()
            .and_then(|t| t.step_prev(current_ms))
            .map(|ms| ms as f64 / 1000.0)
    }

    /// Raster scale for the offscreen render targets, clamped to
    /// `MIN_RENDER_SCALE..=MAX_RENDER_SCALE`; returns the value in effect.
    ///
    /// A frame's GPU cost tracks the *raster* pixel count, not the scene's
    /// content — an empty 1280×720 scene and a full one measure the same — so
    /// this is the one knob that moves browser frame time materially. Layout is
    /// unaffected: the timeline is still evaluated against the scene's own
    /// dimensions, and only the rasterization is scaled (the canvas blit scales
    /// it back up). The shell's job is to pick a scale that fits the element's
    /// displayed pixels; `debug_bench` reports what a scale costs.
    pub fn set_render_scale(&mut self, scale: f64) -> f64 {
        let scale = if scale.is_finite() { scale as f32 } else { 1.0 };
        self.render_scale = scale.clamp(MIN_RENDER_SCALE, MAX_RENDER_SCALE);
        self.render_scale as f64
    }

    pub fn render_scale(&self) -> f64 {
        self.render_scale as f64
    }

    /// The offscreen raster size the next frame will render at, in pixels.
    pub fn raster_width(&self) -> u32 {
        raster_dims(self.dims, self.render_scale).0
    }

    pub fn raster_height(&self) -> u32 {
        raster_dims(self.dims, self.render_scale).1
    }

    /// Drive `frames` frames of the loaded document and report what the *CPU*
    /// side of them cost, as a JSON object:
    ///
    /// ```text
    /// { frames, scene: [w, h], raster: [w, h], scale, cpu_ms,
    ///   cpu_p50_ms, cpu_p90_ms, cpu_max_ms }
    /// ```
    ///
    /// Pair it with [`crate::web::debug_gpu_drain`] to get the frame cost a display would
    /// be paced at:
    ///
    /// ```js
    /// const t0 = performance.now();
    /// const cpu = JSON.parse(player.debug_bench_frames(60, 1 / 60));
    /// await debug_gpu_drain();
    /// const perFrameMs = (performance.now() - t0) / cpu.frames;
    /// ```
    ///
    /// `cpu_ms` is time spent inside `render_frame` (evaluate + vello encode +
    /// submit + blit encode); `perFrameMs - cpu_ms` is therefore the GPU's
    /// share, and `perFrameMs` is what has to fit in a frame budget.
    ///
    /// Why a dedicated driver rather than timing `requestAnimationFrame`: a
    /// backgrounded or headless tab delivers no rAF callbacks at all, and
    /// `queue.submit` returns long before the GPU has drawn anything, so a
    /// naive per-frame timer measures encoding only (it reports ~0.5 ms for a
    /// frame whose GPU work is ~4 ms). Driving the frames explicitly against an
    /// explicit sync point is the same shape as the native
    /// `perf_driver` / `export_perf_driver` harnesses.
    ///
    /// Two warm-up frames are rendered and drained first, so pipeline
    /// compilation and atlas growth are not billed to the mean.
    pub fn debug_bench_frames(&mut self, frames: u32, step_s: f64) -> Result<JsValue, JsError> {
        let frames = frames.clamp(1, 2000);
        let step = if step_s.is_finite() && step_s > 0.0 {
            step_s
        } else {
            1.0 / 60.0
        };
        if !self.has_document() {
            return Err(JsError::new("no document loaded"));
        }

        let mut cpu = Vec::with_capacity(frames as usize);
        for i in 0..frames {
            let t0 = now_ms();
            // A benchmark that dedups measures nothing: this loop asks "what does
            // drawing a frame cost", and two iterations at the same quantised
            // time would otherwise answer "free". Clearing the remembered frame
            // is a store of `None` — no GPU work — so it cannot bias the timing.
            self.dedup.invalidate();
            self.render_frame(i as f64 * step)?;
            cpu.push(now_ms() - t0);
        }
        let cpu_ms = cpu.iter().sum::<f64>() / frames as f64;
        cpu.sort_by(f64::total_cmp);
        let pick = |p: f64| -> f64 {
            let idx = ((cpu.len() - 1) as f64 * p).round() as usize;
            cpu[idx.min(cpu.len() - 1)]
        };
        let raster = raster_dims(self.dims, self.render_scale);
        Ok(JsValue::from_str(&format!(
            "{{\"frames\":{frames},\"scene\":[{},{}],\"raster\":[{},{}],\"scale\":{},\
             \"cpu_ms\":{cpu_ms:.4},\"cpu_p50_ms\":{:.4},\"cpu_p90_ms\":{:.4},\"cpu_max_ms\":{:.4}}}",
            self.dims.width,
            self.dims.height,
            raster.0,
            raster.1,
            self.render_scale,
            pick(0.5),
            pick(0.9),
            pick(1.0),
        )))
    }

    /// Render `frames` warm-up frames and leave them submitted. The caller
    /// awaits [`crate::web::debug_gpu_drain`] before starting a timed run, so a benchmark
    /// does not bill first-frame pipeline compilation to the first sample.
    pub fn debug_bench_warmup(&mut self, frames: u32) -> Result<(), JsError> {
        if !self.has_document() {
            return Ok(());
        }
        for i in 0..frames.max(1) {
            self.render_frame(i as f64 / 60.0)?;
        }
        Ok(())
    }

    /// Frame-dedup counters: how many ticks drew and how many found the same
    /// frame already on the canvas. Read-only, so a page can prove the skip is
    /// happening rather than trust it (`web/demos/perf-probe.html`). Named in
    /// snake_case like every other export here; a `js_name` rename would make
    /// the probe's optional call read as zero forever.
    pub fn debug_dedup_stats(&self) -> Result<JsValue, JsError> {
        let stats = crate::dto::DedupStatsDto {
            drawn: self.frames_drawn,
            deduped: self.frames_deduped,
        };
        serde_wasm_bindgen::to_value(&stats)
            .map_err(|e| JsError::new(&format!("failed to serialize dedup stats: {e}")))
    }

    pub fn scene_width(&self) -> u32 {
        self.dims.width
    }

    pub fn scene_height(&self) -> u32 {
        self.dims.height
    }
}

/// Monotonic milliseconds, the only clock wasm32-unknown-unknown has.
///
/// `std::time::Instant` is *not* implemented for this target — calling
/// `Instant::now()` traps with "time not implemented on this platform" — so the
/// frame timers read `performance.now()` through `web-sys`.
fn now_ms() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0)
}
