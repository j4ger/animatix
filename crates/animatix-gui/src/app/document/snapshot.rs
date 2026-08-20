//! Immutable document snapshot produced by a rebuild.
//!
//! Once published, a snapshot is never mutated. Consumers receive `Arc<DocumentSnapshot>`.

use std::collections::HashMap;
use std::sync::Arc;

use animatix::composition::Composition;
use animatix::timeline::{SceneDimensions, Timeline, TimelineIndex};
use animatix_syntax::ast::Stmt;
use animatix_syntax::diagnostics::Diagnostic;
use animatix_syntax::module::{ComponentEntry, FnTemplate, Namespace};
use animatix_syntax::source_index::SourceIndex;

use crate::app::document::version::{DocumentGeneration, SourceEpoch, SourceHash};

/// Status of a snapshot relative to the current source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotStatus {
    Clean,
    Stale { current_source_epoch: SourceEpoch },
    Failed { error: &'static str },
}

/// The build target of a snapshot: either a single timeline or a composition.
#[derive(Clone)]
pub enum BuildTargetSnapshot {
    Empty,
    Timeline(Arc<Timeline>),
    Composition(Arc<Composition>),
}

/// An immutable snapshot of all derived document state after a rebuild.
///
/// This is the single source of truth for all non-source document data.
/// `DocumentStore` holds the latest snapshot (current) and the last good one.
///
/// Fields are written during construction but not yet read individually —
/// panels currently consume `DocumentSession` directly. The snapshot API
/// exists for the panel-migration path (see `panels/*_model.rs`).
#[derive(Clone)]
#[allow(dead_code)] // DocumentSnapshot is the panel-migration target; not yet consumed by panels
pub struct DocumentSnapshot {
    pub generation: DocumentGeneration,
    pub source_epoch: SourceEpoch,
    pub source_hash: SourceHash,
    pub status: SnapshotStatus,

    // AST and module data
    pub raw_statements: Option<Arc<Vec<Stmt>>>,
    pub expanded_statements: Option<Arc<Vec<Stmt>>>,
    pub namespaces: Arc<HashMap<String, Namespace>>,
    pub components: Arc<HashMap<String, ComponentEntry>>,
    pub module_fns: Arc<HashMap<String, FnTemplate>>,
    pub source_index: Option<SourceIndex>,

    // Build target
    pub target: BuildTargetSnapshot,

    // Derived indexes
    pub timeline_index: TimelineIndex,
    pub keyframe_lines: Vec<usize>,

    // Document metadata
    pub diagnostics: Arc<Vec<Diagnostic>>,
    pub duration_s: f64,
    pub scene_dimensions: SceneDimensions,
}

impl DocumentSnapshot {
    /// Returns true if this snapshot has a renderable target.
    pub fn has_renderable_target(&self) -> bool {
        matches!(
            self.target,
            BuildTargetSnapshot::Timeline(_) | BuildTargetSnapshot::Composition(_)
        )
    }
}
