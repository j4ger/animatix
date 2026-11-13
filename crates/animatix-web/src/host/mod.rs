//! Source → diagnostics → buildable document, shared by the wasm driver and
//! native tests.
//!
//! This is the web analogue of the GUI's `DocumentSession::rebuild` pipeline,
//! minus GUI-specific caching: parse/typecheck/expand through the module
//! system in `SourcesOnly` mode (no disk access — the editor owns the source
//! text), then build through the font-context-aware engine entry points.

mod build;
mod features;
mod library;
mod markers;
mod prepare;
mod requirements;
mod resources;
#[cfg(test)]
mod tests;
mod text_probe;
mod transcripts;
/// Public because the wasm boundary renders page JSON into expression text
/// with it (`web::assets::set_vars`); the other modules here serve `host` only.
pub mod vars;

pub use build::{
    BuiltDocument, build_document, build_document_with_assets, build_document_with_modules,
    build_document_with_vars, build_prepared,
};
pub use features::required_features;
pub use library::{BUILD_ID, ENTRY_PATH, bundled_library, identity_digest};
pub use prepare::{
    AnalysisDto, AssetRequirement, FontRequirements, PreparedProgram, prepare_document,
};
pub use text_probe::{
    GroupedProbeCase, TextProbeCase, TextProbeOutput, TextProbeRequest, probe_text,
};

/// Duration floor for scrubbing/looping behaviour, mirroring the GUI.
const MIN_DURATION_S: f64 = 0.1;
