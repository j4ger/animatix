//! Read-only inventory for bounded text/font caches and this Wasm instance.

use wasm_bindgen::prelude::*;

#[derive(serde::Serialize)]
struct ResourceStats {
    text_compile_entries: usize,
    grouped_text_compile_entries: usize,
    font_context_pool_entries: usize,
    font_context_active_entries: usize,
    wasm_memory_bytes: Option<usize>,
}

/// Report current resource inventories. This does not retain cache values or
/// measure GPU allocations; Wasm memory is the instance's linear-memory size.
#[wasm_bindgen]
pub fn debug_resource_stats() -> Result<JsValue, JsError> {
    let (pool_entries, active_entries) = super::prepared::font_context_pool_stats();
    let wasm_memory_bytes = js_sys::Reflect::get(&wasm_bindgen::memory(), &"buffer".into())
        .ok()
        .and_then(|buffer| js_sys::Reflect::get(&buffer, &"byteLength".into()).ok())
        .and_then(|length| length.as_f64())
        .map(|length| length as usize);
    let stats = ResourceStats {
        text_compile_entries: animatix::renderer::text::text_compile_cache_len(),
        grouped_text_compile_entries: animatix::renderer::text::grouped_text_compile_cache_len(),
        font_context_pool_entries: pool_entries,
        font_context_active_entries: active_entries,
        wasm_memory_bytes,
    };
    serde_wasm_bindgen::to_value(&stats).map_err(|error| JsError::new(&error.to_string()))
}
