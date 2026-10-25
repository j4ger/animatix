use super::*;
use crate::easing::Easing;

#[derive(Clone, Debug)]
/// Immutable, type-erased reference to one property track on a track.
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
    ShapeType(&'a Option<PropertyTrack<crate::timeline::shapes::ShapeType>>),
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
    ShapeType(&'a mut Option<PropertyTrack<crate::timeline::shapes::ShapeType>>),
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

impl TrackFieldRef<'_> {
    /// Whether the referenced property track is interpolating at `time_ms`.
    pub fn is_currently_animating(&self, time_ms: u64) -> bool {
        macro_rules! animating {
            ($($variant:ident),* $(,)?) => {
                match self {
                    $(Self::$variant(track) => track
                        .as_ref()
                        .is_some_and(|track| track.is_currently_animating(time_ms)),)*
                    Self::Tagged(_, track) => track
                        .as_ref()
                        .is_some_and(|track| track.is_currently_animating(time_ms)),
                }
            };
        }
        animating!(
            F32,
            Vec2,
            Vec4,
            Transform,
            String,
            U32,
            F32List,
            Gradient,
            PointList,
            CommandList,
            ShapeType,
            PlacementMode,
            CalloutPlace,
            MorphOptions,
            VectorPaths,
            TextPaths,
            Image,
            PositionBinding,
        )
    }
}
