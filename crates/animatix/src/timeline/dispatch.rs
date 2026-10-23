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

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::animation_track::{CalloutPlace, PlacementMode, PositionBinding};
use super::property_track::PropertyTrack;
use crate::easing::Easing;
use crate::renderer::types::{GradientSpec, TextPath, VelloPath};
use crate::timeline::morph::MorphOptions;
use crate::timeline::property_engine::EnumPropertyValue;
pub use crate::timeline::property_engine::PropertyValue;
use crate::timeline::property_registry::{ActorField, PropertySchema};

pub use super::track::{AnimationTrack, apply_split_text_animation};

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
/// Primitive-specific state lives in exactly one place: [`Self::payload`]. An
/// actor carries at most one payload variant — a plot actor has a
/// [`crate::primitives::PlotPayload`], an `Svg` actor an
/// [`crate::primitives::SvgPayload`], an `Image` actor an
/// [`crate::primitives::ImagePayload`], and everything else
/// [`crate::primitives::PrimitivePayload::None`]. Read through the accessors
/// ([`Self::procedural_plot`], [`Self::func_transitions`],
/// [`Self::svg_paths`], [`Self::image_track`], …) rather than matching the enum,
/// and write through the `ensure_*_payload` forms — but only from the primitive
/// family that owns that variant, because `ensure_*` *replaces* the variant.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
    /// List of floats property track (stroke dash pattern).
    F32List(&'a Option<PropertyTrack<Vec<f32>>>),
    /// Command string property track.
    CommandList(&'a Option<PropertyTrack<String>>),
    /// Gradient paint property track.
    Gradient(&'a Option<PropertyTrack<GradientSpec>>),
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
    /// List of floats property track (stroke dash pattern).
    F32List(&'a mut Option<PropertyTrack<Vec<f32>>>),
    /// Command string property track.
    CommandList(&'a mut Option<PropertyTrack<String>>),
    /// Gradient paint property track.
    Gradient(&'a mut Option<PropertyTrack<GradientSpec>>),
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
            Self::F32List(opt) => {
                opt.as_ref().map(|pt| PropertyValue::F32List(pt.evaluate(time_ms)))
            },
            Self::Gradient(opt) => {
                opt.as_ref().map(|pt| PropertyValue::Gradient(pt.evaluate(time_ms)))
            },
            Self::CommandList(opt) => {
                opt.as_ref().map(|pt| PropertyValue::CommandList(pt.evaluate(time_ms)))
            },
            Self::VectorPaths(_) | Self::TextPaths(_) | Self::PositionBinding(_) => None,
            Self::Tagged(_, opt) => opt.as_ref().map(|pt| pt.evaluate(time_ms)),
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
            Self::F32List(opt) => {
                opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms))
            },
            Self::Gradient(opt) => {
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
            Self::F32List(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::Gradient(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::CommandList(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::ShapeType(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::PlacementMode(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::CalloutPlace(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::MorphOptions(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::VectorPaths(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::TextPaths(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
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
            Self::F32List(opt) => {
                opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect())
            },
            Self::Gradient(opt) => {
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
            Self::F32List(opt) => {
                opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e))
            },
            Self::Gradient(opt) => {
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
            ShapeType => TrackFieldRef::ShapeType(&self.shape.shape_type),
            LineFrom => TrackFieldRef::Vec2(&self.shape.line_from),
            LineTo => TrackFieldRef::Vec2(&self.shape.line_to),
            HeadSize => TrackFieldRef::F32(&self.shape.head_size),
            LineCap => TrackFieldRef::U32(&self.style.line_cap),
            LineJoin => TrackFieldRef::U32(&self.style.line_join),
            ArcAngles => TrackFieldRef::Vec2(&self.shape.arc_angles),
            CornerRadius => TrackFieldRef::F32(&self.shape.corner_radius),
            Points => TrackFieldRef::PointList(&self.shape.points),
            DashPattern => TrackFieldRef::F32List(&self.style.dash_pattern),
            FillGradient => TrackFieldRef::Gradient(&self.style.fill_gradient),
            StrokeGradient => TrackFieldRef::Gradient(&self.style.stroke_gradient),
            GradientExtend => TrackFieldRef::String(&self.style.gradient_extend),
            GradientSpace => TrackFieldRef::String(&self.style.gradient_space),
            DashOffset => TrackFieldRef::F32(&self.style.dash_offset),
            Blend => TrackFieldRef::String(&self.style.blend),
            Commands => TrackFieldRef::CommandList(&self.shape.commands),
            TextContent => TrackFieldRef::String(&self.text.text_content),
            TextMaxWidth => TrackFieldRef::F32(&self.text.text_max_width),
            TextAlign => TrackFieldRef::String(&self.text.text_align),
            TextAnchor => TrackFieldRef::String(&self.text.text_anchor),
            Overflow => TrackFieldRef::String(&self.text.overflow),
            Language => TrackFieldRef::String(&self.text.language),
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
            // The image payload owns the raster track; an actor without that
            // payload has no image data, which reads as `None`.
            ImageData => return self.image_payload().map(|p| TrackFieldRef::Image(&p.image)),
            PositionBinding => TrackFieldRef::PositionBinding(&self.geometry.position_binding),
            ActorField::Tagged(name) => {
                return self
                    .tagged_tracks
                    .get(name)
                    .map(|track| TrackFieldRef::Tagged(name, track));
            },
            AudioVolume => TrackFieldRef::F32(&self.geometry.volume),
            // Remaining variants without track storage
            SvgPaths | AudioSource | PositionBindingGroup | VectorShapeGroup | PlotDomainGroup
            | ContainerLayoutGroup | NoStorage => return None,
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
            ShapeType => TrackFieldMut::ShapeType(&mut self.shape.shape_type),
            LineFrom => TrackFieldMut::Vec2(&mut self.shape.line_from),
            LineTo => TrackFieldMut::Vec2(&mut self.shape.line_to),
            HeadSize => TrackFieldMut::F32(&mut self.shape.head_size),
            LineCap => TrackFieldMut::U32(&mut self.style.line_cap),
            LineJoin => TrackFieldMut::U32(&mut self.style.line_join),
            ArcAngles => TrackFieldMut::Vec2(&mut self.shape.arc_angles),
            CornerRadius => TrackFieldMut::F32(&mut self.shape.corner_radius),
            Points => TrackFieldMut::PointList(&mut self.shape.points),
            DashPattern => TrackFieldMut::F32List(&mut self.style.dash_pattern),
            FillGradient => TrackFieldMut::Gradient(&mut self.style.fill_gradient),
            StrokeGradient => TrackFieldMut::Gradient(&mut self.style.stroke_gradient),
            GradientExtend => TrackFieldMut::String(&mut self.style.gradient_extend),
            GradientSpace => TrackFieldMut::String(&mut self.style.gradient_space),
            DashOffset => TrackFieldMut::F32(&mut self.style.dash_offset),
            Blend => TrackFieldMut::String(&mut self.style.blend),
            Commands => TrackFieldMut::CommandList(&mut self.shape.commands),
            TextContent => TrackFieldMut::String(&mut self.text.text_content),
            TextMaxWidth => TrackFieldMut::F32(&mut self.text.text_max_width),
            TextAlign => TrackFieldMut::String(&mut self.text.text_align),
            TextAnchor => TrackFieldMut::String(&mut self.text.text_anchor),
            Overflow => TrackFieldMut::String(&mut self.text.overflow),
            Language => TrackFieldMut::String(&mut self.text.language),
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
            AudioVolume => TrackFieldMut::F32(&mut self.geometry.volume),
            // Image data is written through the media/image paths (which hold
            // the asset cache), never through generic property dispatch.
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
            TrackFieldRef::F32List(opt) => {
                opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms))
            },
            TrackFieldRef::Gradient(opt) => {
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

    /// The one `property name → ActorField` table.
    ///
    /// Three introspection queries (`has_keyframe_at`, `has_keyframes_for`,
    /// `list_keyframes`) each carried a copy of this mapping, verified identical
    /// apart from the fall-through arm — which is what each caller keeps for itself
    /// below. A fourth copy is how a property ends up answering one query and not
    /// another.
    fn field_for_property(property: &str) -> Option<ActorField> {
        use ActorField::*;
        Some(match property {
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
            "stroke_progress" => StrokeProgress,
            "dash_offset" => DashOffset,
            "dash_pattern" => DashPattern,
            "fill_gradient" => FillGradient,
            "stroke_gradient" => StrokeGradient,
            "gradient_extend" => GradientExtend,
            "gradient_space" => GradientSpace,
            "blend" => Blend,
            "fill_opacity" => FillOpacity,
            "shape_type" => ShapeType,
            "line_from" => LineFrom,
            "line_to" => LineTo,
            "arc_angles" => ArcAngles,
            "corner_radius" => CornerRadius,
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
            "text_anchor" => TextAnchor,
            "overflow" => Overflow,
            "language" => Language,
            "placement_mode" => PlacementMode,
            "morph_options" => MorphOptions,
            "legend" => Tagged("legend"),
            "url" => ImageData,
            _ => return None,
        })
    }

    /// Check if a keyframe exists for the given property at exactly `time_ms`.
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    pub fn has_keyframe_at(&self, property: &str, time_ms: u64) -> bool {
        use ActorField::*;
        let Some(field) = Self::field_for_property(property) else {
            return false;
        };

        if field == ImageData && self.caps.vector_paths {
            return self
                .svg_payload()
                .and_then(|p| p.svg_paths_track.as_ref())
                .is_some_and(|track| track.keyframes.contains_key(&time_ms));
        }

        self.field_ref(field).is_some_and(|f| f.has_keyframe_at(time_ms))
    }

    /// Check if the property has any keyframes at all (regardless of time).
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    pub fn has_keyframes_for(&self, property: &str) -> bool {
        use ActorField::*;
        let Some(field) = Self::field_for_property(property) else {
            return false;
        };

        if field == ImageData && self.caps.vector_paths {
            return self
                .svg_payload()
                .and_then(|p| p.svg_paths_track.as_ref())
                .is_some_and(|track| !track.keyframes.is_empty());
        }

        self.field_ref(field).is_some_and(|f| f.keyframe_count() > 0)
    }

    /// True when the author animated `property`: the track holds at least two
    /// keyframes that do not all carry the same value.
    ///
    /// [`Self::has_keyframes_for`] cannot answer this. The build stamps a
    /// keyframe for most *declared* properties (`insert_end_keyframes`, plus a
    /// start keyframe when the scene has a duration), so a non-zero keyframe
    /// count says the property was written down, not that anything moves. That
    /// is why `always-overrides-keyframes` fired on scenes that animate nothing:
    /// a `dash_pattern` in a declaration seeds a constant `dash_offset`, and an
    /// `always` writing it was reported as overriding "keyframe animation".
    ///
    /// A single keyframe is treated as not-animated, and that is a deliberate
    /// loss. It would be nice to keep warning on `#1s box.opacity = 0.8` beside
    /// an `always` writing opacity, but a lone keyframe is bit-for-bit what
    /// declaration seeding produces — the seeder writes the *declared* value, not
    /// the registry default, so even `size: (100, 60)` lands as one keyframe at a
    /// value no default matches. Telling the two apart needs keyframe provenance,
    /// which tracks do not carry (the storage is `InlineKeyframes<T>`).
    /// A constant value costs nothing to override, so the false positives cost
    /// more than the case is worth.
    pub(crate) fn is_property_animated(&self, property: &str) -> bool {
        let times = self.list_keyframes(property);
        if times.len() < 2 {
            return false;
        }
        let Some(schema) = crate::timeline::property_registry::lookup_property(property) else {
            return true;
        };
        // Fields whose tracks cannot be sampled as values (path, image and
        // binding tracks) keep the coarser rule: two keyframes is animation.
        let Some(field) = self.field_ref(schema.field) else {
            return true;
        };
        let Some(base) = field.evaluate_value(times[0]) else {
            return true;
        };
        times[1..]
            .iter()
            .any(|time_ms| field.evaluate_value(*time_ms).is_none_or(|value| value != base))
    }

    /// List all keyframe times (in ms) for the given property.
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    /// Returns a sorted, deduplicated list of timestamps.
    pub fn list_keyframes(&self, property: &str) -> Vec<u64> {
        use ActorField::*;
        let Some(field) = Self::field_for_property(property) else {
            return Vec::new();
        };

        let mut times: Vec<u64> = if field == ImageData && self.caps.vector_paths {
            self.svg_payload()
                .and_then(|p| p.svg_paths_track.as_ref())
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
    if track.caps.vector_paths && field == ActorField::ImageData {
        track.svg_payload().and_then(|p| p.svg_paths_track.as_ref())
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
    if let ActorField::Tagged(name) = field
        && name != "legend"
        && name != "callout_place"
        && let Some(id) = crate::timeline::property_registry::property_id(name)
        && let Some(value) = super::property_engine::read_property_plan_slot(track, id, time_ms)
    {
        return Some(value);
    }
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
        .unwrap_or_else(|| (schema.default_value)(&track.caps))
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
    if let Some(id) = plan_id_for_tagged_field(&field)
        && track.property_plan.has_keyframe_at(id, time_ms)
    {
        return true;
    }
    track.field_ref(field).is_some_and(|f| f.has_keyframe_at(time_ms))
}

/// Returns the number of keyframes for a property on the given track.
pub fn property_keyframe_count(track: &AnimationTrack, field: ActorField) -> usize {
    if let Some(svg_track) = svg_paths_track_for(track, field) {
        return svg_track.keyframes.len();
    }
    if let Some(id) = plan_id_for_tagged_field(&field)
        && track.property_plan.keyframe_count(id) > 0
    {
        return track.property_plan.keyframe_count(id);
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
    if let Some(id) = plan_id_for_tagged_field(&field) {
        let times = track.property_plan.keyframe_times(id);
        if !times.is_empty() {
            return times;
        }
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
    if let Some(id) = plan_id_for_tagged_field(&field) {
        if let Some(easing) = track.property_plan.keyframe_easing(id, time_ms) {
            return Some(easing);
        }
    }
    track.field_ref(field).and_then(|f| f.keyframe_easing(time_ms))
}

/// Resolve a tagged field to a plan-backed property id, excluding legacy
/// special fields that still use typed/tagged storage.
fn plan_id_for_tagged_field(field: &ActorField) -> Option<animatix_syntax::schema::PropertyId> {
    match field {
        ActorField::Tagged(name) if *name != "legend" && *name != "callout_place" => {
            crate::timeline::property_registry::property_id(name)
        },
        _ => None,
    }
}

#[cfg(test)]
mod payload_tests {
    use super::AnimationTrack;
    use crate::primitives::PrimitivePayload;

    #[test]
    fn rect_track_carries_no_payload() {
        let track = AnimationTrack::new("box".to_string(), "Rect");
        assert!(matches!(track.payload, PrimitivePayload::None));
    }

    #[test]
    fn ensure_plot_payload_switches_variant() {
        let mut track = AnimationTrack::new("curve".to_string(), "PlotCurve");
        assert!(matches!(track.payload, PrimitivePayload::None));

        track
            .ensure_plot_payload()
            .plot_param_tracks
            .insert("freq".to_string(), super::PropertyTrack::new(2.0));
        assert!(matches!(track.payload, PrimitivePayload::Plot(_)));
        assert_eq!(track.plot_param_track("freq").map(|t| t.default_value), Some(2.0));

        // A second `ensure` must hand back the same payload, not reset it.
        assert_eq!(track.ensure_plot_payload().plot_param_tracks.len(), 1);
        assert_eq!(track.plot_param_track("freq").map(|t| t.default_value), Some(2.0));
    }

    #[test]
    fn svg_and_image_payloads_are_distinct() {
        let mut track = AnimationTrack::new("icon".to_string(), "Svg");
        track.ensure_svg_payload().svg_paths.push(crate::timeline::VelloPath::default());
        assert!(matches!(track.payload, PrimitivePayload::Svg(_)));
        assert!(track.image_payload().is_none());
        assert!(track.image_track().is_none());
        assert_eq!(track.svg_paths().len(), 1);
    }
}
