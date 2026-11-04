//! Minimal differential perf demo: the same synthetic vector scene rendered by
//! vello compute (WebGPU) and vello_hybrid (WebGL2), from one wasm module.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

mod bench;
mod compute;
mod hybrid;
mod scene;

#[wasm_bindgen]
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}

/// Render `scene` (`"light"` / `"dense"`) with the compute backend and return a
/// `BenchResult` object.
#[wasm_bindgen]
pub async fn run_compute(
    canvas: web_sys::HtmlCanvasElement,
    scene: String,
    width: u32,
    height: u32,
    frames: u32,
    warmup: u32,
    reps: u32,
) -> Result<JsValue, JsError> {
    console_error_panic_hook::set_once();
    let r = compute::run(canvas, &scene, width, height, frames, warmup, reps)
        .await
        .map_err(|e| JsError::new(&e))?;
    serde_wasm_bindgen::to_value(&r).map_err(|e| JsError::new(&e.to_string()))
}

/// Render the same scene with the WebGL2 hybrid backend.
#[wasm_bindgen]
pub fn run_hybrid(
    canvas: web_sys::HtmlCanvasElement,
    scene: String,
    width: u32,
    height: u32,
    frames: u32,
    warmup: u32,
    reps: u32,
) -> Result<JsValue, JsError> {
    console_error_panic_hook::set_once();
    let r = hybrid::run(canvas, &scene, width, height, frames, warmup, reps)
        .map_err(|e| JsError::new(&e))?;
    serde_wasm_bindgen::to_value(&r).map_err(|e| JsError::new(&e.to_string()))
}
