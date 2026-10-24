//! Per-actor keyframed property tracks.
//!
//! Split out of `dispatch.rs`; the field-dispatch layer stays there.

use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::dispatch::property_keyframe_times;

use super::actor_caps::ActorCaps;
use super::animation_track::{
    GeometryTracks, HighlightTracks, ShapeTracks, StyleTracks, TextTracks,
};
use super::entity::EntityId;
use super::kurbo_shapes::KurboShape;
use super::morph;
use super::property_track::{PropertyTrack, TrackAccessor};
use super::shapes::ShapeType;
use crate::renderer::types::{TextPath, VelloPath};
use crate::timeline::morph::MorphOptions;
pub use crate::timeline::property_engine::PropertyValue;

// ─────────────────────────────────────────────────────────────
// AnimationTrack
// ─────────────────────────────────────────────────────────────

/// Per-actor container for all keyframed property tracks.
///
/// Every actor in the scene graph has exactly one `AnimationTrack`.  The track
/// stores typed optional property tracks (geometry, style, filter, shape, text,
/// highlight tiers), along with metadata such as the derived capability
/// projection `caps` and `visible`.
///
/// The scene graph is *not* stored here: parent/children links live in
/// [`crate::timeline::world::SceneHierarchyComponent`] (see `docs/ecs.md`), so
/// `world.hierarchy(id)` is the single source of truth for the hierarchy.
///
/// Primitive-specific state lives in exactly one place: the
/// [`crate::timeline::world::World::payload`] component. An actor carries at
/// most one payload variant — a plot actor has a
/// [`crate::primitives::PlotPayload`], an `Svg` actor an
/// [`crate::primitives::SvgPayload`], an `Image` actor an
/// [`crate::primitives::ImagePayload`], and everything else
/// [`crate::primitives::PrimitivePayload::None`]. Read and write through the
/// entity-id keyed `World` accessors (`procedural_plot`, `func_transitions`,
/// `svg_paths`, `image_track`, …); the `ensure_*_payload` forms *replace* the
/// variant, so only call them from the primitive family that owns it.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct AnimationTrack {
    // ── Identity / metadata ──
    /// Strongly-typed dense entity ID for this actor in the timeline.
    #[cfg_attr(feature = "serde", serde(default))]
    pub id: EntityId,
    /// Human-readable identifier for the actor.
    pub label: String,
    /// Capability projection of this actor's primitive kind, derived once at
    /// identity time.
    pub caps: ActorCaps,
    /// Source type name for this actor's primitive (e.g. `"Rect"`, `"Text"`,
    /// or an extension's type name). Required and non-optional: it is the
    /// single registry key the frame path resolves the primitive with, so no
    /// lookup needs a kind fallback. Empty only across a legacy deserialization
    /// boundary, which [`Self::normalize_identity`] resolves once on load.
    #[cfg_attr(feature = "serde", serde(default))]
    pub actor_type: String,
    /// Registry-derived property plan for this actor.
    ///
    /// Built after the actor identity is known; frame-time extension paths can
    /// use this instead of string lookups.
    pub property_plan: super::plan::PropertyPlan,
    /// Dense index into the per-frame precise-bounds table (PF-6 slot-id
    /// design). Stamped by `EvalCaches::ensure_bounds_registry` in sorted
    /// `tracks` order and invalidated via `invalidate_frame_cache`; the
    /// per-node bounds write is then a `Vec` store with no string key.
    /// `u32::MAX` until first stamped.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) bounds_slot: std::cell::Cell<u32>,
    /// Memoized single-command output for the vector-shape primitives
    /// (PF-6 round 8; see `primitives::ShapeCommandMemo`). Keyed on
    /// `(vector_paths_epoch, style, state)` — every track mutation funnels
    /// through `invalidate_frame_cache`, which bumps the epoch. Owned-Vec
    /// borrow protocol: `evaluate_shape_render` takes the cached `Vec` on a
    /// key match, the frame consumer recycles it after encoding. Boxed to
    /// keep the per-track inline footprint at one pointer — the memo
    /// payload is large, and `env_50/100/200` iterate every track per frame
    /// (an inline struct measurably regressed them via cache pressure).
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) shape_command_memo: std::cell::RefCell<Box<crate::primitives::ShapeCommandMemo>>,
    /// Bounds handed over by a shape-command memo hit (PF-4 scoped item):
    /// `begin_shape_commands` clears it before primitive evaluation, a memo
    /// hit offers the build-time-computed bounds, and the frame consumer
    /// takes them instead of re-unioning `cmd.local_bounds` per node.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) pending_shape_command_bounds: std::cell::Cell<Option<Option<kurbo::Rect>>>,

    // ── Geometry tier (sub-struct) ──
    /// Geometry property tracks (position, size, rotation, scale, etc.).
    pub geometry: GeometryTracks,

    // ── Style tier (sub-struct) ──
    /// Style property tracks (color, opacity, stroke, line_cap, line_join, morph_options).
    pub style: StyleTracks,

    // ── Shape tier (sub-struct) ──
    /// Shape property tracks (shape_type, line_from, line_to, etc.).
    pub shape: ShapeTracks,

    // ── Text tier (sub-struct) ──
    /// Text property tracks (text_content, font_family, font_size, etc.).
    pub text: TextTracks,

    // ── Tagged union tracks ──
    /// Generic tagged union property tracks, keyed by canonical property name.
    pub tagged_tracks: HashMap<String, Option<PropertyTrack<PropertyValue>>>,

    // ── Highlight tier (sub-struct) ──
    /// Highlight property tracks (color, opacity, padding, radius, blend).
    pub highlight: HighlightTracks,
}

impl AnimationTrack {
    /// Create a new empty animation track for an actor of primitive type
    /// `actor_type` (e.g. `"Rect"`, `"Text"`, or an extension's type name).
    ///
    /// The primitive identity is required at construction: capabilities are
    /// derived from it, so no later lookup needs a fallback for a track that
    /// was built without a type name.
    pub fn new(label: String, actor_type: impl Into<String>) -> Self {
        let actor_type = actor_type.into();
        let caps = animatix_std::caps_for_type(&actor_type).unwrap_or_default();
        Self {
            // Identity
            id: EntityId::INVALID,
            label: label.clone(),
            caps,
            actor_type,
            property_plan: super::plan::PropertyPlan::default(),
            bounds_slot: std::cell::Cell::new(u32::MAX),
            shape_command_memo: std::cell::RefCell::new(Box::new(
                crate::primitives::ShapeCommandMemo::default(),
            )),
            pending_shape_command_bounds: std::cell::Cell::new(None),

            // Geometry tier (sub-struct)
            geometry: GeometryTracks::default(),

            // Style tier (sub-struct)
            style: StyleTracks::default(),

            // Shape tier (sub-struct)
            shape: ShapeTracks::default(),

            // Text tier (sub-struct)
            text: TextTracks::default(),

            // Tagged union tracks
            tagged_tracks: HashMap::new(),

            // Highlight tier (sub-struct)
            highlight: HighlightTracks::default(),
        }
    }

    /// Test-fixture constructor: a track with a placeholder `Rect` identity.
    ///
    /// Available only under `cfg(test)` so production code cannot construct a
    /// track with an implicit type — production tracks go through
    /// [`Self::new`] with a real primitive type name.
    #[cfg(test)]
    pub(crate) fn placeholder(label: String) -> Self {
        Self::new(label, "Rect")
    }

    /// Resolve tracks whose identity was never set. The authored type name is
    /// the only identity — there is nothing to derive it from.
    pub fn normalize_identity(&mut self) {
        if self.actor_type.is_empty() {
            tracing::warn!("track '{}' has an empty actor type; it cannot be rendered", self.label);
        }
    }

    /// Set this track's primitive identity, deriving capabilities from it.
    ///
    /// The single funnel for identity writes, so `actor_type` and `caps`
    /// cannot drift: built-in type names derive their caps from the registry,
    /// and anything unregistered (extension primitives) keeps all-false caps
    /// until its build path refines them.
    pub(crate) fn set_identity(&mut self, actor_type: &str) {
        self.actor_type = actor_type.to_string();
        self.caps = animatix_std::caps_for_type(actor_type).unwrap_or_default();
    }

    /// Returns `true` if this actor is a container (group, layout container,
    /// or effect scope) rather than a leaf drawable.
    pub fn is_container(&self) -> bool {
        self.caps.is_container
    }

    /// Returns `true` if this actor is a plain structural group.
    pub fn is_group_like(&self) -> bool {
        self.caps.is_group_like()
    }

    /// Returns `true` if this actor directly renders visual ink or content
    /// of its own, delegating to [`ActorCaps::renders_visual_content`].
    pub fn renders_visual_content(&self) -> bool {
        self.caps.renders_visual_content()
    }

    /// The primitive type name this track must resolve to at `time_ms`.
    ///
    /// Normally the track identity, but a same-label re-declaration that
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
    pub fn render_type_name(&self, time_ms: u64) -> &str {
        const VECTOR_SHAPES: [&str; 6] = ["Rect", "Ellipse", "Line", "Arrow", "Polygon", "Path"];
        if !VECTOR_SHAPES.contains(&self.actor_type.as_str()) {
            return &self.actor_type;
        }
        let Some(shape_track) = self.shape.shape_type.as_ref() else {
            return &self.actor_type;
        };
        // Only a track whose value actually changes may switch the primitive.
        // The verdict is memoized on the track (`shape_type_switches`, cleared
        // by `invalidate_frame_cache`): constant tracks — every non-morph
        // vector shape — pay one `Cell` read per frame instead of re-walking
        // the keyframe map.
        let switches = match self.shape.shape_type_switches.get() {
            Some(verdict) => verdict,
            None => {
                let mut values = shape_track.keyframes.values();
                let Some((first, _)) = values.next() else {
                    self.shape.shape_type_switches.set(Some(false));
                    return &self.actor_type;
                };
                let changed = values.any(|(v, _)| v != first);
                self.shape.shape_type_switches.set(Some(changed));
                changed
            },
        };
        if !switches {
            return &self.actor_type;
        }
        // A `switches == true` verdict implies a non-empty map; emptiness
        // returns through `invalidate_frame_cache`'s memo clear, so this
        // guard is belt-and-braces, not a live path.
        let Some((first, _)) = shape_track.keyframes.values().next() else {
            return &self.actor_type;
        };
        let at: ShapeType = self.shape.shape_type.get(time_ms, *first);
        let name = match at {
            ShapeType::Rect => "Rect",
            ShapeType::Ellipse => "Ellipse",
            ShapeType::Line => "Line",
            ShapeType::Polygon => "Polygon",
            ShapeType::Path => "Path",
            ShapeType::Arrow => "Arrow",
            _ => return &self.actor_type,
        };
        if name != self.actor_type {
            name
        } else {
            &self.actor_type
        }
    }

    /// Refine this track's capabilities from a registration info card.
    ///
    /// Extension builds call this after `set_identity`: an unregistered type
    /// name has no built-in caps, but the registration info knows its own.
    pub(crate) fn set_caps(&mut self, caps: ActorCaps) {
        self.caps = caps;
    }

    /// Rebuild the property plan from the current actor identity.
    pub fn rebuild_property_plan(&mut self) {
        let previous = std::mem::take(&mut self.property_plan);
        self.property_plan = super::plan::PropertyPlan::for_actor(&self.caps, &self.actor_type);
        self.property_plan.preserve_extension_slots(&previous);
    }

    /// Sample a plan-stored property by canonical name.
    pub fn sample_property_named(&self, name: &str, time_ms: u64) -> Option<PropertyValue> {
        let id = crate::timeline::property_registry::property_id(name)?;
        self.property_plan.get(id).and_then(|slot| slot.track.sample(time_ms))
    }

    /// Sample an f32 property by name with fallback default.
    pub fn sample_f32_prop(&self, name: &str, time_ms: u64, default: f32) -> f32 {
        match self.sample_property_named(name, time_ms) {
            Some(PropertyValue::F32(v)) => v,
            _ => default,
        }
    }

    /// Sample a Color / Vec4 property by name with fallback default.
    pub fn sample_color_prop(&self, name: &str, time_ms: u64, default: [f32; 4]) -> [f32; 4] {
        match self.sample_property_named(name, time_ms) {
            Some(PropertyValue::Color(c)) | Some(PropertyValue::Vec4(c)) => c,
            _ => default,
        }
    }

    /// Sample a bool property by name with fallback default.
    pub fn sample_bool_prop(&self, name: &str, time_ms: u64, default: bool) -> bool {
        match self.sample_property_named(name, time_ms) {
            Some(PropertyValue::Bool(v)) => v,
            _ => default,
        }
    }

    /// Sample a Vec2 property by name with fallback default.
    pub fn sample_vec2_prop(&self, name: &str, time_ms: u64, default: [f32; 2]) -> [f32; 2] {
        match self.sample_property_named(name, time_ms) {
            Some(PropertyValue::Vec2(v)) => v,
            _ => default,
        }
    }

    /// Sample a string property by name with fallback default.
    pub fn sample_string_prop(&self, name: &str, time_ms: u64, default: &str) -> String {
        match self.sample_property_named(name, time_ms) {
            Some(PropertyValue::String(s)) => s,
            _ => default.to_string(),
        }
    }

    /// Sample a u32 property by name with fallback default.
    pub fn sample_u32_prop(&self, name: &str, time_ms: u64, default: u32) -> u32 {
        match self.sample_property_named(name, time_ms) {
            Some(PropertyValue::U32(v)) => v,
            _ => default,
        }
    }

    // ── layout_size convenience methods (replacing old LayoutSizeState) ──
    /// Evaluate `layout_size` at `time_ms`.
    pub fn layout_size_get(&self, time_ms: u64) -> Option<[f32; 2]> {
        self.geometry.layout_size.as_ref().map(|t| t.evaluate(time_ms))
    }
    /// Return the last value of `layout_size`.
    pub fn layout_size_last(&self) -> Option<[f32; 2]> {
        self.geometry.layout_size.as_ref().map(|t| t.last_value())
    }
    /// Ensure `layout_size` exists, creating it with `default` if absent.
    pub fn ensure_layout_size(&mut self, default: [f32; 2]) -> &mut PropertyTrack<[f32; 2]> {
        self.geometry.layout_size.get_or_insert_with(|| PropertyTrack::new(default))
    }
    /// Return `true` if `layout_size` has been set.
    pub fn has_layout_size(&self) -> bool {
        self.geometry.layout_size.is_some()
    }

    // ── Font metrics accessors (Phase 6) ──
    /// Evaluate `ascent` at `time_ms`.
    pub fn ascent_get(&self, time_ms: u64) -> f32 {
        self.text.ascent.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0)
    }
    /// Evaluate `descent` at `time_ms`.
    pub fn descent_get(&self, time_ms: u64) -> f32 {
        self.text.descent.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0)
    }
    /// Evaluate `baseline` at `time_ms`.
    /// This is the offset of the baseline from the text center (0,0) after centering paths.
    pub fn baseline_get(&self, time_ms: u64) -> f32 {
        self.text.baseline.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0)
    }
    /// Set all three font metrics on the track at the given time.
    pub fn set_metrics(&mut self, time_ms: u64, ascent: f32, descent: f32, baseline: f32) {
        use crate::easing::Easing;
        self.text.ascent.ensure(0.0).add_keyframe(time_ms, ascent, Easing::Linear);
        self.text.descent.ensure(0.0).add_keyframe(time_ms, descent, Easing::Linear);
        self.text.baseline.ensure(0.0).add_keyframe(time_ms, baseline, Easing::Linear);
    }

    // ── Path evaluation ──
    /// Evaluate text paths and optional baseline clip path at `time_ms`,
    /// applying morphing, char_progress typewriter truncation, and split-text baseline reveal.
    pub fn evaluate_text_paths_and_clip(
        &self,
        time_ms: u64,
    ) -> (Vec<TextPath>, Option<kurbo::BezPath>) {
        if let Some(content_track) = &self.text.text_content {
            if !content_track.keyframes.is_empty() {
                let current_text = content_track.evaluate(time_ms);
                if current_text.is_empty() {
                    return (Vec::new(), None);
                }
            }
        }
        let default_paths = PropertyTrack::new(Vec::new());
        let paths_track = self.text.text_paths.as_ref().unwrap_or(&default_paths);
        let default_morph = PropertyTrack::new(MorphOptions::default());
        let morph_track = self.style.morph_options.as_ref().unwrap_or(&default_morph);
        let mut paths = morph::evaluate_paths_with_options(
            paths_track,
            morph_track,
            time_ms,
            morph::interpolate_text_paths,
        );

        let clip = apply_split_text_animation(&mut paths, self, time_ms);
        (paths, clip)
    }

    /// Evaluate text paths at `time_ms`, applying morphing and char_progress truncation.
    pub fn evaluate_text_paths(&self, time_ms: u64) -> Vec<TextPath> {
        self.evaluate_text_paths_and_clip(time_ms).0
    }
}

/// Applies split-text baseline reveal and typewriter animation to a collection of text paths.
/// Returns an optional baseline clip path if `split_mask` is active.
pub fn apply_split_text_animation(
    paths: &mut Vec<TextPath>,
    track: &AnimationTrack,
    time_ms: u64,
) -> Option<kurbo::BezPath> {
    let cp_track = track.text.char_progress.as_ref()?;
    let progress = cp_track.evaluate(time_ms).clamp(0.0, 1.0);
    if progress >= 1.0 {
        return None;
    }

    let split_by = track
        .text
        .split_by
        .as_ref()
        .map(|t| t.evaluate(time_ms))
        .unwrap_or_else(|| "char".to_string());
    let split_stagger =
        track.text.split_stagger.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0);
    let split_offset_y =
        track.text.split_offset_y.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0);
    let split_mask = track.text.split_mask.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(false);

    // Case A: Standard typewriter truncation
    let is_split_mode = split_offset_y != 0.0
        || split_mask
        || split_stagger > 0.0
        || (split_by != "char" && split_by != "chars");

    if !is_split_mode {
        let n = (progress as f64 * paths.len() as f64).ceil() as usize;
        paths.truncate(n);
        return None;
    }

    if paths.is_empty() {
        return None;
    }

    // Case B: Split-text reveal
    let mut max_unit = 0usize;
    for g in paths.iter() {
        let u = match split_by.as_str() {
            "line" | "lines" => g.line_idx as usize,
            "word" | "words" => g.word_idx as usize,
            _ => g.char_idx as usize,
        };
        if u > max_unit {
            max_unit = u;
        }
    }
    let n_units = max_unit + 1;

    let s = if split_stagger > 0.0 {
        if split_stagger > 1.0 {
            (split_stagger / 1000.0).clamp(0.0, 0.5)
        } else {
            split_stagger
        }
    } else {
        (0.6 / n_units as f32).min(0.08)
    };

    let s_clamped = if n_units > 1 {
        s.min(0.85 / (n_units - 1) as f32)
    } else {
        0.0
    };
    let w = if n_units > 1 {
        1.0 - (n_units - 1) as f32 * s_clamped
    } else {
        1.0
    };

    let effective_offset = if split_offset_y != 0.0 {
        split_offset_y
    } else if split_mask {
        30.0
    } else {
        0.0
    };

    // Calculate baseline clip rectangles if split_mask is true
    let clip_path = if split_mask {
        if let Some(static_clip) = &track.text.static_split_clip {
            Some(static_clip.clone())
        } else {
            crate::renderer::text::compute_split_mask_clip(paths)
        }
    } else {
        None
    };

    for g in paths.iter_mut() {
        let u = match split_by.as_str() {
            "line" | "lines" => g.line_idx as usize,
            "word" | "words" => g.word_idx as usize,
            _ => g.char_idx as usize,
        };
        let start_u = u as f32 * s_clamped;
        let p_u = if w > 0.0 {
            ((progress - start_u) / w).clamp(0.0, 1.0)
        } else {
            1.0
        };

        let dy = (1.0 - p_u) * effective_offset;
        if dy != 0.0 {
            g.path = kurbo::Affine::translate((0.0, dy as f64)) * g.path.clone();
            g.glyph_center[1] += dy;
            g.bounds = g.bounds + kurbo::Vec2::new(0.0, dy as f64);
        }

        if split_mask {
            let alpha = if p_u <= 0.0 {
                0.0
            } else {
                (p_u * 4.0).min(1.0)
            };
            g.opacity *= alpha;
        } else {
            g.opacity *= p_u;
        }
    }

    clip_path
}

impl AnimationTrack {
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
    /// value is memoized on the track and re-clones become refcount bumps.
    /// `alloc_driver` measured ~61 allocations / ~21 KB per frame on a
    /// 60-actor scene from every static shape actor cloning its whole path
    /// list per frame. The memo is guarded by `vector_paths_epoch` (bumped by
    /// `invalidate_frame_cache`, the funnel every mutation goes through) and
    /// must never observe a track mutated without that bump.
    pub fn evaluate_vector_paths(&self, time_ms: u64) -> std::sync::Arc<Vec<VelloPath>> {
        use std::sync::Arc;
        if let Some(paths_track) = self.shape.vector_paths.as_ref() {
            let constant = paths_track
                .keyframes
                .keys()
                .next_back()
                .map(|&last| time_ms > last)
                .unwrap_or(true);
            if constant {
                let epoch = self.shape.vector_paths_epoch.get();
                if let Some((memo_epoch, shared)) = self.shape.vector_paths_memo.borrow().as_ref() {
                    if *memo_epoch == epoch {
                        return Arc::clone(shared);
                    }
                }
                let computed = Arc::new(self.evaluate_vector_paths_value(time_ms));
                *self.shape.vector_paths_memo.borrow_mut() = Some((epoch, Arc::clone(&computed)));
                return computed;
            }
        }
        Arc::new(self.evaluate_vector_paths_value(time_ms))
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
    pub fn shape_path_memoized(&self, shape: &KurboShape) -> std::sync::Arc<kurbo::BezPath> {
        use std::sync::Arc;
        if let Some((cached_shape, shared)) = self.shape.shape_path_memo.borrow().as_ref() {
            if cached_shape == shape {
                return Arc::clone(shared);
            }
        }
        let built = Arc::new(shape.to_path_default());
        *self.shape.shape_path_memo.borrow_mut() = Some((shape.clone(), Arc::clone(&built)));
        built
    }

    /// Unmemoized single evaluation of the vector-path track.
    ///
    /// Build-time callers (`insert_start_keyframes`, assignment rewrites) use
    /// this so they observe values they have just written instead of a memo
    /// captured before the write.
    pub(crate) fn evaluate_vector_paths_value(&self, time_ms: u64) -> Vec<VelloPath> {
        let Some(paths_track) = self.shape.vector_paths.as_ref() else {
            return Vec::new();
        };
        let default_morph = PropertyTrack::new(MorphOptions::default());
        let morph_track = self.style.morph_options.as_ref().unwrap_or(&default_morph);
        morph::evaluate_paths_with_options(
            paths_track,
            morph_track,
            time_ms,
            morph::interpolate_vello_paths,
        )
    }

    /// Return the maximum keyframe time across all property tracks.
    pub fn max_keyframe_time(&self) -> Option<u64> {
        self.max_keyframe_time_with(None)
    }

    /// Maximum keyframe time across all tiers, including a payload when given.
    pub fn max_keyframe_time_with(
        &self,
        payload: Option<&crate::primitives::PrimitivePayload>,
    ) -> Option<u64> {
        use crate::timeline::property_registry::PROPERTY_REGISTRY;
        let mut max: Option<u64> = None;
        for schema in PROPERTY_REGISTRY.iter() {
            if let Some(t) = property_keyframe_times(self, schema.field).into_iter().max() {
                max = Some(max.map_or(t, |m| m.max(t)));
            }
        }
        // Dynamic plot parameter tracks (not registry-representable)
        if let Some(tracks) = payload
            .and_then(|payload| payload.as_plot())
            .map(|plot| &plot.plot_param_tracks)
        {
            for pt in tracks.values() {
                if let Some(t) = pt.last_keyframe_time() {
                    max = Some(max.map_or(t, |m| m.max(t)));
                }
            }
        }
        // Tagged union tracks (also registry-representable when a schema exists).
        for track in self.tagged_tracks.values().flatten() {
            if let Some(t) = track.last_keyframe_time() {
                max = Some(max.map_or(t, |m| m.max(t)));
            }
        }
        // Registry-driven plan tracks.
        if let Some(t) = self.property_plan.max_keyframe_time() {
            max = Some(max.map_or(t, |m| m.max(t)));
        }
        // Func transitions: include the end time of each transition.
        for ft in payload
            .and_then(|payload| payload.as_plot())
            .map_or(&[] as &[crate::timeline::plot::FuncTransition], |p| &p.func_transitions)
        {
            max = Some(max.map_or(ft.end_ms, |m| m.max(ft.end_ms)));
        }
        // BarChart data transitions: the same reasoning as func transitions.
        for bt in payload
            .and_then(|payload| payload.as_plot())
            .map_or(&[] as &[crate::timeline::plot::BarDataTransition], |p| &p.bar_data_transitions)
        {
            max = Some(max.map_or(bt.end_ms, |m| m.max(bt.end_ms)));
        }
        max
    }

    /// Returns true if any property track has animated keyframes.
    /// A track is "animated" if it has 2+ keyframes or 1 keyframe at time > 0.
    pub fn has_any_keyframes(&self) -> bool {
        self.has_any_keyframes_with(None)
    }

    /// Whether any tier (incl. payload) has animated keyframes.
    pub fn has_any_keyframes_with(
        &self,
        payload: Option<&crate::primitives::PrimitivePayload>,
    ) -> bool {
        use crate::timeline::property_registry::PROPERTY_REGISTRY;
        for schema in PROPERTY_REGISTRY.iter() {
            let times = property_keyframe_times(self, schema.field);
            if times.len() > 1 || (times.len() == 1 && times[0] > 0) {
                return true;
            }
        }
        let plot = payload.and_then(|payload| payload.as_plot());
        plot.is_some_and(|plot| {
            plot.plot_param_tracks.values().any(|t| !t.is_effectively_static())
                || !plot.func_transitions.is_empty()
                || !plot.bar_data_transitions.is_empty()
        }) || self.tagged_tracks.values().flatten().any(|t| !t.is_effectively_static())
            || self.property_plan.has_any_keyframes()
    }
}
