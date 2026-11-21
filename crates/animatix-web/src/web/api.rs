//! Build identity exported to the JS shell: crate version, commit, and SDK identity.

use wasm_bindgen::prelude::*;

/// Identifies the running build from the JS side (stale-artifact checks).
///
/// Compact diagnostic hash of the build-supplied SDK identity. Integrity uses
/// the full `runtime_build_id()` and manifest hashes, not this value.
#[wasm_bindgen]
pub fn build_id() -> u32 {
    crate::host::BUILD_ID
}

/// The crate version this bundle was built from, for a human reading a report.
#[wasm_bindgen]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// The commit this bundle was built from, when the build was told.
///
/// `option_env!` so an unset `ANIMATIX_GIT_SHA` is an empty string rather than a
/// compile error — a release workflow sets it, a local build does not, and neither
/// should fail. The SDK build supplies its identity explicitly; reading `.git` from a
/// build script would make `cargo test --workspace` non-hermetic and two identical
/// checkouts would produce different artifacts.
#[wasm_bindgen]
pub fn engine_commit() -> String {
    option_env!("ANIMATIX_GIT_SHA").unwrap_or("").to_string()
}

/// Classify draft AMX text as UTF-16 ranges without building a scene.
#[wasm_bindgen]
pub fn highlight_source(source: &str) -> Result<JsValue, JsError> {
    serde_wasm_bindgen::to_value(&crate::host::highlight_source(source))
        .map_err(|error| JsError::new(&error.to_string()))
}
