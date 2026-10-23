//! Document session: loading, rebuilding, saving, and querying the loaded program.
//!
//! The implementation is split by domain; this module owns the session types and
//! re-exports the public entry points so `crate::document::*` paths stay stable.

mod audio;
mod load;
mod query;
mod rebuild;
mod save;
mod timing;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;

use animatix::composition::Composition;
use animatix::timeline::{SceneDimensions, Timeline, TimelineIndex};
use animatix_analyzer::ExtensionManifest;
use animatix_syntax::ast::Stmt;
use animatix_syntax::diagnostics::Diagnostic;
use animatix_syntax::module::{ComponentEntry, FnTemplate, ModuleGraph, Namespace};
use animatix_syntax::source_index::SourceIndex;

pub use load::LoadedProgramResult;
pub(crate) use load::extension_manifest_for_path;
pub use save::default_file_path;
pub(crate) use save::{clear_recovery, recovery_is_newer, recovery_path, write_recovery};
pub use timing::{timeline_duration_seconds, timeline_keyframe_times_s};

pub struct DocumentSession {
    pub file_path: PathBuf,
    pub source_text: String,
    pub raw_statements: Option<Vec<Stmt>>,
    pub expanded_statements: Option<Vec<Stmt>>,
    pub namespaces: HashMap<String, Namespace>,
    pub source_index: Option<SourceIndex>,
    pub timeline: Option<Timeline>,
    pub composition: Option<Composition>,
    pub active_scene: Option<String>,
    pub diagnostics: Vec<Diagnostic>,
    pub last_rebuild_error: Option<String>,
    pub is_dirty: bool,
    pub duration_s: f64,
    pub scene_dimensions: SceneDimensions,
    /// Per-document extension context used for all timeline rebuilds.
    pub extension_context: std::sync::Arc<animatix::extension_context::ExtensionContext>,
    /// Analyzer metadata merged from `.amx-plugin.toml` files beside the document.
    pub extension_manifest: ExtensionManifest,
    /// Optional workspace root used for relative plugin/asset resolution.
    pub workspace_root: Option<PathBuf>,
    /// Plugin context generation used to discard stale background rebuilds.
    pub plugin_epoch: u64,
    /// Bi-directional timeline index mapping source lines to times and vice versa.
    pub timeline_index: TimelineIndex,
    /// Set of 0-indexed line numbers that contain keyframe declarations.
    /// Derived from timeline_index for editor decorations.
    pub keyframe_lines: Vec<usize>,
    /// Component registry from the last successful program load.
    pub components: HashMap<String, ComponentEntry>,
    /// Module-scoped actions from the last successful program load.
    pub module_fns: HashMap<String, FnTemplate>,
    /// Hash of source_text at last successful rebuild — skip rebuild if unchanged.
    last_source_hash: u64,
    /// Hash of component registry at last successful rebuild — skip expansion if unchanged.
    last_component_hash: u64,
    /// Cached expanded statements — reused when components haven't changed.
    cached_expanded: Option<Vec<Stmt>>,
    /// Cached module graph — preserves parsed imports across rebuilds.
    cached_module_graph: Option<ModuleGraph>,
}
