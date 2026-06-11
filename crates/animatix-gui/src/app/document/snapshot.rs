//! Immutable document snapshot produced by a rebuild.
//!
//! Once published, a snapshot is never mutated. Consumers receive `Arc<DocumentSnapshot>`.

use animatix::composition::Composition;
use animatix::timeline::{SceneDimensions, Timeline, TimelineIndex};
use animatix_syntax::ast::Stmt;
use animatix_syntax::diagnostics::Diagnostic;
use animatix_syntax::module::{ActionTemplate, ComponentEntry, Namespace};
use animatix_syntax::source_index::SourceIndex;
use std::collections::HashMap;
use std::sync::Arc;

use crate::app::document::active_timeline::{ActiveSceneId, ActiveTimelineRef};
use crate::app::document::export_target::{ExportScope, ExportTargetRef};
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
#[derive(Clone)]
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
    pub module_actions: Arc<HashMap<String, ActionTemplate>>,
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
    /// Resolve the active editable timeline from this snapshot.
    #[allow(dead_code)] // Will be used by panels and export dialog once they read from DocumentSnapshot instead of DocumentSession.
    pub fn active_timeline(&self, active_scene: Option<&str>) -> Option<ActiveTimelineRef<'_>> {
        match &self.target {
            BuildTargetSnapshot::Timeline(timeline) => Some(ActiveTimelineRef {
                id: ActiveSceneId::SingleScene,
                timeline: timeline.as_ref(),
                composition: None,
                scene_name: None,
                duration_s: timeline.duration_seconds(),
                dimensions: self.scene_dimensions,
            }),
            BuildTargetSnapshot::Composition(composition) => {
                let _scene_name = active_scene
                    .and_then(|name| composition.scenes.get(name))
                    .or_else(|| {
                        composition
                            .declaration_order
                            .first()
                            .and_then(|name| composition.scenes.get(name))
                    })
                    .or_else(|| composition.scenes.values().next())?;
                // Find active scene by name, or first in declaration order
                let scene_entry = active_scene
                    .and_then(|name| composition.scenes.get_key_value(name))
                    .or_else(|| {
                        composition
                            .declaration_order
                            .first()
                            .and_then(|name| composition.scenes.get_key_value(name))
                    })
                    .or_else(|| composition.scenes.iter().next())?;

                let (actual_name, scene) = scene_entry;
                Some(ActiveTimelineRef {
                    id: ActiveSceneId::Scene(actual_name.clone()),
                    timeline: &scene.timeline,
                    composition: Some(composition.as_ref()),
                    scene_name: Some(actual_name),
                    duration_s: scene.duration_s.max(0.1),
                    dimensions: self.scene_dimensions,
                })
            },
            BuildTargetSnapshot::Empty => None,
        }
    }

    /// Resolve an export target from this snapshot.
    #[allow(dead_code)] // Will be used by panels and export dialog once they read from DocumentSnapshot instead of DocumentSession.
    pub fn export_target(
        &self,
        scope: ExportScope,
        active_scene: Option<&str>,
    ) -> Option<ExportTargetRef<'_>> {
        match scope {
            ExportScope::ActiveScene | ExportScope::Scene(_) => {
                let tr = self.active_timeline(active_scene)?;
                Some(ExportTargetRef::Timeline {
                    timeline: tr.timeline,
                    duration_s: tr.duration_s,
                    dimensions: tr.dimensions,
                })
            },
            ExportScope::WholeComposition => match &self.target {
                BuildTargetSnapshot::Composition(composition) => {
                    Some(ExportTargetRef::Composition {
                        composition: composition.as_ref(),
                        duration_s: composition.global_duration_s.max(0.1),
                        dimensions: self.scene_dimensions,
                    })
                },
                BuildTargetSnapshot::Timeline(timeline) => Some(ExportTargetRef::Timeline {
                    timeline: timeline.as_ref(),
                    duration_s: timeline.duration_seconds(),
                    dimensions: self.scene_dimensions,
                }),
                BuildTargetSnapshot::Empty => None,
            },
        }
    }

    /// Returns true if this snapshot has a renderable target.
    #[allow(dead_code)] // Will be used by panels and export dialog once they read from DocumentSnapshot instead of DocumentSession.
    pub fn has_renderable_target(&self) -> bool {
        matches!(
            self.target,
            BuildTargetSnapshot::Timeline(_) | BuildTargetSnapshot::Composition(_)
        )
    }
}
