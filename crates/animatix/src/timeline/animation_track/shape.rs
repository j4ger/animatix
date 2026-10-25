use std::sync::Arc;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::geometry::SceneAnchor;
use super::style::StyleTracks;
use crate::renderer::types::VelloPath;
use crate::timeline::kurbo_shapes::KurboShape;
use crate::timeline::morph::MorphOptions;
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
    /// `Self::render_type_name` (cross-type morphs): a constant
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

impl ShapeTracks {
    /// The primitive type name that renders at `time_ms`.
    ///
    /// Normally the entity's actor type, but a same-label re-declaration that
    /// changes the actor type — the morph syntax — leaves the identity
    /// holding the LAST type while the `shape_type` keyframes carry the
    /// switch. Resolving the primitive from the identity alone made the
    /// target shape own the whole timeline: a `Rect` re-declared `Ellipse`
    /// drew as a circle from birth (the Ellipse primitive builds its state
    /// from the shared size track, so every pre-morph frame rounded too).
    /// Inside the vector-shape family the per-frame `shape_type` value picks
    /// the primitive instead; every other identity keeps its type, including
    /// non-shape actors whose plan carries only a vestigial constant
    /// `shape_type` (inserted by the generic morph build path).
    pub fn render_type_name<'a>(&self, actor_type: &'a str, time_ms: u64) -> &'a str {
        const VECTOR_SHAPES: [&str; 6] = ["Rect", "Ellipse", "Line", "Arrow", "Polygon", "Path"];
        if !VECTOR_SHAPES.contains(&actor_type) {
            return actor_type;
        }
        let Some(shape_track) = self.shape_type.as_ref() else {
            return actor_type;
        };
        // Only a track whose value actually changes may switch the primitive.
        // The verdict is memoized (`shape_type_switches`, cleared by
        // `invalidate_frame_cache`): constant tracks — every non-morph
        // vector shape — pay one `Cell` read per frame instead of re-walking
        // the keyframe map.
        let switches = match self.shape_type_switches.get() {
            Some(verdict) => verdict,
            None => {
                let mut values = shape_track.keyframes.values();
                let Some((first, _)) = values.next() else {
                    self.shape_type_switches.set(Some(false));
                    return actor_type;
                };
                let changed = values.any(|(v, _)| v != first);
                self.shape_type_switches.set(Some(changed));
                changed
            },
        };
        if !switches {
            return actor_type;
        }
        // A `switches == true` verdict implies a non-empty map; emptiness
        // returns through `invalidate_frame_cache`'s memo clear, so this
        // guard is belt-and-braces, not a live path.
        if shape_track.keyframes.is_empty() {
            return actor_type;
        }
        let at: ShapeType = shape_track.evaluate(time_ms);
        let name = match at {
            ShapeType::Rect => "Rect",
            ShapeType::Ellipse => "Ellipse",
            ShapeType::Line => "Line",
            ShapeType::Polygon => "Polygon",
            ShapeType::Path => "Path",
            ShapeType::Arrow => "Arrow",
            _ => return actor_type,
        };
        if name != actor_type { name } else { actor_type }
    }

    /// Evaluate vector paths at `time_ms`, applying morphing if configured.
    ///
    /// PF-4: when the actor has no `vector_paths` track (every geometric shape
    /// whose path the primitive derives from `shape_type` + size), the previous
    /// code constructed two throwaway `PropertyTrack`s per call, fed them
    /// through the morph evaluator, and got an empty `Vec` back — measurable
    /// per-node overhead on the frame path. Returning early is exactly
    /// equivalent: the empty track's evaluation is `default_value.clone()`,
    /// which is an empty `Vec`.
    ///
    /// PF-6 (2026-09-04): the result is returned as a shared `Arc` — when the
    /// track is time-independent (no keyframes, or `time_ms` past the last
    /// keyframe, where the evaluator returns the frozen `last_value`), the
    /// value is memoized and re-clones become refcount bumps.
    /// `alloc_driver` measured ~61 allocations / ~21 KB per frame on a
    /// 60-actor scene from every static shape actor cloning its whole path
    /// list per frame. The memo is guarded by `vector_paths_epoch` (bumped by
    /// `invalidate_frame_cache`, the funnel every mutation goes through) and
    /// must never observe a track mutated without that bump.
    pub fn evaluate_vector_paths(&self, style: &StyleTracks, time_ms: u64) -> Arc<Vec<VelloPath>> {
        if let Some(paths_track) = self.vector_paths.as_ref() {
            let constant = paths_track
                .keyframes
                .keys()
                .next_back()
                .map(|&last| time_ms > last)
                .unwrap_or(true);
            if constant {
                let epoch = self.vector_paths_epoch.get();
                if let Some((memo_epoch, shared)) = self.vector_paths_memo.borrow().as_ref() {
                    if *memo_epoch == epoch {
                        return Arc::clone(shared);
                    }
                }
                let computed = Arc::new(self.evaluate_vector_paths_value(style, time_ms));
                *self.vector_paths_memo.borrow_mut() = Some((epoch, Arc::clone(&computed)));
                return computed;
            }
        }
        Arc::new(self.evaluate_vector_paths_value(style, time_ms))
    }

    /// Unmemoized single evaluation of the vector-path track.
    ///
    /// Build-time callers (`insert_start_keyframes`, assignment rewrites) use
    /// this so they observe values they have just written instead of a memo
    /// captured before the write.
    pub(crate) fn evaluate_vector_paths_value(
        &self,
        style: &StyleTracks,
        time_ms: u64,
    ) -> Vec<VelloPath> {
        let Some(paths_track) = self.vector_paths.as_ref() else {
            return Vec::new();
        };
        let default_morph = PropertyTrack::new(MorphOptions::default());
        let morph_track = style.morph_options.as_ref().unwrap_or(&default_morph);
        crate::timeline::morph::evaluate_paths_with_options(
            paths_track,
            morph_track,
            time_ms,
            crate::timeline::morph::interpolate_vello_paths,
        )
    }

    /// Memoized `KurboShape → BezPath` conversion for the render path (PF-6).
    ///
    /// `render()` rebuilds identical geometry every frame for any actor whose
    /// geometry is static while a sibling animates (forcing a frame-cache
    /// miss). The memo compares the sampled `KurboShape` — an allocation-free
    /// equality check — and shares the converted path as an `Arc`, so a
    /// 60-actor scene with one animated actor stops rebuilding 59 identical
    /// `Rect` paths per frame (alloc_driver 2026-09-04: ~70 blocks /
    /// 23.5 KB per frame). The shape value fully determines the path (fixed
    /// default tolerance), so the memo is self-validating and needs no epoch.
    pub fn shape_path_memoized(&self, shape: &KurboShape) -> Arc<kurbo::BezPath> {
        if let Some((cached_shape, shared)) = self.shape_path_memo.borrow().as_ref() {
            if cached_shape == shape {
                return Arc::clone(shared);
            }
        }
        let built = Arc::new(shape.to_path_default());
        *self.shape_path_memo.borrow_mut() = Some((shape.clone(), Arc::clone(&built)));
        built
    }

    /// Latest keyframe across the shape property tracks.
    pub fn max_keyframe_time(&self) -> Option<u64> {
        macro_rules! latest {
            ($($field:ident),* $(,)?) => {
                [$(self.$field.as_ref().and_then(|t| t.last_keyframe_time())),*]
                    .into_iter()
                    .flatten()
                    .max()
            };
        }
        latest!(
            shape_type,
            line_from,
            line_to,
            head_size,
            arc_angles,
            corner_radius,
            points,
            commands,
            vector_paths,
        )
    }

    /// Whether any shape property track has animated keyframes.
    pub fn has_any_keyframes(&self) -> bool {
        macro_rules! any_animated {
            ($($field:ident),* $(,)?) => {
                false $(|| self.$field.as_ref().is_some_and(|t| !t.is_effectively_static()))*
            };
        }
        any_animated!(
            shape_type,
            line_from,
            line_to,
            head_size,
            arc_angles,
            corner_radius,
            points,
            commands,
            vector_paths,
        )
    }
}
