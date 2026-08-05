//! Export target resolution for single-scene and multi-scene documents.
//!
//! These types replace ad hoc `document.timeline.clone()` patterns in the
//! export dialog by providing a unified resolution API.

use animatix::composition::Composition;
use animatix::timeline::{SceneDimensions, Timeline};

/// Which part of the document to export.
#[derive(Debug, Clone, PartialEq)]
pub enum ExportScope {
    ActiveScene,
    WholeComposition,
    Scene(String),
}

/// Borrowed export target with duration/dimensions.
#[derive(Clone, Copy)]
pub enum ExportTargetRef<'a> {
    Timeline {
        timeline: &'a Timeline,
        duration_s: f64,
        dimensions: SceneDimensions,
    },
    Composition {
        composition: &'a Composition,
        duration_s: f64,
        dimensions: SceneDimensions,
    },
}

impl<'a> ExportTargetRef<'a> {
    pub fn duration_s(&self) -> f64 {
        match self {
            ExportTargetRef::Timeline { duration_s, .. } => *duration_s,
            ExportTargetRef::Composition { duration_s, .. } => *duration_s,
        }
    }
}

/// Owned export target for background export threads.
///
/// Timeline is boxed to keep this enum small and to avoid moving a large,
/// non-Sync type into the worker closure. Renderer calls that require owned
/// timelines clone once inside the worker.
#[derive(Clone)]
pub enum ExportTargetOwned {
    Timeline(Box<Timeline>),
    Composition(Box<Composition>),
}
