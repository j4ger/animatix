//! # Field Dispatch Module
//!
//! Central dispatch for reading/writing property tracks on [`AnimationTrack`].
//!
//! ## Key types
//!
//! - [`AnimationTrack`] — per-actor keyframed property container.
//! - [`TrackFieldRef`] / [`TrackFieldMut`] — type-erased references to property tracks, enabling
//!   generic dispatch over the property registry.
//!
//! ## Free functions
//!
//! - [`read_property_value`] — read a property value at a given time.
//! - [`read_property_value_or_default`] — read a property, falling back to the schema default.
//! - [`property_has_keyframes`] / [`property_has_keyframe_at`] — keyframe existence checks.
//! - [`property_keyframe_count`] / [`property_keyframe_times`] — keyframe metadata.
//! - [`property_keyframe_easing`] — easing at a specific keyframe.

use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::actor_kind::{ActorKindId, ShapeKind};
use super::animation_track::{
    CalloutPlace, FilterTracks, GeometryTracks, HighlightTracks, PlacementMode, PositionBinding,
    ShapeTracks, StyleTracks, TextTracks,
};
use super::morph;
use super::property_track::{PropertyTrack, TrackAccessor};
use crate::easing::Easing;
use crate::renderer::types::{TextPath, VelloPath};
use crate::timeline::morph::MorphOptions;
use crate::timeline::plot::{FuncTransition, ProceduralPlot};
use crate::timeline::property_engine::EnumPropertyValue;
pub use crate::timeline::property_engine::PropertyValue;
use crate::timeline::property_registry::{ActorField, PropertySchema};

// ─────────────────────────────────────────────────────────────
// AnimationTrack
// ─────────────────────────────────────────────────────────────

/// Per-actor container for all keyframed property tracks.
///
/// Every actor in the scene graph has exactly one `AnimationTrack`.  The track
/// stores typed optional property tracks (geometry, style, filter, shape, text,
/// highlight tiers), along with metadata such as `kind` (`ActorKindId`),
/// `children` (scene-graph hierarchy), and `visible`.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct AnimationTrack {
    // ── Identity / metadata ──
    /// Human-readable identifier for the actor.
    pub label: String,
    /// Compile-time kind of this actor.
    pub kind: ActorKindId,
    /// First frame (ms) this actor appears.
    pub first_seen_ms: u64,
    /// Labels of child actors in the scene hierarchy.
    pub children: Vec<String>,
    /// Label of the parent actor in the scene hierarchy, if any.
    /// `None` for root-level actors.
    pub parent: Option<String>,
    /// Whether the actor is visible in the preview and export.
    pub visible: bool,
    /// Whether the actor is locked (preventing selection and drag in the GUI).
    pub locked: bool,

    // ── Geometry tier (sub-struct) ──
    /// Geometry property tracks (position, size, rotation, scale, etc.).
    pub geometry: GeometryTracks,

    // ── Style tier (sub-struct) ──
    /// Style property tracks (color, opacity, stroke, line_cap, line_join, morph_options).
    pub style: StyleTracks,

    // ── Filter tier (sub-struct) ──
    /// Filter property tracks (blur, brightness, contrast, etc.).
    pub filter: FilterTracks,

    // ── Shape tier (sub-struct) ──
    /// Shape property tracks (shape_type, line_from, line_to, etc.).
    pub shape: ShapeTracks,

    // ── Text tier (sub-struct) ──
    /// Text property tracks (text_content, font_family, font_size, etc.).
    pub text: TextTracks,
    /// Static SVG paths from declarations.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub svg_paths: Vec<crate::timeline::VelloPath>,
    /// Keyframed SVG paths from timed `Svg.url` assignments.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub svg_paths_track: Option<PropertyTrack<Option<Vec<crate::timeline::VelloPath>>>>,
    /// Raster image data.
    #[cfg(feature = "render")]
    #[cfg_attr(feature = "serde", serde(skip))]
    pub image: Option<PropertyTrack<Option<crate::timeline::image::SceneImage>>>,

    // ── Procedural plot (re-sampled at frame time) ──
    /// Procedural plot generator, re-sampled each frame.
    pub procedural_plot: Option<ProceduralPlot>,

    // ── Plot parameter keyframe tracks ──
    /// Per-parameter keyframe tracks for procedural plot actors.
    /// Maps parameter name (e.g. "freq") to an f64 property track.
    pub plot_param_tracks: HashMap<String, PropertyTrack<f64>>,

    // ── Tagged union tracks ──
    /// Generic tagged union property tracks, keyed by canonical property name.
    pub tagged_tracks: HashMap<String, Option<PropertyTrack<PropertyValue>>>,

    // ── Func transition tracks (side-channel) ──
    /// Parallel transition storage for `func` property on `PlotCurve`.
    ///
    /// ## Why a side-channel?
    ///
    /// Most animatable properties (f32, Vec2, Color, etc.) implement the
    /// [`Interpolate`] trait, which allows the standard `PropertyTrack<T>`
    /// keyframe system to compute in-between values automatically. Closures
    /// (function sources like `(x) => sin(x * freq)`) **cannot** implement
    /// [`Interpolate`] — there is no meaningful way to "lerp" two closures.
    ///
    /// Instead of forcing closures into the `Interpolate` model, we store
    /// function transitions as a **parallel side-channel** alongside the
    /// normal property tracks. Each [`FuncTransition`] records a start time,
    /// end time, easing, and the `from` / `to` [`FuncSource`] closures. At
    /// frame evaluation time, [`sample_procedural_plot_at`] checks for active
    /// transitions and blends the *outputs* of the two sources by the eased
    /// progress value.
    ///
    /// ## How it works
    ///
    /// 1. Parsing discovers `func = <expr> [easing, duration]` and appends a [`FuncTransition`] to
    ///    this vector (not to a `PropertyTrack`).
    /// 2. At each frame, [`sample_procedural_plot_at`] finds the active transition (if any),
    ///    evaluates both `from` and `to` closure outputs at each sample point, and lerps the
    ///    outputs.
    /// 3. Completed transitions are detected via [`FuncTransition::is_complete_at`] and the last
    ///    completed `to` source is used as the new baseline.
    ///
    /// ## When to use this pattern
    ///
    /// Any property whose type cannot implement [`Interpolate`] should use a
    /// side-channel. Candidates include:
    ///
    /// - **Closures / AST function bodies** (the current case)
    /// - **Arbitrary AST nodes** that are structural rather than numeric
    /// - **External resource handles** (e.g., image URLs that need loading)
    ///
    /// If the type *can* implement [`Interpolate`], prefer the standard
    /// `PropertyTrack<T>` approach instead.
    ///
    /// ## Implementation checklist for new non-interpolatable properties
    ///
    /// 1. Define a transition struct (like [`FuncTransition`]) with `start_ms`, `end_ms`,
    ///    `easing`, `from`, `to`.
    /// 2. Define the source type (like [`FuncSource`]) with variants for raw values and
    ///    mid-transition blends.
    /// 3. Add a `Vec<YourTransition>` field to [`AnimationTrack`].
    /// 4. Include the transition end times in [`max_keyframe_time`].
    /// 5. Include non-empty transitions in [`has_any_keyframes`].
    /// 6. At frame evaluation, find the active transition and blend the *outputs* of `from` and
    ///    `to` by eased progress.
    pub func_transitions: Vec<FuncTransition>,

    // ── Highlight tier (sub-struct) ──
    /// Highlight property tracks (color, opacity, padding, radius, blend).
    pub highlight: HighlightTracks,

    // ── Legend tier ──
    /// Legend entries (auto-generated from scene content).
    pub legend: super::legend::LegendTracks,
}

impl AnimationTrack {
    /// Create a new empty animation track with the given label.
    pub fn new(label: String) -> Self {
        Self {
            // Identity
            label: label.clone(),
            kind: ActorKindId::Shape(ShapeKind::Rect),
            first_seen_ms: u64::MAX,
            children: Vec::new(),
            parent: None,
            visible: true,
            locked: false,

            // Geometry tier (sub-struct)
            geometry: GeometryTracks::default(),

            // Style tier (sub-struct)
            style: StyleTracks::default(),

            // Filter tier (sub-struct)
            filter: FilterTracks::default(),

            // Shape tier (sub-struct)
            shape: ShapeTracks::default(),

            // Text tier (sub-struct)
            text: TextTracks::default(),
            svg_paths: Vec::new(),
            svg_paths_track: None,
            #[cfg(feature = "render")]
            image: None,

            // Procedural plot
            procedural_plot: None,

            // Plot parameter tracks
            plot_param_tracks: HashMap::new(),

            // Tagged union tracks
            tagged_tracks: HashMap::new(),

            // Func transition tracks
            func_transitions: Vec::new(),

            // Highlight tier (sub-struct)
            highlight: HighlightTracks::default(),

            // Legend tier
            legend: super::legend::LegendTracks::default(),
        }
    }

    // ── layout_size convenience methods (replacing old LayoutSizeState) ──
    /// Evaluate `layout_size` at `time_ms`.
    pub fn layout_size_get(&self, time_ms: u64) -> Option<[f32; 2]> {
        self.geometry.layout_size.as_ref().map(|t| t.evaluate(time_ms))
    }
    /// Returns the parent actor label, if this actor has a parent.
    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }

    /// Returns the labels of child actors.
    pub fn children(&self) -> &[String] {
        &self.children
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
    /// Evaluate text paths at `time_ms`, applying morphing and char_progress truncation.
    pub fn evaluate_text_paths(&self, time_ms: u64) -> Vec<TextPath> {
        if let Some(content_track) = &self.text.text_content {
            if !content_track.keyframes.is_empty() {
                let current_text = content_track.evaluate(time_ms);
                if current_text.is_empty() {
                    return Vec::new();
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

        // Apply char_progress typewriter truncation
        if let Some(cp_track) = &self.text.char_progress {
            let progress = cp_track.evaluate(time_ms).clamp(0.0, 1.0) as f64;
            if progress < 1.0 {
                let n = (progress * paths.len() as f64).ceil() as usize;
                paths.truncate(n);
            }
        }

        paths
    }

    /// Evaluate SVG paths at `time_ms`, preferring timed assignments over the
    /// declaration-level static path set.
    pub fn svg_paths_at(&self, time_ms: u64) -> Option<Vec<VelloPath>> {
        self.svg_paths_track
            .as_ref()
            .and_then(|track| track.evaluate(time_ms))
            .or_else(|| {
                if self.svg_paths.is_empty() {
                    None
                } else {
                    Some(self.svg_paths.clone())
                }
            })
    }

    /// Whether this actor has any declaration or assignment SVG path content.
    pub fn has_svg_path_content(&self) -> bool {
        !self.svg_paths.is_empty()
            || self
                .svg_paths_track
                .as_ref()
                .is_some_and(|track| track.default_value.is_some() || !track.keyframes.is_empty())
    }

    /// Evaluate vector paths at `time_ms`, applying morphing if configured.
    pub fn evaluate_vector_paths(&self, time_ms: u64) -> Vec<VelloPath> {
        let default_paths = PropertyTrack::new(Vec::new());
        let paths_track = self.shape.vector_paths.as_ref().unwrap_or(&default_paths);
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
        use crate::timeline::property_registry::PROPERTY_REGISTRY;
        let mut max: Option<u64> = None;
        for schema in PROPERTY_REGISTRY {
            if let Some(t) = property_keyframe_times(self, schema.field).into_iter().max() {
                max = Some(max.map_or(t, |m| m.max(t)));
            }
        }
        // Dynamic plot parameter tracks (not registry-representable)
        for pt in self.plot_param_tracks.values() {
            if let Some(t) = pt.last_keyframe_time() {
                max = Some(max.map_or(t, |m| m.max(t)));
            }
        }
        // Tagged union tracks (also registry-representable when a schema exists).
        for track in self.tagged_tracks.values().flatten() {
            if let Some(t) = track.last_keyframe_time() {
                max = Some(max.map_or(t, |m| m.max(t)));
            }
        }
        // Func transitions: include the end time of each transition.
        for ft in &self.func_transitions {
            max = Some(max.map_or(ft.end_ms, |m| m.max(ft.end_ms)));
        }
        max
    }

    /// Returns true if any property track has animated keyframes.
    /// A track is "animated" if it has 2+ keyframes or 1 keyframe at time > 0.
    pub fn has_any_keyframes(&self) -> bool {
        use crate::timeline::property_registry::PROPERTY_REGISTRY;
        for schema in PROPERTY_REGISTRY {
            let times = property_keyframe_times(self, schema.field);
            if times.len() > 1 || (times.len() == 1 && times[0] > 0) {
                return true;
            }
        }
        self.plot_param_tracks.values().any(|t| !t.is_effectively_static())
            || self.tagged_tracks.values().flatten().any(|t| !t.is_effectively_static())
            || !self.func_transitions.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────
// Field dispatch enums (centralise the ActorField → track field mapping)
// ─────────────────────────────────────────────────────────────

/// Immutable reference to a track field, abstracting over the value type.
///
/// This enum lets generic code access `PropertyTrack<T>` fields without
/// knowing `T` at compile time.  It eliminates the N×M match-block explosion
/// in `property_engine.rs`.
pub enum TrackFieldRef<'a> {
    /// `f32` property track.
    F32(&'a Option<PropertyTrack<f32>>),
    /// `[f32; 2]` (2D vector) property track.
    Vec2(&'a Option<PropertyTrack<[f32; 2]>>),
    /// `[f32; 4]` (RGBA color) property track.
    Vec4(&'a Option<PropertyTrack<[f32; 4]>>),
    /// `[f32; 6]` (2×3 transform) property track.
    Transform(&'a Option<PropertyTrack<[f32; 6]>>),
    /// `String` property track.
    String(&'a Option<PropertyTrack<String>>),
    /// `u32` property track.
    U32(&'a Option<PropertyTrack<u32>>),
    /// List of 2D points property track.
    PointList(&'a Option<PropertyTrack<Vec<[f32; 2]>>>),
    /// Command string property track.
    CommandList(&'a Option<PropertyTrack<String>>),
    /// Shape type property track.
    ShapeType(&'a Option<PropertyTrack<super::shapes::ShapeType>>),
    /// Placement mode property track.
    PlacementMode(&'a Option<PropertyTrack<PlacementMode>>),
    /// Callout place property track.
    CalloutPlace(&'a Option<PropertyTrack<CalloutPlace>>),
    /// Morph options property track.
    MorphOptions(&'a Option<PropertyTrack<MorphOptions>>),
    /// Vector paths property track.
    VectorPaths(&'a Option<PropertyTrack<Vec<VelloPath>>>),
    /// Text paths property track.
    TextPaths(&'a Option<PropertyTrack<Vec<TextPath>>>),
    /// Raster image data track (cfg-gated on "render").
    #[cfg(feature = "render")]
    Image(&'a Option<PropertyTrack<Option<crate::timeline::image::SceneImage>>>),
    /// Position binding property track.
    PositionBinding(&'a Option<PropertyTrack<PositionBinding>>),
    /// Generic tagged union property track.
    Tagged(&'static str, &'a Option<PropertyTrack<PropertyValue>>),
}

/// Mutable reference to a track field, abstracting over the value type.
pub enum TrackFieldMut<'a> {
    /// `f32` property track.
    F32(&'a mut Option<PropertyTrack<f32>>),
    /// `[f32; 2]` (2D vector) property track.
    Vec2(&'a mut Option<PropertyTrack<[f32; 2]>>),
    /// `[f32; 4]` (RGBA color) property track.
    Vec4(&'a mut Option<PropertyTrack<[f32; 4]>>),
    /// `[f32; 6]` (2×3 transform) property track.
    Transform(&'a mut Option<PropertyTrack<[f32; 6]>>),
    /// `String` property track.
    String(&'a mut Option<PropertyTrack<String>>),
    /// `u32` property track.
    U32(&'a mut Option<PropertyTrack<u32>>),
    /// List of 2D points property track.
    PointList(&'a mut Option<PropertyTrack<Vec<[f32; 2]>>>),
    /// Command string property track.
    CommandList(&'a mut Option<PropertyTrack<String>>),
    /// Shape type property track.
    ShapeType(&'a mut Option<PropertyTrack<super::shapes::ShapeType>>),
    /// Placement mode property track.
    PlacementMode(&'a mut Option<PropertyTrack<PlacementMode>>),
    /// Callout place property track.
    CalloutPlace(&'a mut Option<PropertyTrack<CalloutPlace>>),
    /// Morph options property track.
    MorphOptions(&'a mut Option<PropertyTrack<MorphOptions>>),
    /// Vector paths property track.
    VectorPaths(&'a mut Option<PropertyTrack<Vec<VelloPath>>>),
    /// Text paths property track.
    TextPaths(&'a mut Option<PropertyTrack<Vec<TextPath>>>),
    /// Raster image data track (cfg-gated on "render").
    #[cfg(feature = "render")]
    Image(&'a mut Option<PropertyTrack<Option<crate::timeline::image::SceneImage>>>),
    /// Position binding property track.
    PositionBinding(&'a mut Option<PropertyTrack<PositionBinding>>),
    /// Generic tagged union property track.
    Tagged(&'static str, &'a mut Option<PropertyTrack<PropertyValue>>),
}

// ─────────────────────────────────────────────────────────────
// TrackFieldRef convenience methods
// ─────────────────────────────────────────────────────────────

impl<'a> TrackFieldRef<'a> {
    /// Evaluate the track value at `time_ms`, returning the result as a `PropertyValue`.
    /// Returns `None` for types that cannot be represented as `PropertyValue`.
    pub fn evaluate_value(&self, time_ms: u64) -> Option<PropertyValue> {
        match self {
            Self::F32(opt) => opt.as_ref().map(|pt| PropertyValue::F32(pt.evaluate_copy(time_ms))),
            Self::Vec2(opt) => {
                opt.as_ref().map(|pt| PropertyValue::Vec2(pt.evaluate_copy(time_ms)))
            },
            Self::Vec4(opt) => {
                opt.as_ref().map(|pt| PropertyValue::Color(pt.evaluate_copy(time_ms)))
            },
            Self::Transform(opt) => {
                opt.as_ref().map(|pt| PropertyValue::Transform(pt.evaluate_copy(time_ms)))
            },
            Self::U32(opt) => opt.as_ref().map(|pt| PropertyValue::U32(pt.evaluate_copy(time_ms))),
            Self::ShapeType(opt) => {
                opt.as_ref().map(|pt| pt.evaluate_copy(time_ms).to_property_value())
            },
            Self::PlacementMode(opt) => {
                opt.as_ref().map(|pt| pt.evaluate_copy(time_ms).to_property_value())
            },
            Self::CalloutPlace(opt) => {
                opt.as_ref().map(|pt| pt.evaluate_copy(time_ms).to_property_value())
            },
            Self::MorphOptions(opt) => opt
                .as_ref()
                .map(|pt| PropertyValue::String(pt.evaluate_copy(time_ms).summary())),
            Self::String(opt) => opt.as_ref().map(|pt| PropertyValue::String(pt.evaluate(time_ms))),
            Self::PointList(opt) => {
                opt.as_ref().map(|pt| PropertyValue::PointList(pt.evaluate(time_ms)))
            },
            Self::CommandList(opt) => {
                opt.as_ref().map(|pt| PropertyValue::CommandList(pt.evaluate(time_ms)))
            },
            Self::VectorPaths(_) | Self::TextPaths(_) | Self::PositionBinding(_) => None,
            Self::Tagged(_, opt) => opt.as_ref().map(|pt| pt.evaluate(time_ms)),
            #[cfg(feature = "render")]
            Self::Image(_) => None,
        }
    }

    /// Returns `true` if this track has a keyframe at exactly `time_ms`.
    pub fn has_keyframe_at(&self, time_ms: u64) -> bool {
        match self {
            Self::F32(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::Vec2(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::Vec4(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::Transform(opt) => {
                opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms))
            },
            Self::String(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::U32(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::PointList(opt) => {
                opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms))
            },
            Self::CommandList(opt) => {
                opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms))
            },
            Self::ShapeType(opt) => {
                opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms))
            },
            Self::PlacementMode(opt) => {
                opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms))
            },
            Self::CalloutPlace(opt) => {
                opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms))
            },
            Self::MorphOptions(opt) => {
                opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms))
            },
            Self::VectorPaths(opt) => {
                opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms))
            },
            Self::TextPaths(opt) => {
                opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms))
            },
            #[cfg(feature = "render")]
            Self::Image(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::PositionBinding(opt) => {
                opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms))
            },
            Self::Tagged(_, opt) => {
                opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms))
            },
        }
    }

    /// Returns the number of keyframes in this track.
    pub fn keyframe_count(&self) -> usize {
        match self {
            Self::F32(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::Vec2(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::Vec4(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::Transform(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::String(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::U32(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::PointList(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::CommandList(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::ShapeType(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::PlacementMode(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::CalloutPlace(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::MorphOptions(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::VectorPaths(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::TextPaths(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            #[cfg(feature = "render")]
            Self::Image(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::PositionBinding(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::Tagged(_, opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        }
    }

    /// Returns all keyframe timestamps (ms), sorted.
    pub fn keyframe_times(&self) -> Vec<u64> {
        match self {
            Self::F32(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::Vec2(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::Vec4(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::Transform(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::String(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::U32(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::PointList(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::CommandList(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::ShapeType(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::PlacementMode(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::CalloutPlace(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::MorphOptions(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::VectorPaths(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::TextPaths(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            #[cfg(feature = "render")]
            Self::Image(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::PositionBinding(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::Tagged(_, opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
        }
    }

    /// Returns the easing at a specific keyframe time, if one exists.
    pub fn keyframe_easing(&self, time_ms: u64) -> Option<Easing> {
        match self {
            Self::F32(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::Vec2(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::Vec4(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::Transform(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::String(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::U32(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::PointList(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::CommandList(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::ShapeType(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::PlacementMode(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::CalloutPlace(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::MorphOptions(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::VectorPaths(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::TextPaths(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            #[cfg(feature = "render")]
            Self::Image(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::PositionBinding(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::Tagged(_, opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
        }
    }
}

impl AnimationTrack {
    /// Get an immutable reference to the track field identified by `field`.
    pub fn field_ref(&self, field: ActorField) -> Option<TrackFieldRef<'_>> {
        use ActorField::*;
        Some(match field {
            Position => TrackFieldRef::Vec2(&self.geometry.position),
            MotionOffset => TrackFieldRef::Vec2(&self.geometry.motion_offset),
            Size => TrackFieldRef::Vec2(&self.geometry.size),
            LayoutSize => TrackFieldRef::Vec2(&self.geometry.layout_size),
            Rotation => TrackFieldRef::F32(&self.geometry.rotation),
            Scale => TrackFieldRef::F32(&self.geometry.scale),
            Transform => TrackFieldRef::Transform(&self.geometry.transform),
            Color => TrackFieldRef::Vec4(&self.style.color),
            Opacity => TrackFieldRef::F32(&self.style.opacity),
            StrokeWidth => TrackFieldRef::F32(&self.style.stroke_width),
            StrokeColor => TrackFieldRef::Vec4(&self.style.stroke_color),
            StrokeProgress => TrackFieldRef::F32(&self.style.stroke_progress),
            FillOpacity => TrackFieldRef::F32(&self.style.fill_opacity),
            FilterBlur => TrackFieldRef::F32(&self.filter.filter_blur),
            FilterBrightness => TrackFieldRef::F32(&self.filter.filter_brightness),
            FilterContrast => TrackFieldRef::F32(&self.filter.filter_contrast),
            FilterSaturate => TrackFieldRef::F32(&self.filter.filter_saturate),
            FilterHueRotate => TrackFieldRef::F32(&self.filter.filter_hue_rotate),
            FilterSepia => TrackFieldRef::F32(&self.filter.filter_sepia),
            ShapeType => TrackFieldRef::ShapeType(&self.shape.shape_type),
            LineFrom => TrackFieldRef::Vec2(&self.shape.line_from),
            LineTo => TrackFieldRef::Vec2(&self.shape.line_to),
            HeadSize => TrackFieldRef::F32(&self.shape.head_size),
            LineCap => TrackFieldRef::U32(&self.style.line_cap),
            LineJoin => TrackFieldRef::U32(&self.style.line_join),
            ArcAngles => TrackFieldRef::Vec2(&self.shape.arc_angles),
            Points => TrackFieldRef::PointList(&self.shape.points),
            Commands => TrackFieldRef::CommandList(&self.shape.commands),
            TextContent => TrackFieldRef::String(&self.text.text_content),
            TextMaxWidth => TrackFieldRef::F32(&self.text.text_max_width),
            TextAlign => TrackFieldRef::String(&self.text.text_align),
            Overflow => TrackFieldRef::String(&self.text.overflow),
            FontFamily => TrackFieldRef::String(&self.text.font_family),
            FontSize => TrackFieldRef::F32(&self.text.font_size),
            CharProgress => TrackFieldRef::F32(&self.text.char_progress),
            PlacementMode => TrackFieldRef::PlacementMode(&self.geometry.placement_mode),
            MorphOptions => TrackFieldRef::MorphOptions(&self.style.morph_options),
            Ascent => TrackFieldRef::F32(&self.text.ascent),
            Descent => TrackFieldRef::F32(&self.text.descent),
            Baseline => TrackFieldRef::F32(&self.text.baseline),
            HighlightColor => TrackFieldRef::Vec4(&self.highlight.highlight_color),
            HighlightOpacity => TrackFieldRef::F32(&self.highlight.highlight_opacity),
            HighlightPadding => TrackFieldRef::F32(&self.highlight.highlight_padding),
            HighlightRadius => TrackFieldRef::F32(&self.highlight.highlight_radius),
            FontWeight => TrackFieldRef::F32(&self.text.font_weight),
            FontStyle => TrackFieldRef::String(&self.text.font_style),
            LineHeight => TrackFieldRef::F32(&self.text.line_height),
            LetterSpacing => TrackFieldRef::F32(&self.text.letter_spacing),
            WordSpacing => TrackFieldRef::F32(&self.text.word_spacing),
            MinWidth => TrackFieldRef::F32(&self.geometry.min_width),
            MinHeight => TrackFieldRef::F32(&self.geometry.min_height),
            MaxHeight => TrackFieldRef::F32(&self.geometry.max_height),
            LabelAt => TrackFieldRef::Vec2(&self.geometry.label_at),
            CalloutTarget => TrackFieldRef::String(&self.geometry.callout_target),
            CalloutPlace => TrackFieldRef::CalloutPlace(&self.geometry.callout_place),
            CalloutStandoff => TrackFieldRef::F32(&self.geometry.callout_standoff),
            CalloutToOffset => TrackFieldRef::Vec2(&self.geometry.callout_to_offset),
            VectorPaths => TrackFieldRef::VectorPaths(&self.shape.vector_paths),
            TextPaths => TrackFieldRef::TextPaths(&self.text.text_paths),
            #[cfg(feature = "render")]
            ImageData => TrackFieldRef::Image(&self.image),
            #[cfg(not(feature = "render"))]
            ImageData => return None,
            PositionBinding => TrackFieldRef::PositionBinding(&self.geometry.position_binding),
            ActorField::Tagged(name) => {
                return self
                    .tagged_tracks
                    .get(name)
                    .map(|track| TrackFieldRef::Tagged(name, track));
            },
            // Remaining variants without track storage
            SvgPaths | AudioSource | AudioVolume | PositionBindingGroup | VectorShapeGroup
            | PlotDomainGroup | ContainerLayoutGroup | NoStorage => return None,
        })
    }

    /// Get a mutable reference to the track field identified by `field`.
    pub fn field_mut(&mut self, field: ActorField) -> Option<TrackFieldMut<'_>> {
        if let ActorField::Tagged(name) = field {
            return Some(TrackFieldMut::Tagged(
                name,
                self.tagged_tracks.entry(name.to_string()).or_default(),
            ));
        }
        use ActorField::*;
        Some(match field {
            Position => TrackFieldMut::Vec2(&mut self.geometry.position),
            MotionOffset => TrackFieldMut::Vec2(&mut self.geometry.motion_offset),
            Size => TrackFieldMut::Vec2(&mut self.geometry.size),
            LayoutSize => TrackFieldMut::Vec2(&mut self.geometry.layout_size),
            Rotation => TrackFieldMut::F32(&mut self.geometry.rotation),
            Scale => TrackFieldMut::F32(&mut self.geometry.scale),
            Transform => TrackFieldMut::Transform(&mut self.geometry.transform),
            Color => TrackFieldMut::Vec4(&mut self.style.color),
            Opacity => TrackFieldMut::F32(&mut self.style.opacity),
            StrokeWidth => TrackFieldMut::F32(&mut self.style.stroke_width),
            StrokeColor => TrackFieldMut::Vec4(&mut self.style.stroke_color),
            StrokeProgress => TrackFieldMut::F32(&mut self.style.stroke_progress),
            FillOpacity => TrackFieldMut::F32(&mut self.style.fill_opacity),
            FilterBlur => TrackFieldMut::F32(&mut self.filter.filter_blur),
            FilterBrightness => TrackFieldMut::F32(&mut self.filter.filter_brightness),
            FilterContrast => TrackFieldMut::F32(&mut self.filter.filter_contrast),
            FilterSaturate => TrackFieldMut::F32(&mut self.filter.filter_saturate),
            FilterHueRotate => TrackFieldMut::F32(&mut self.filter.filter_hue_rotate),
            FilterSepia => TrackFieldMut::F32(&mut self.filter.filter_sepia),
            ShapeType => TrackFieldMut::ShapeType(&mut self.shape.shape_type),
            LineFrom => TrackFieldMut::Vec2(&mut self.shape.line_from),
            LineTo => TrackFieldMut::Vec2(&mut self.shape.line_to),
            HeadSize => TrackFieldMut::F32(&mut self.shape.head_size),
            LineCap => TrackFieldMut::U32(&mut self.style.line_cap),
            LineJoin => TrackFieldMut::U32(&mut self.style.line_join),
            ArcAngles => TrackFieldMut::Vec2(&mut self.shape.arc_angles),
            Points => TrackFieldMut::PointList(&mut self.shape.points),
            Commands => TrackFieldMut::CommandList(&mut self.shape.commands),
            TextContent => TrackFieldMut::String(&mut self.text.text_content),
            TextMaxWidth => TrackFieldMut::F32(&mut self.text.text_max_width),
            TextAlign => TrackFieldMut::String(&mut self.text.text_align),
            Overflow => TrackFieldMut::String(&mut self.text.overflow),
            FontFamily => TrackFieldMut::String(&mut self.text.font_family),
            FontSize => TrackFieldMut::F32(&mut self.text.font_size),
            CharProgress => TrackFieldMut::F32(&mut self.text.char_progress),
            PlacementMode => TrackFieldMut::PlacementMode(&mut self.geometry.placement_mode),
            MorphOptions => TrackFieldMut::MorphOptions(&mut self.style.morph_options),
            Ascent => TrackFieldMut::F32(&mut self.text.ascent),
            Descent => TrackFieldMut::F32(&mut self.text.descent),
            Baseline => TrackFieldMut::F32(&mut self.text.baseline),
            HighlightColor => TrackFieldMut::Vec4(&mut self.highlight.highlight_color),
            HighlightOpacity => TrackFieldMut::F32(&mut self.highlight.highlight_opacity),
            HighlightPadding => TrackFieldMut::F32(&mut self.highlight.highlight_padding),
            HighlightRadius => TrackFieldMut::F32(&mut self.highlight.highlight_radius),
            FontWeight => TrackFieldMut::F32(&mut self.text.font_weight),
            FontStyle => TrackFieldMut::String(&mut self.text.font_style),
            LineHeight => TrackFieldMut::F32(&mut self.text.line_height),
            LetterSpacing => TrackFieldMut::F32(&mut self.text.letter_spacing),
            WordSpacing => TrackFieldMut::F32(&mut self.text.word_spacing),
            MinWidth => TrackFieldMut::F32(&mut self.geometry.min_width),
            MinHeight => TrackFieldMut::F32(&mut self.geometry.min_height),
            MaxHeight => TrackFieldMut::F32(&mut self.geometry.max_height),
            LabelAt => TrackFieldMut::Vec2(&mut self.geometry.label_at),
            CalloutTarget => TrackFieldMut::String(&mut self.geometry.callout_target),
            CalloutPlace => TrackFieldMut::CalloutPlace(&mut self.geometry.callout_place),
            CalloutStandoff => TrackFieldMut::F32(&mut self.geometry.callout_standoff),
            CalloutToOffset => TrackFieldMut::Vec2(&mut self.geometry.callout_to_offset),
            VectorPaths => TrackFieldMut::VectorPaths(&mut self.shape.vector_paths),
            TextPaths => TrackFieldMut::TextPaths(&mut self.text.text_paths),
            #[cfg(feature = "render")]
            ImageData => TrackFieldMut::Image(&mut self.image),
            #[cfg(not(feature = "render"))]
            ImageData => return None,
            PositionBinding => TrackFieldMut::PositionBinding(&mut self.geometry.position_binding),
            _ => return None,
        })
    }

    /// Returns `true` if the property track for `field` is currently
    /// interpolating between two keyframes at the given time (the next
    /// keyframe after `time_ms` uses a non-Linear easing, distinguishing
    /// real animation targets from build-time snapshot scaffolding).
    ///
    /// This is the frame-time definition of "being driven by keyframes"
    /// used by the `_animating_*` environment flags.
    pub fn is_field_currently_animating(&self, field: ActorField, time_ms: u64) -> bool {
        self.field_ref(field).is_some_and(|f| match f {
            TrackFieldRef::F32(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::Vec2(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::Vec4(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::Transform(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::String(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::U32(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::PointList(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::CommandList(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::ShapeType(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::PlacementMode(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::CalloutPlace(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::MorphOptions(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::VectorPaths(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::TextPaths(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            #[cfg(feature = "render")]
            TrackFieldRef::Image(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::PositionBinding(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::Tagged(_, opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
        })
    }

    /// Check if a keyframe exists for the given property at exactly `time_ms`.
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    pub fn has_keyframe_at(&self, property: &str, time_ms: u64) -> bool {
        use ActorField::*;
        let field = match property {
            "position" => Position,
            "motion_offset" => MotionOffset,
            "size" => Size,
            "layout_size" => LayoutSize,
            "rotation" => Rotation,
            "scale" => Scale,
            "transform" => Transform,
            "color" => Color,
            "opacity" => Opacity,
            "stroke_width" => StrokeWidth,
            "stroke_color" => StrokeColor,
            "stroke_progress" => StrokeProgress,
            "fill_opacity" => FillOpacity,
            "filter_blur" => FilterBlur,
            "filter_brightness" => FilterBrightness,
            "filter_contrast" => FilterContrast,
            "filter_saturate" => FilterSaturate,
            "filter_hue_rotate" => FilterHueRotate,
            "filter_sepia" => FilterSepia,
            "shape_type" => ShapeType,
            "line_from" => LineFrom,
            "line_to" => LineTo,
            "arc_angles" => ArcAngles,
            "points" => Points,
            "commands" => Commands,
            "head_size" => HeadSize,
            "text_content" => TextContent,
            "font_family" => FontFamily,
            "font_size" => FontSize,
            "font_weight" => FontWeight,
            "font_style" => FontStyle,
            "line_height" => LineHeight,
            "letter_spacing" => LetterSpacing,
            "word_spacing" => WordSpacing,
            "max_width" => TextMaxWidth,
            "text_align" => TextAlign,
            "overflow" => Overflow,
            "placement_mode" => PlacementMode,
            "morph_options" => MorphOptions,
            "legend" => Tagged("legend"),
            "url" => ImageData,
            _ => return false,
        };

        if field == ImageData && self.kind == ActorKindId::Svg {
            return self
                .svg_paths_track
                .as_ref()
                .is_some_and(|track| track.keyframes.contains_key(&time_ms));
        }

        self.field_ref(field).is_some_and(|f| f.has_keyframe_at(time_ms))
    }

    /// Check if the property has any keyframes at all (regardless of time).
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    pub fn has_keyframes_for(&self, property: &str) -> bool {
        use ActorField::*;
        let field = match property {
            "position" => Position,
            "motion_offset" => MotionOffset,
            "size" => Size,
            "layout_size" => LayoutSize,
            "rotation" => Rotation,
            "scale" => Scale,
            "transform" => Transform,
            "color" => Color,
            "opacity" => Opacity,
            "stroke_width" => StrokeWidth,
            "stroke_color" => StrokeColor,
            "stroke_progress" => StrokeProgress,
            "fill_opacity" => FillOpacity,
            "filter_blur" => FilterBlur,
            "filter_brightness" => FilterBrightness,
            "filter_contrast" => FilterContrast,
            "filter_saturate" => FilterSaturate,
            "filter_hue_rotate" => FilterHueRotate,
            "filter_sepia" => FilterSepia,
            "shape_type" => ShapeType,
            "line_from" => LineFrom,
            "line_to" => LineTo,
            "arc_angles" => ArcAngles,
            "points" => Points,
            "commands" => Commands,
            "head_size" => HeadSize,
            "text_content" => TextContent,
            "font_family" => FontFamily,
            "font_size" => FontSize,
            "font_weight" => FontWeight,
            "font_style" => FontStyle,
            "line_height" => LineHeight,
            "letter_spacing" => LetterSpacing,
            "word_spacing" => WordSpacing,
            "max_width" => TextMaxWidth,
            "text_align" => TextAlign,
            "overflow" => Overflow,
            "placement_mode" => PlacementMode,
            "morph_options" => MorphOptions,
            "legend" => Tagged("legend"),
            "url" => ImageData,
            _ => return false,
        };

        if field == ImageData && self.kind == ActorKindId::Svg {
            return self.svg_paths_track.as_ref().is_some_and(|track| !track.keyframes.is_empty());
        }

        self.field_ref(field).is_some_and(|f| f.keyframe_count() > 0)
    }

    /// List all keyframe times (in ms) for the given property.
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    /// Returns a sorted, deduplicated list of timestamps.
    pub fn list_keyframes(&self, property: &str) -> Vec<u64> {
        use ActorField::*;
        let field = match property {
            "position" => Position,
            "motion_offset" => MotionOffset,
            "size" => Size,
            "layout_size" => LayoutSize,
            "rotation" => Rotation,
            "scale" => Scale,
            "transform" => Transform,
            "color" => Color,
            "opacity" => Opacity,
            "stroke_width" => StrokeWidth,
            "stroke_color" => StrokeColor,
            "stroke_progress" => StrokeProgress,
            "fill_opacity" => FillOpacity,
            "filter_blur" => FilterBlur,
            "filter_brightness" => FilterBrightness,
            "filter_contrast" => FilterContrast,
            "filter_saturate" => FilterSaturate,
            "filter_hue_rotate" => FilterHueRotate,
            "filter_sepia" => FilterSepia,
            "shape_type" => ShapeType,
            "line_from" => LineFrom,
            "line_to" => LineTo,
            "arc_angles" => ArcAngles,
            "points" => Points,
            "commands" => Commands,
            "head_size" => HeadSize,
            "text_content" => TextContent,
            "font_family" => FontFamily,
            "font_size" => FontSize,
            "font_weight" => FontWeight,
            "font_style" => FontStyle,
            "line_height" => LineHeight,
            "letter_spacing" => LetterSpacing,
            "word_spacing" => WordSpacing,
            "max_width" => TextMaxWidth,
            "text_align" => TextAlign,
            "overflow" => Overflow,
            "placement_mode" => PlacementMode,
            "morph_options" => MorphOptions,
            "legend" => Tagged("legend"),
            "url" => ImageData,
            _ => return Vec::new(),
        };

        let mut times: Vec<u64> = if field == ImageData && self.kind == ActorKindId::Svg {
            self.svg_paths_track
                .as_ref()
                .map_or(Vec::new(), |track| track.keyframes.keys().copied().collect())
        } else {
            self.field_ref(field).map(|f| f.keyframe_times()).unwrap_or_default()
        };
        times.sort_unstable();
        times.dedup();
        times
    }
}

// ─────────────────────────────────────────────────────────────
// Free functions: Property value reading & keyframe introspection
// ─────────────────────────────────────────────────────────────

fn svg_paths_track_for(
    track: &AnimationTrack,
    field: ActorField,
) -> Option<&PropertyTrack<Option<Vec<VelloPath>>>> {
    if track.kind == ActorKindId::Svg && field == ActorField::ImageData {
        track.svg_paths_track.as_ref()
    } else {
        None
    }
}

/// Read the current value of a property from a track at the given time.
/// Returns `None` if the property has no track (not set on this actor).
pub fn read_property_value(
    track: &AnimationTrack,
    field: ActorField,
    time_ms: u64,
) -> Option<PropertyValue> {
    track.field_ref(field).and_then(|f| f.evaluate_value(time_ms))
}

/// Read a property value, falling back to the schema default if the track
/// has no value for this property.
pub fn read_property_value_or_default(
    track: &AnimationTrack,
    schema: &PropertySchema,
    time_ms: u64,
) -> PropertyValue {
    read_property_value(track, schema.field, time_ms)
        .unwrap_or_else(|| (schema.default_value)(track.kind))
}

/// Returns whether a property has any keyframes on the given track.
pub fn property_has_keyframes(track: &AnimationTrack, field: ActorField) -> bool {
    property_keyframe_count(track, field) > 0
}

/// Returns whether a property has a keyframe at exactly the given time.
pub fn property_has_keyframe_at(track: &AnimationTrack, field: ActorField, time_ms: u64) -> bool {
    if let Some(svg_track) = svg_paths_track_for(track, field) {
        return svg_track.keyframes.contains_key(&time_ms);
    }
    track.field_ref(field).is_some_and(|f| f.has_keyframe_at(time_ms))
}

/// Returns the number of keyframes for a property on the given track.
pub fn property_keyframe_count(track: &AnimationTrack, field: ActorField) -> usize {
    if let Some(svg_track) = svg_paths_track_for(track, field) {
        return svg_track.keyframes.len();
    }
    track.field_ref(field).map_or(0, |f| f.keyframe_count())
}

/// Returns all keyframe times (in ms) for a property, sorted.
pub fn property_keyframe_times(track: &AnimationTrack, field: ActorField) -> Vec<u64> {
    if let Some(svg_track) = svg_paths_track_for(track, field) {
        let mut times: Vec<u64> = svg_track.keyframes.keys().copied().collect();
        times.sort_unstable();
        return times;
    }
    track.field_ref(field).map_or(Vec::new(), |f| f.keyframe_times())
}

/// Returns the easing at a specific keyframe time for a property.
pub fn property_keyframe_easing(
    track: &AnimationTrack,
    field: ActorField,
    time_ms: u64,
) -> Option<Easing> {
    if let Some(svg_track) = svg_paths_track_for(track, field) {
        return svg_track.keyframes.get(&time_ms).map(|(_, easing)| *easing);
    }
    track.field_ref(field).and_then(|f| f.keyframe_easing(time_ms))
}
