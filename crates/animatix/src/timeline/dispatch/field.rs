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

// TrackFieldRef convenience methods

/// Expand a uniform per-track body over every [`TrackFieldRef`] variant.
///
/// `$track` binds the variant's `&Option<PropertyTrack<T>>`; the body is
/// expanded once per variant, so each resampling call stays statically
/// dispatched and type-checked against its own payload. `Tagged` binds only
/// its payload — every caller of this macro ignores the tag. The same
/// expansion serves every per-track query (for example `is_currently_animating`).
macro_rules! map_track {
    ($self:expr, |$track:ident| $body:expr) => {
        match $self {
            Self::F32($track) => $body,
            Self::Vec2($track) => $body,
            Self::Vec4($track) => $body,
            Self::Transform($track) => $body,
            Self::String($track) => $body,
            Self::U32($track) => $body,
            Self::PointList($track) => $body,
            Self::F32List($track) => $body,
            Self::CommandList($track) => $body,
            Self::Gradient($track) => $body,
            Self::ShapeType($track) => $body,
            Self::PlacementMode($track) => $body,
            Self::CalloutPlace($track) => $body,
            Self::MorphOptions($track) => $body,
            Self::VectorPaths($track) => $body,
            Self::TextPaths($track) => $body,
            Self::Image($track) => $body,
            Self::PositionBinding($track) => $body,
            Self::Tagged(_, $track) => $body,
        }
    };
}

/// Build [`TrackFieldRef::evaluate_value`]'s exhaustive match from
/// conversion-shape groups, so the conversion table is stated once:
///
/// - `copy`: `evaluate_copy` into the named `PropertyValue` constructor
///   (`Vec4` becomes `Color`).
/// - `evaluate`: `evaluate` into the same-named constructor.
/// - `to_value`: copies the value and crosses the enum bridge.
/// - `none`: the deliberate semantic exception — these payloads have no
///   `PropertyValue` representation and are read through their dedicated
///   tracks instead of the generic value channel.
///
/// `MorphOptions` (summary string) and `Tagged` (already a `PropertyValue`)
/// are single-variant shapes and stay explicit below.
macro_rules! evaluate_value_arms {
    (
        $self:expr, $time:ident;
        copy: $( $copy_variant:ident => $copy_ctor:ident ),+ $(,)?;
        evaluate: $( $evaluate_variant:ident ),+ $(,)?;
        to_value: $( $to_value_variant:ident ),+ $(,)?;
        none: $( $none_variant:ident ),+ $(,)?;
    ) => {
        match $self {
            $(
                Self::$copy_variant(opt) => opt
                    .as_ref()
                    .map(|pt| PropertyValue::$copy_ctor(pt.evaluate_copy($time))),
            )+
            $(
                Self::$evaluate_variant(opt) => opt
                    .as_ref()
                    .map(|pt| PropertyValue::$evaluate_variant(pt.evaluate($time))),
            )+
            $(
                Self::$to_value_variant(opt) => opt
                    .as_ref()
                    .map(|pt| pt.evaluate_copy($time).to_property_value()),
            )+
            Self::MorphOptions(opt) => opt
                .as_ref()
                .map(|pt| PropertyValue::String(pt.evaluate_copy($time).summary())),
            // Semantic exception, kept explicit: these payloads (vector/text
            // paths, position bindings, raster images) are consumed by their
            // dedicated track readers and cannot be represented as a
            // `PropertyValue`.
            $( Self::$none_variant(_) => None, )+
            Self::Tagged(_, opt) => opt.as_ref().map(|pt| pt.evaluate($time)),
        }
    };
}

impl<'a> TrackFieldRef<'a> {
    /// Evaluate the track value at `time_ms`, returning the result as a `PropertyValue`.
    /// Returns `None` for types that cannot be represented as `PropertyValue`.
    pub fn evaluate_value(&self, time_ms: u64) -> Option<PropertyValue> {
        evaluate_value_arms!(
            self, time_ms;
            copy: F32 => F32, Vec2 => Vec2, Vec4 => Color, Transform => Transform, U32 => U32;
            evaluate: String, PointList, F32List, Gradient, CommandList;
            to_value: ShapeType, PlacementMode, CalloutPlace;
            none: VectorPaths, TextPaths, PositionBinding, Image;
        )
    }

    /// Returns `true` if this track has a keyframe at exactly `time_ms`.
    pub fn has_keyframe_at(&self, time_ms: u64) -> bool {
        map_track!(self, |track| track
            .as_ref()
            .is_some_and(|pt| pt.keyframes.contains_key(&time_ms)))
    }

    /// Returns the number of keyframes in this track.
    pub fn keyframe_count(&self) -> usize {
        map_track!(self, |track| track.as_ref().map_or(0, |pt| pt.keyframes.len()))
    }

    /// Returns all keyframe timestamps (ms), sorted.
    pub fn keyframe_times(&self) -> Vec<u64> {
        map_track!(self, |track| track.as_ref().map_or(Vec::new(), |pt| pt
            .keyframes
            .keys()
            .copied()
            .collect()))
    }

    /// Returns the easing at a specific keyframe time, if one exists.
    pub fn keyframe_easing(&self, time_ms: u64) -> Option<Easing> {
        map_track!(self, |track| track
            .as_ref()
            .and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, easing)| *easing)))
    }

    /// Whether the referenced property track is interpolating at `time_ms`.
    pub fn is_currently_animating(&self, time_ms: u64) -> bool {
        map_track!(self, |track| track
            .as_ref()
            .is_some_and(|pt| pt.is_currently_animating(time_ms)))
    }
}
