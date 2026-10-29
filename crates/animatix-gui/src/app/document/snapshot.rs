//! Immutable document snapshot produced by a rebuild.
//!
//! Once published, a snapshot is never mutated. Consumers receive `Arc<DocumentSnapshot>`.

use std::sync::Arc;

use animatix::composition::Composition;
use animatix::timeline::{SceneDimensions, Timeline};

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
/// `DocumentStore` publishes the latest snapshot and retains the last
/// renderable one for preview fallback.
#[derive(Clone)]
pub struct DocumentSnapshot {
    pub generation: DocumentGeneration,
    pub source_epoch: SourceEpoch,
    pub source_hash: SourceHash,
    pub status: SnapshotStatus,
    pub target: BuildTargetSnapshot,
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

    /// Pause points in seconds from this snapshot's target.
    pub fn pause_points_s(&self) -> Vec<f64> {
        match &self.target {
            BuildTargetSnapshot::Composition(comp) => {
                comp.pause_points().iter().map(|&ms| ms as f64 / 1000.0).collect()
            },
            BuildTargetSnapshot::Timeline(tl) => {
                tl.pause_points().iter().map(|&ms| ms as f64 / 1000.0).collect()
            },
            BuildTargetSnapshot::Empty => Vec::new(),
        }
    }
}
