//! Document core: the session that owns the loaded program, plus the GUI-side
//! state derived from it.
//!
//! `DocumentSession` is the load/rebuild/save/query surface; the sibling
//! modules own the pieces the GUI adds on top of it (active-timeline
//! abstraction, undo history, snapshot/diff, background workers, plugins).
//! They live here together because the two halves depend on each other — the
//! session already drives `rebuild_output`, so a separate top-level `document`
//! module would only be a cycle across a module boundary.

mod audio;
mod load;
mod query;
mod save;
mod session_rebuild;
mod timing;

#[cfg(test)]
mod tests;

pub(crate) mod active_timeline;
pub(crate) mod export_target;
pub(crate) mod history;
pub(crate) mod load_worker;
pub(crate) mod plugins;
pub(crate) mod rebuild;
pub(crate) mod rebuild_output;
pub(crate) mod snapshot;
pub(crate) mod source_change;
pub(crate) mod timeline_diff;
pub(crate) mod version;

use std::collections::HashMap;
use std::path::PathBuf;

use animatix::composition::Composition;
use animatix::timeline::{SceneDimensions, Timeline, TimelineIndex};
use animatix_analyzer::ExtensionManifest;
use animatix_syntax::ast::Stmt;
use animatix_syntax::diagnostics::Diagnostic;
use animatix_syntax::module::{ComponentEntry, FnTemplate, ModuleGraph, Namespace};
use animatix_syntax::source_index::SourceIndex;

pub(crate) use load::extension_manifest_for_path;
pub(crate) use save::default_file_path;
pub(crate) use save::{clear_recovery, recovery_is_newer, recovery_path, write_recovery};
pub(crate) use timing::{review_scene_at_time, timeline_keyframe_times_s};

/// The loaded `.amx` document: source text, the program built from it, and the
/// diagnostics of the last attempt.
pub struct DocumentSession {
    /// Unique in-process identity for derived media caches, including same-file reopen.
    waveform_identity: u64,
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
