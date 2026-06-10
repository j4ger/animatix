//! Active timeline resolution for single-scene and multi-scene documents.
//!
//! These types replace ad hoc `document.timeline.as_ref()` + fallback-to-composition
//! patterns scattered across panels and handlers.

use animatix::composition::Composition;
use animatix::timeline::{SceneDimensions, Timeline};

/// Identifies which scene's timeline is the current editing target.
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

/// Immutable reference to the active editable timeline, with scene context.
///
/// This is the single source of truth for "which timeline should I read from?"
/// Used by preview overlays, inspector, timeline panel, drag handlers, etc.
#[derive(Clone)]
pub struct ActiveTimelineRef<'a> {
    pub id: ActiveSceneId,
    pub timeline: &'a Timeline,
    pub composition: Option<&'a Composition>,
    pub scene_name: Option<&'a str>,
    pub duration_s: f64,
    pub dimensions: SceneDimensions,
}

impl<'a> ActiveTimelineRef<'a> {
    pub fn scene_key(&self) -> &str {
        match &self.id {
            ActiveSceneId::SingleScene => "",
            ActiveSceneId::Scene(name) => name.as_str(),
        }
    }
}

/// Mutable reference to the active editable timeline, with scene context.
///
/// Used by mutation paths that need to modify the active scene's timeline
/// (e.g., in-memory preview during drag, layout reorder).
pub struct ActiveTimelineMut<'a> {
    pub id: ActiveSceneId,
    pub timeline: &'a mut Timeline,
    pub scene_name: Option<String>,
}
