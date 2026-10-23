//! Build identity exported to the JS shell: crate version, commit, and library digest.

use wasm_bindgen::prelude::*;

/// Identifies the running build from the JS side (stale-artifact checks).
///
/// A digest of the version and the embedded `.amx` library, computed at compile
/// time — see [`crate::host::identity_digest`]. Kept as a `u32` so
/// `web/demos/perf-probe.html` needs no change.
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
/// should fail. Deliberately *not* folded into [`build_id`]: reading `.git` from a
/// build script would make `cargo test --workspace` non-hermetic and two identical
/// checkouts would produce different artifacts.
#[wasm_bindgen]
pub fn engine_commit() -> String {
    option_env!("ANIMATIX_GIT_SHA").unwrap_or("").to_string()
}
