use std::sync::Arc;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::geometry::SceneAnchor;
use crate::renderer::types::VelloPath;
use crate::timeline::kurbo_shapes::KurboShape;
use crate::timeline::property_track::PropertyTrack;
use crate::timeline::shapes::ShapeType;

// ─────────────────────────────────────────────────────────────
// ShapeTracks sub-struct
// ─────────────────────────────────────────────────────────────

/// Sub-struct holding all shape-related property tracks.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ShapeTracks {
    /// Specific shape geometry type.
    pub shape_type: Option<PropertyTrack<ShapeType>>,
    /// Line start point.
    pub line_from: Option<PropertyTrack<[f32; 2]>>,
    /// Line end point.
    pub line_to: Option<PropertyTrack<[f32; 2]>>,
    /// Arrow head size.
    pub head_size: Option<PropertyTrack<f32>>,
    /// Arc start and end angles.
    pub arc_angles: Option<PropertyTrack<[f32; 2]>>,
    /// Corner rounding radius for `Rect`, in scene pixels (0 = square corners).
    pub corner_radius: Option<PropertyTrack<f32>>,
    /// Polygon vertex list.
    pub points: Option<PropertyTrack<Vec<[f32; 2]>>>,
    /// Path command string (e.g. SVG path data).
    pub commands: Option<PropertyTrack<String>>,
    /// Pre-built vector paths for shape rendering.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub vector_paths: Option<PropertyTrack<Vec<VelloPath>>>,
    /// Memoized shape→path conversion for the render path (PF-6): the key is
    /// the sampled `KurboShape` (self-validating — any geometry change misses
    /// and rebuilds; the fixed default tolerance is part of the conversion),
    /// the value is the shared `Arc<BezPath>` the render wraps with fresh
    /// style. Never serialized (recomputed on demand).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub shape_path_memo: std::cell::RefCell<Option<(KurboShape, Arc<kurbo::BezPath>)>>,
    /// Shared memo of the last `evaluate_vector_paths` result (PF-4/PF-6:
    /// a static track re-clones its whole path `Vec` every frame; this shares
    /// it as an `Arc` instead). Guarded by [`Self::vector_paths_epoch`], which
    /// `invalidate_frame_cache` bumps because every keyframe/assignment/action
    /// mutation funnels through it. Never serialized (recomputed on demand).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub vector_paths_memo: std::cell::RefCell<Option<(u64, Arc<Vec<VelloPath>>)>>,
    /// Mutation counter matched with the memo; see [`Self::vector_paths_memo`].
    #[cfg_attr(feature = "serde", serde(skip))]
    pub vector_paths_epoch: std::cell::Cell<u64>,
    /// Memoized "does this track's `shape_type` value ever change" verdict,
    /// guarding the per-frame primitive switch in
    /// `AnimationTrack::render_type_name` (cross-type morphs): a constant
    /// shape-type track pays one `Cell` read per frame instead of re-scanning
    /// the keyframe map. `invalidate_frame_cache` clears it in the same funnel
    /// that bumps [`Self::vector_paths_epoch`]. Never serialized.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub shape_type_switches: std::cell::Cell<Option<bool>>,

    // ── Actor-anchor refs for `from`/`to` (G6) ──
    /// When set, `line_from` / `from` is resolved each frame from this
    /// actor's world-space anchor point instead of the PropertyTrack.
    /// `(actor_label, anchor)`.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub from_anchor: Option<(String, SceneAnchor)>,
    /// Same as `from_anchor` but for the `to` / `line_to` endpoint.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub to_anchor: Option<(String, SceneAnchor)>,
}
