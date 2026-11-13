//! Debug-only cross-target text snapshot entry point.

use wasm_bindgen::prelude::*;

/// Compile plain and grouped text using only the provided font byte arrays.
#[wasm_bindgen]
pub fn debug_text_probe(request: JsValue, font_bytes: js_sys::Array) -> Result<JsValue, JsValue> {
    let request: crate::host::TextProbeRequest = serde_wasm_bindgen::from_value(request)
        .map_err(|error| JsValue::from_str(&format!("invalid text probe request: {error}")))?;
    let fonts = font_bytes
        .iter()
        .map(|value| js_sys::Uint8Array::new(&value).to_vec())
        .collect::<Vec<_>>();
    let output =
        crate::host::probe_text(&request, &fonts).map_err(|error| JsValue::from_str(&error))?;
    serde_wasm_bindgen::to_value(&output)
        .map_err(|error| JsValue::from_str(&format!("serialize text probe result: {error}")))
}
