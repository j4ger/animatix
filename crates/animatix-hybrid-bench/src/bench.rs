//! Shared benchmark result shape and timing helpers.

use serde::Serialize;

/// What one backend/one configuration measured. Times are per frame in ms.
/// `submit_ms` is CPU time spent building + encoding + submitting the frames
/// (before the GPU sync); `gpu_ms = per_frame_ms - submit_ms` is the rest of
/// the wall time after the sync. `clear_only_ms` renders an empty scene and
/// isolates the backend's fixed per-frame overhead.
#[derive(Serialize, Clone)]
pub struct BenchResult {
    pub backend: String,
    pub scene: String,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub reps: u32,
    pub per_frame_ms: f64,
    pub submit_ms: f64,
    pub gpu_ms: f64,
    pub clear_only_ms: f64,
}

/// `performance.now()` in milliseconds. Never `Instant` — that traps on wasm.
pub fn now_ms() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0)
}
