//! Source → diagnostics → buildable document, shared by the wasm driver and
//! native tests.
//!
//! This is the web analogue of the GUI's `DocumentSession::rebuild` pipeline,
//! minus GUI-specific caching: parse/typecheck/expand through the module
//! system in `SourcesOnly` mode (no disk access — the editor owns the source
//! text), then build through the font-context-aware engine entry points.

mod build;
mod library;
mod markers;
#[cfg(test)]
mod tests;

pub use build::{
    BuiltDocument, build_document, build_document_with_assets, build_document_with_modules,
};
pub use library::{BUILD_ID, ENTRY_PATH, bundled_library, identity_digest};

/// Duration floor for scrubbing/looping behaviour, mirroring the GUI.
const MIN_DURATION_S: f64 = 0.1;
