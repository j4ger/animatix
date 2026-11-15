//! Active timeline resolution for single-scene and multi-scene documents.
//!
//! These types replace ad hoc `document.timeline.as_ref()` + fallback-to-composition
//! patterns scattered across panels and handlers.

use animatix::composition::Composition;
use animatix::timeline::{SceneDimensions, Timeline};

/// Identifies which scene's timeline is being reviewed.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActiveSceneId {
    /// A single-scene document (no composition).
    SingleScene,
    /// A named scene inside a composition.
    Scene(String),
}

impl std::fmt::Display for ActiveSceneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActiveSceneId::SingleScene => write!(f, "single-scene"),
            ActiveSceneId::Scene(name) => write!(f, "scene '{}'", name),
        }
    }
}

/// Immutable reference to the active timeline, with scene context.
///
/// This is the single source of truth for "which timeline should I read from?"
/// Used by preview overlays, the Inspector, timeline review, and export.
#[derive(Clone)]
// `id`, `composition` and `scene_name` are the scene-resolution context this
// type exists to carry; the call sites today read `timeline` and `duration_s`.
#[allow(dead_code)]
pub struct ActiveTimelineRef<'a> {
    pub id: ActiveSceneId,
    pub timeline: &'a Timeline,
    pub composition: Option<&'a Composition>,
    pub scene_name: Option<&'a str>,
    pub duration_s: f64,
    pub dimensions: SceneDimensions,
}
