use crate::easing::{Easing, apply_easing};
use crate::renderer::types::{TextPath, VelloPath};
use crate::timeline::morph::{
    MorphOptions, MorphStrategy, align_path_lists_with_strategy, morph_paths_with_options,
};
use crate::timeline::plot::ProceduralPlot;
use crate::timeline::shapes::ShapeType;
use std::collections::BTreeMap;
use std::collections::HashMap;

/// Default half-size for layout bounds (`[50.0, 50.0]`).
pub const DEFAULT_LAYOUT_HALF_SIZE: [f32; 2] = [50.0, 50.0];
/// Default white color in RGBA (`[1.0, 1.0, 1.0, 1.0]`).
pub const DEFAULT_WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

// ─────────────────────────────────────────────────────────────
// Action event metadata (for GUI timeline visualization)
// ─────────────────────────────────────────────────────────────

/// A recorded action event in the timeline.
///
/// Actions are processed at build time into keyframes, but their metadata
/// is retained for GUI visualization (colored blocks in the timeline).
#[derive(Clone, Debug, PartialEq)]
pub struct ActionEvent {
    /// Action verb (e.g. "fade-in", "move", "rotate").
    pub verb: String,
    /// Target actor labels.
    pub targets: Vec<String>,
    /// Start time in milliseconds.
    pub start_time_ms: u64,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Easing curve used.
    pub easing: Easing,
    /// Action category for UI color coding.
    pub category: ActionCategory,
}

/// Category of an action for UI color coding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionCategory {
    /// Entrance actions (fade-in, wipe-in, etc.) — green.
    Entrance,
    /// Motion actions (move, shift, rotate, scale) — blue.
    Motion,
    /// Exit actions (fade-out, wipe-out) — red.
    Exit,
    /// Effect actions (bounce, pulse, shake) — amber.
    Effect,
    /// Reorder actions (swap, reorder) — purple.
    Reorder,
    /// Reveal actions (draw-in, reveal-in, draw-out, reveal-out) — cyan.
    Reveal,
}

// ─────────────────────────────────────────────────────────────
// Actor kind identification
// ─────────────────────────────────────────────────────────────

/// Stable, compile-time constant identifying an actor's type.
/// Set once at first declaration and never changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActorKindId {
    /// Geometric shape (rect, ellipse, line, polygon, path).
    Shape(ShapeKind),
    /// Plain text actor.
    Text,
    /// Code block actor.
    Code,
    /// Typst document actor.
    Typst,
    /// Raster image actor.
    Image,
    /// SVG graphic actor.
    Svg,
    /// Graph / chart actor.
    Graph,
    /// Single curve plot actor.
    PlotCurve,
    /// Vector field visualization actor.
    VectorField,
    /// Heatmap visualization actor.
    Heatmap,
    /// Contour set visualization actor.
    ContourSet,
    /// Number plane / coordinate grid actor.
    NumberPlane,
    /// Bar chart / column chart actor.
    BarChart,
    /// Horizontal row layout container.
    Row,
    /// Vertical column layout container.
    Col,
    /// Grid layout container.
    Grid,
    /// Stack layout container.
    Stack,
    /// Generic group container.
    Group,
    /// Mask / clip container.
    Mask,
    /// Filter / post-processing container.
    Filter,
    /// Audio track actor.
    Audio,
    /// Equation container (Typst math with fragment highlighting).
    Equation,
    /// Fragment sub-item within an Equation.
    Fragment,
}

impl ActorKindId {
    /// Parse an actor kind from its type name (e.g. `"rect"`, `"text"`).
    pub fn from_type_name(ty: &str) -> Option<Self> {
        crate::primitives::find_primitive(ty).map(|p| p.kind_id())
    }
}

/// Specific shape geometry variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    /// Axis-aligned rectangle.
    Rect,
    /// Ellipse (or circle).
    Ellipse,
    /// Straight line segment.
    Line,
    /// Closed polygon.
    Polygon,
    /// Arbitrary Bézier path.
    Path,
    /// Arrow with a dedicated arrowhead.
    Arrow,
}

impl From<super::shapes::ShapeType> for ShapeKind {
    fn from(st: super::shapes::ShapeType) -> Self {
        match st {
            super::shapes::ShapeType::Rect => Self::Rect,
            super::shapes::ShapeType::Ellipse => Self::Ellipse,
            super::shapes::ShapeType::Line => Self::Line,
            super::shapes::ShapeType::Polygon => Self::Polygon,
            super::shapes::ShapeType::Path => Self::Path,
            super::shapes::ShapeType::Graph => Self::Rect,
            super::shapes::ShapeType::Plot => Self::Rect,
            super::shapes::ShapeType::Arrow => Self::Arrow,
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Actor kind metadata registry
// ─────────────────────────────────────────────────────────────

/// High-level category for grouping actor kinds in UI palettes and docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActorCategory {
    /// Geometric shapes (rect, ellipse, etc.).
    Shape,
    /// Text and typographic actors.
    Text,
    /// Image, SVG, and audio actors.
    Media,
    /// Plot and graph actors.
    Plot,
    /// Layout containers (row, column, grid, etc.).
    Container,
}

impl ActorCategory {
    /// Human-readable label for this category.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Shape => "Shapes",
            Self::Text => "Text",
            Self::Media => "Media",
            Self::Plot => "Plots",
            Self::Container => "Containers",
        }
    }
}

pub use crate::primitives::ActorKindMeta;

/// Global registry of all supported actor kinds.
pub fn actor_kind_registry() -> &'static [ActorKindMeta] {
    crate::primitives::actor_kind_registry()
}

/// Lookup metadata for a specific [`ActorKindId`].
pub fn actor_kind_meta(kind: ActorKindId) -> Option<&'static ActorKindMeta> {
    crate::primitives::actor_kind_meta(kind)
}

/// Lookup metadata by the actor's type name (e.g. `"rect"`, `"text"`).
pub fn actor_kind_meta_by_name(name: &str) -> Option<&'static ActorKindMeta> {
    crate::primitives::actor_kind_meta_by_name(name)
}

/// Extension trait for lazy property track access.
pub trait TrackAccessor<T: Interpolate> {
    /// Evaluate the track at `time_ms`, falling back to `default` if empty.
    fn get(&self, time_ms: u64, default: T) -> T;
    /// Evaluate the track at `time_ms`, returning `None` when no track exists.
    fn get_or_default(&self, time_ms: u64) -> Option<T>;
    /// Ensure the track exists, creating it with `default` if absent.
    fn ensure(&mut self, default: T) -> &mut PropertyTrack<T>;
    /// Return the value of the last keyframe, or `default` if empty.
    fn last(&self, default: T) -> T;
    /// Return the timestamp of the last keyframe, if any.
    fn last_time(&self) -> Option<u64>;
    /// Check whether a keyframe exists at exactly `time_ms`.
    fn has_keyframe_at(&self, time_ms: u64) -> bool;
}

impl<T: Interpolate> TrackAccessor<T> for Option<PropertyTrack<T>> {
    fn get(&self, time_ms: u64, default: T) -> T {
        self.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(default)
    }
    fn get_or_default(&self, time_ms: u64) -> Option<T> {
        self.as_ref().map(|t| t.evaluate(time_ms))
    }
    fn ensure(&mut self, default: T) -> &mut PropertyTrack<T> {
        self.get_or_insert_with(|| PropertyTrack::new(default))
    }
    fn last(&self, default: T) -> T {
        self.as_ref().map(|t| t.last_value()).unwrap_or(default)
    }
    fn last_time(&self) -> Option<u64> {
        self.as_ref().and_then(|t| t.last_keyframe_time())
    }
    fn has_keyframe_at(&self, time_ms: u64) -> bool {
        self.as_ref().map(|t| t.keyframes.contains_key(&time_ms)).unwrap_or(false)
    }
}

/// Trait for values that can be interpolated between two states.
pub trait Interpolate: Clone {
    /// Interpolate between `self` and `other` using parameter `t` in `[0, 1]`.
    fn interpolate(&self, other: &Self, t: f32) -> Self;
}

impl Interpolate for f32 {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        self + (other - self) * t.clamp(0.0, 1.0)
    }
}

impl Interpolate for f64 {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        self + (other - self) * t.clamp(0.0, 1.0) as f64
    }
}

impl Interpolate for [f32; 2] {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        [self[0] + (other[0] - self[0]) * t, self[1] + (other[1] - self[1]) * t]
    }
}

impl Interpolate for [f32; 4] {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        [
            self[0] + (other[0] - self[0]) * t,
            self[1] + (other[1] - self[1]) * t,
            self[2] + (other[2] - self[2]) * t,
            self[3] + (other[3] - self[3]) * t,
        ]
    }
}

impl Interpolate for [f32; 6] {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        [
            self[0] + (other[0] - self[0]) * t,
            self[1] + (other[1] - self[1]) * t,
            self[2] + (other[2] - self[2]) * t,
            self[3] + (other[3] - self[3]) * t,
            self[4] + (other[4] - self[4]) * t,
            self[5] + (other[5] - self[5]) * t,
        ]
    }
}

impl Interpolate for u32 {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

/// Controls whether an actor's position is managed by a layout container.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementMode {
    /// Position is computed by the parent layout container.
    LayoutManaged,
    /// Position is set manually, ignoring layout.
    Manual,
}

/// Controls how an actor's dimensions are interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeMode {
    /// Absolute width and height.
    Size,
    /// Uniform scale factor.
    Scale,
}

impl Interpolate for PlacementMode {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

/// Anchor point on a 3×3 scene grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneAnchor {
    /// Top-left corner.
    TopLeft,
    /// Top edge center.
    Top,
    /// Top-right corner.
    TopRight,
    /// Left edge center.
    Left,
    /// Center of the scene.
    Center,
    /// Right edge center.
    Right,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom edge center.
    Bottom,
    /// Bottom-right corner.
    BottomRight,
}

impl Interpolate for SceneAnchor {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

/// Strategy for binding an actor's position to a reference frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PositionBinding {
    /// Absolute coordinates in the scene.
    Absolute,
    /// Offset from a [`SceneAnchor`].
    SceneAnchor {
        /// Anchor point on the scene.
        anchor: SceneAnchor,
        /// Pixel offset from the anchor.
        offset: [f32; 2],
    },
    /// Percentage-based position within the scene, plus offset.
    ScenePercent {
        /// Horizontal percentage (0–1).
        x: f32,
        /// Vertical percentage (0–1).
        y: f32,
        /// Pixel offset from the computed position.
        offset: [f32; 2],
    },
    /// Default position inside a container, anchored at a [`SceneAnchor`].
    ContainerDefault {
        /// Anchor point within the container.
        anchor: SceneAnchor,
    },
    /// Percentage-based position inside a container.
    ContainerPercent {
        /// Horizontal percentage (0–1).
        x: f32,
        /// Vertical percentage (0–1).
        y: f32,
    },
}

impl Interpolate for PositionBinding {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        match (*self, *other) {
            (Self::Absolute, Self::Absolute) => Self::Absolute,
            (Self::SceneAnchor { anchor, offset: start_offset }, Self::SceneAnchor { anchor: other_anchor, offset: end_offset }) if anchor == other_anchor => Self::SceneAnchor { anchor, offset: start_offset.interpolate(&end_offset, t) },
            (Self::ScenePercent { x: start_x, y: start_y, offset: start_offset }, Self::ScenePercent { x: end_x, y: end_y, offset: end_offset }) => Self::ScenePercent { x: start_x.interpolate(&end_x, t), y: start_y.interpolate(&end_y, t), offset: start_offset.interpolate(&end_offset, t) },
            (Self::ContainerDefault { anchor }, Self::ContainerDefault { anchor: other_anchor }) if anchor == other_anchor => Self::ContainerDefault { anchor },
            (Self::ContainerPercent { x: x1, y: y1 }, Self::ContainerPercent { x: x2, y: y2 }) => Self::ContainerPercent { x: x1.interpolate(&x2, t), y: y1.interpolate(&y2, t) },
            _ => { if t < 0.5 { *self } else { *other } }
        }
    }
}

impl Interpolate for MorphOptions {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

impl Interpolate for Vec<TextPath> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        interpolate_text_paths(self, other, t, MorphOptions::default())
    }
}

fn lerp_color(c1: vello::peniko::Color, c2: vello::peniko::Color, t: f32) -> vello::peniko::Color {
    let t = t.clamp(0.0, 1.0);
    let comp1 = c1.to_rgba8();
    let comp2 = c2.to_rgba8();
    vello::peniko::Color::from_rgba8(
        (comp1.r as f32 + (comp2.r as f32 - comp1.r as f32) * t) as u8,
        (comp1.g as f32 + (comp2.g as f32 - comp1.g as f32) * t) as u8,
        (comp1.b as f32 + (comp2.b as f32 - comp1.b as f32) * t) as u8,
        (comp1.a as f32 + (comp2.a as f32 - comp1.a as f32) * t) as u8,
    )
}

impl Interpolate for Vec<VelloPath> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        interpolate_vello_paths(self, other, t, MorphOptions::default())
    }
}

#[cfg(feature = "render")]
impl Interpolate for Option<crate::timeline::image::SceneImage> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { self.clone() } else { other.clone() }
    }
}

impl Interpolate for String {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { self.clone() } else { other.clone() }
    }
}

impl Interpolate for Vec<String> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { self.clone() } else { other.clone() }
    }
}

impl Interpolate for Vec<[f32; 2]> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if self.is_empty() || other.is_empty() || self.len() != other.len() {
            if t < 0.5 { self.clone() } else { other.clone() }
        } else {
            self.iter().zip(other.iter()).map(|(a, b)| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]).collect()
        }
    }
}

/// A keyed animation track holding values of type `T` over time.
#[derive(Debug)]
pub struct PropertyTrack<T> {
    /// Map from timestamp (ms) to `(value, easing)` pairs.
    pub(crate) keyframes: BTreeMap<u64, (T, Easing)>,
    /// Value used when no keyframes are defined.
    pub(crate) default_value: T,
    /// P2.20: Memoization cache for repeated time queries.
    last_evaluated: std::cell::RefCell<Option<(u64, T)>>,
}

impl<T: Interpolate> Clone for PropertyTrack<T> {
    fn clone(&self) -> Self {
        Self {
            keyframes: self.keyframes.clone(),
            default_value: self.default_value.clone(),
            last_evaluated: std::cell::RefCell::new(None),
        }
    }
}

impl<T: Interpolate> PropertyTrack<T> {
    /// Create a new track with the given default value.
    pub fn new(default_value: T) -> Self {
        Self { keyframes: BTreeMap::new(), default_value, last_evaluated: std::cell::RefCell::new(None) }
    }
    /// Insert a keyframe at `time_ms` with `value` and `easing`.
    pub fn add_keyframe(&mut self, time_ms: u64, value: T, easing: Easing) {
        self.keyframes.insert(time_ms, (value, easing));
        // Invalidate memoization cache when keyframes change
        *self.last_evaluated.borrow_mut() = None;
    }
    /// Evaluate the interpolated value at `time_ms`.
    pub fn evaluate(&self, time_ms: u64) -> T {
        self.evaluate_with(time_ms, T::clone)
    }
    /// Optimized evaluate for `Copy` types — avoids heap allocation on clone.
    pub fn evaluate_copy(&self, time_ms: u64) -> T where T: Copy {
        self.evaluate_with(time_ms, |v| *v)
    }
    /// Returns `true` if this property track is currently inside an
    /// interpolation segment — there exists both a previous keyframe at
    /// `time <= time_ms` AND a next keyframe at `time > time_ms`.
    pub fn is_currently_animating(&self, time_ms: u64) -> bool {
        use std::ops::Bound;
        let next = self.keyframes.range((Bound::Excluded(time_ms), Bound::Unbounded)).next();
        let prev = self.keyframes.range(..=time_ms).next_back();
        matches!((prev, next), (Some(_), Some(_)))
    }
    /// Returns the interpolation segment for `time_ms`, if one exists between
    /// two keyframes. Returns `(found_time, prev_val, found_val, progress, found_easing)`
    /// where `progress` is in `(0, 1]`.
    fn interpolation_segment(&self, time_ms: u64) -> Option<(u64, &T, &T, f32, &Easing)> {
        let found = self.keyframes.range(time_ms..).next()?;
        let (&found_time, (found_val, found_easing)) = found;

        // Before or at first keyframe: no interior segment
        if let Some((&first_time, _)) = self.keyframes.iter().next() {
            if time_ms <= first_time {
                return None;
            }
        }

        // Find the previous keyframe before time_ms
        let (prev_time, (prev_val, _)) = self.keyframes.range(..time_ms).next_back()?;

        let duration = (found_time - prev_time) as f32;
        let elapsed = (time_ms - prev_time) as f32;
        let progress = elapsed / duration;

        Some((found_time, prev_val, found_val, progress, found_easing))
    }
    /// Core evaluation logic parameterized by clone strategy.
    fn evaluate_with(&self, time_ms: u64, clone_val: impl Fn(&T) -> T) -> T {
        // P2.20: Memoization — return cached value if time matches
        if let Some((cached_time, cached_value)) = self.last_evaluated.borrow().as_ref() {
            if *cached_time == time_ms {
                return clone_val(cached_value);
            }
        }

        let result = if let Some((_found_time, prev_val, found_val, progress, found_easing)) = self.interpolation_segment(time_ms) {
            let eased_progress = apply_easing(progress, *found_easing);
            prev_val.interpolate(found_val, eased_progress)
        } else {
            // No interior segment — use default or boundary value
            if self.keyframes.is_empty() {
                clone_val(&self.default_value)
            } else if let Some((&first_time, (val, _))) = self.keyframes.iter().next() {
                if time_ms <= first_time {
                    clone_val(val)
                } else {
                    let val = self.last_value_with(&clone_val);
                    *self.last_evaluated.borrow_mut() = Some((time_ms, clone_val(&val)));
                    return val;
                }
            } else {
                clone_val(&self.default_value)
            }
        };

        *self.last_evaluated.borrow_mut() = Some((time_ms, clone_val(&result)));
        result
    }
    /// Return the value of the most recent keyframe, or the default.
    pub fn last_value(&self) -> T {
        self.last_value_with(T::clone)
    }
    /// Return the value of the most recent keyframe, or the default, using a custom clone strategy.
    fn last_value_with(&self, clone_val: impl Fn(&T) -> T) -> T {
        self.keyframes.iter().next_back().map(|(_, (val, _))| clone_val(val))
            .unwrap_or_else(|| clone_val(&self.default_value))
    }
    /// Return the timestamp of the most recent keyframe, if any.
    pub fn last_keyframe_time(&self) -> Option<u64> {
        self.keyframes.keys().next_back().copied()
    }
    /// Returns true if this track has keyframes that could change value over time.
    /// A track with 0 keyframes or 1 keyframe at time 0 is effectively static.
    pub fn is_effectively_static(&self) -> bool {
        match self.keyframes.len() {
            0 => true,
            1 => self.keyframes.keys().next() == Some(&0),
            _ => false,
        }
    }

    /// Sets the default value and invalidates the memoization cache.
    pub fn set_default_value(&mut self, value: T) {
        self.default_value = value;
        *self.last_evaluated.borrow_mut() = None;
    }

    /// Returns a reference to the default value.
    pub fn default_value(&self) -> &T {
        &self.default_value
    }

    /// Returns a reference to the keyframes map.
    pub fn keyframes(&self) -> &BTreeMap<u64, (T, Easing)> {
        &self.keyframes
    }

    /// Returns a mutable reference to the keyframes map and invalidates the cache.
    pub fn keyframes_mut(&mut self) -> &mut BTreeMap<u64, (T, Easing)> {
        *self.last_evaluated.borrow_mut() = None;
        &mut self.keyframes
    }
}

// ─────────────────────────────────────────────────────────────
// FilterTracks sub-struct
// ─────────────────────────────────────────────────────────────

/// Sub-struct holding all filter-related property tracks.
#[derive(Clone, Debug, Default)]
pub struct FilterTracks {
    /// Gaussian blur radius.
    pub filter_blur: Option<PropertyTrack<f32>>,
    /// Brightness multiplier.
    pub filter_brightness: Option<PropertyTrack<f32>>,
    /// Contrast multiplier.
    pub filter_contrast: Option<PropertyTrack<f32>>,
    /// Saturation multiplier.
    pub filter_saturate: Option<PropertyTrack<f32>>,
    /// Hue rotation in degrees.
    pub filter_hue_rotate: Option<PropertyTrack<f32>>,
    /// Sepia intensity.
    pub filter_sepia: Option<PropertyTrack<f32>>,
}

// ─────────────────────────────────────────────────────────────
// HighlightTracks
// ─────────────────────────────────────────────────────────────

/// Per-actor highlight property tracks (for equation fragment highlights).
#[derive(Clone, Debug)]
pub struct HighlightTracks {
    /// Highlight background color (RGBA) for equation fragments.
    pub highlight_color: Option<PropertyTrack<[f32; 4]>>,
    /// Highlight opacity for equation fragments.
    pub highlight_opacity: Option<PropertyTrack<f32>>,
    /// Highlight padding (in logical pixels) around equation fragments.
    pub highlight_padding: Option<PropertyTrack<f32>>,
    /// Highlight corner radius for equation fragments.
    pub highlight_radius: Option<PropertyTrack<f32>>,
    /// Highlight blend mode for equation fragments (non-animated configuration).
    pub highlight_blend: vello::peniko::Mix,
}

impl Default for HighlightTracks {
    fn default() -> Self {
        Self {
            highlight_color: None,
            highlight_opacity: None,
            highlight_padding: None,
            highlight_radius: None,
            highlight_blend: vello::peniko::Mix::Difference,
        }
    }
}

// ─────────────────────────────────────────────────────────────
// AnimationTrack
// ─────────────────────────────────────────────────────────────

/// Per-actor animation track holding all animatable properties.
#[derive(Clone, Debug)]
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
    /// Whether the actor is visible in the preview and export.
    pub visible: bool,
    /// Whether the actor is locked (preventing selection and drag in the GUI).
    pub locked: bool,

    // ── Geometry tier (flat compat fields) ──
    /// Position track (x, y).
    pub position: Option<PropertyTrack<[f32; 2]>>,
    /// Motion offset applied after layout.
    pub motion_offset: Option<PropertyTrack<[f32; 2]>>,
    /// Rotation angle in radians.
    pub rotation: Option<PropertyTrack<f32>>,
    /// Uniform scale factor.
    pub scale: Option<PropertyTrack<f32>>,
    /// 2×3 affine transform matrix.
    pub transform: Option<PropertyTrack<[f32; 6]>>,
    /// Whether the actor is layout-managed or manually placed.
    pub placement_mode: Option<PropertyTrack<PlacementMode>>,
    /// Position binding strategy.
    pub position_binding: Option<PropertyTrack<PositionBinding>>,
    /// Width and height.
    pub size: Option<PropertyTrack<[f32; 2]>>,

    // ── Style tier (flat compat fields) ──
    /// Fill color in RGBA.
    pub color: Option<PropertyTrack<[f32; 4]>>,
    /// Overall opacity multiplier.
    pub opacity: Option<PropertyTrack<f32>>,
    /// Stroke width.
    pub stroke_width: Option<PropertyTrack<f32>>,
    /// Stroke color in RGBA.
    pub stroke_color: Option<PropertyTrack<[f32; 4]>>,
    /// Stroke draw progress (0–1).
    pub stroke_progress: Option<PropertyTrack<f32>>,
    /// Fill opacity multiplier.
    pub fill_opacity: Option<PropertyTrack<f32>>,
    /// Stroke line cap (0=Butt, 1=Round, 2=Square).
    pub line_cap: Option<PropertyTrack<u32>>,
    /// Stroke line join (0=Miter, 1=Round, 2=Bevel).
    pub line_join: Option<PropertyTrack<u32>>,
    /// Path morphing options.
    pub morph_options: Option<PropertyTrack<MorphOptions>>,

    // ── Filter tier (sub-struct) ──
    /// Filter property tracks (blur, brightness, contrast, etc.).
    pub filter: FilterTracks,

    // ── Shape payload (flat compat fields) ──
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
    /// Polygon vertex list.
    pub points: Option<PropertyTrack<Vec<[f32; 2]>>>,
    /// Path command string (e.g. SVG path data).
    pub commands: Option<PropertyTrack<String>>,
    /// Pre-built vector paths for shape rendering.
    pub vector_paths: Option<PropertyTrack<Vec<VelloPath>>>,

    // ── Text / media payload (flat compat fields) ──
    /// Raw text content.
    pub text_content: Option<PropertyTrack<String>>,
    /// Font family name.
    pub font_family: Option<PropertyTrack<String>>,
    /// Font size in points.
    pub font_size: Option<PropertyTrack<f32>>,
    /// Font weight (100–900).
    pub font_weight: Option<PropertyTrack<f32>>,
    /// Font style ("normal" | "italic").
    pub font_style: Option<PropertyTrack<String>>,
    /// Line height multiplier.
    pub line_height: Option<PropertyTrack<f32>>,
    /// Letter spacing in points.
    pub letter_spacing: Option<PropertyTrack<f32>>,
    /// Word spacing in points.
    pub word_spacing: Option<PropertyTrack<f32>>,
    /// Max width for text wrapping (0 = no wrap).
    pub text_max_width: Option<PropertyTrack<f32>>,
    /// Text alignment ("left", "center", "right", "justify").
    pub text_align: Option<PropertyTrack<String>>,
    /// Overflow behavior ("visible", "clip", "ellipsis").
    pub overflow: Option<PropertyTrack<String>>,
    /// Pre-built text paths for rendering.
    pub text_paths: Option<PropertyTrack<Vec<TextPath>>>,
    /// Static SVG paths.
    pub svg_paths: Vec<crate::timeline::VelloPath>,
    /// Raster image data.
    #[cfg(feature = "render")]
    pub image: Option<PropertyTrack<Option<crate::timeline::image::SceneImage>>>, 


    // ── Font metrics (Phase 6: baseline alignment) ──
    /// Font ascent in scene units (points), used for baseline alignment.
    pub ascent: Option<PropertyTrack<f32>>,
    /// Font descent in scene units (points), used for baseline alignment.
    pub descent: Option<PropertyTrack<f32>>,
    /// Baseline offset from text center, used for baseline alignment.
    /// A positive value means the baseline is above the text center.
    pub baseline: Option<PropertyTrack<f32>>,

    // ── Layout ──
    /// Size allocated by the layout system.
    pub layout_size: Option<PropertyTrack<[f32; 2]>>,

    // ── Min/Max constraints (Phase 7: percentage & intrinsic sizing) ──
    /// Minimum width constraint.
    pub min_width: Option<PropertyTrack<f32>>,

    /// Minimum height constraint.
    pub min_height: Option<PropertyTrack<f32>>,
    /// Maximum height constraint.
    pub max_height: Option<PropertyTrack<f32>>,
    /// Size specification for percentage/auto/fill sizing (non-animated, set at build time).
    pub size_spec: Option<crate::timeline::taffy_layout::ChildSizeSpec>,

    // ── Procedural plot (re-sampled at frame time) ──
    /// Procedural plot generator, re-sampled each frame.
    pub procedural_plot: Option<ProceduralPlot>,

    // ── Plot parameter keyframe tracks ──
    /// Per-parameter keyframe tracks for procedural plot actors.
    /// Maps parameter name (e.g. "freq") to an f64 property track.
    pub plot_param_tracks: HashMap<String, PropertyTrack<f64>>,

    // ── Highlight tier (sub-struct) ──
    /// Highlight property tracks (color, opacity, padding, radius, blend).
    pub highlight: HighlightTracks,
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
            visible: true,
            locked: false,

            // Geometry flat fields
            position: None,
            motion_offset: None,
            rotation: None,
            scale: None,
            transform: None,
            placement_mode: None,
            position_binding: None,
            size: None,

            // Style flat fields
            color: None,
            opacity: None,
            stroke_width: None,
            stroke_color: None,
            stroke_progress: None,
            fill_opacity: None,
            line_cap: None,
            line_join: None,
            morph_options: None,

            // Filter tier (sub-struct)
            filter: FilterTracks::default(),

            // Shape flat fields
            shape_type: None,
            line_from: None,
            line_to: None,
            head_size: None,
            arc_angles: None,
            points: None,
            commands: None,
            vector_paths: None,

            // Text / media flat fields
            text_content: None,
            font_family: None,
            font_size: None,
            font_weight: None,
            font_style: None,
            line_height: None,
            letter_spacing: None,
            word_spacing: None,
            text_max_width: None,
            text_align: None,
            overflow: None,
            text_paths: None,
            svg_paths: Vec::new(),
            #[cfg(feature = "render")]
            image: None,

            // Font metrics (Phase 6)
            ascent: None,
            descent: None,
            baseline: None,

            // Layout flat fields
            layout_size: None,

            // Min/Max constraints (Phase 7)
            min_width: None,

            min_height: None,
            max_height: None,
            size_spec: None,

            // Procedural plot
            procedural_plot: None,

            // Plot parameter tracks
            plot_param_tracks: HashMap::new(),

            // Highlight tier (sub-struct)
            highlight: HighlightTracks::default(),
        }
    }

    // ── layout_size convenience methods (replacing old LayoutSizeState) ──
    /// Evaluate `layout_size` at `time_ms`.
    pub fn layout_size_get(&self, time_ms: u64) -> Option<[f32; 2]> {
        self.layout_size.as_ref().map(|t| t.evaluate(time_ms))
    }
    /// Return the last value of `layout_size`.
    pub fn layout_size_last(&self) -> Option<[f32; 2]> {
        self.layout_size.as_ref().map(|t| t.last_value())
    }
    /// Ensure `layout_size` exists, creating it with `default` if absent.
    pub fn ensure_layout_size(&mut self, default: [f32; 2]) -> &mut PropertyTrack<[f32; 2]> {
        self.layout_size.get_or_insert_with(|| PropertyTrack::new(default))
    }
    /// Return `true` if `layout_size` has been set.
    pub fn has_layout_size(&self) -> bool {
        self.layout_size.is_some()
    }

    // ── Font metrics accessors (Phase 6) ──
    /// Evaluate `ascent` at `time_ms`.
    pub fn ascent_get(&self, time_ms: u64) -> f32 {
        self.ascent.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0)
    }
    /// Evaluate `descent` at `time_ms`.
    pub fn descent_get(&self, time_ms: u64) -> f32 {
        self.descent.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0)
    }
    /// Evaluate `baseline` at `time_ms`.
    /// This is the offset of the baseline from the text center (0,0) after centering paths.
    pub fn baseline_get(&self, time_ms: u64) -> f32 {
        self.baseline.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(0.0)
    }
    /// Set all three font metrics on the track at the given time.
    pub fn set_metrics(&mut self, time_ms: u64, ascent: f32, descent: f32, baseline: f32) {
        use crate::easing::Easing;
        self.ascent.ensure(0.0).add_keyframe(time_ms, ascent, Easing::Linear);
        self.descent.ensure(0.0).add_keyframe(time_ms, descent, Easing::Linear);
        self.baseline.ensure(0.0).add_keyframe(time_ms, baseline, Easing::Linear);
    }

    // ── Path evaluation ──
    /// Evaluate text paths at `time_ms`, applying morphing if configured.
    pub fn evaluate_text_paths(&self, time_ms: u64) -> Vec<TextPath> {
        if let Some(content_track) = &self.text_content {
            if !content_track.keyframes.is_empty() {
                let current_text = content_track.evaluate(time_ms);
                if current_text.is_empty() { return Vec::new(); }
            }
        }
        let default_paths = PropertyTrack::new(Vec::new());
        let paths_track = self.text_paths.as_ref().unwrap_or(&default_paths);
        let default_morph = PropertyTrack::new(MorphOptions::default());
        let morph_track = self.morph_options.as_ref().unwrap_or(&default_morph);
        evaluate_paths_with_options(paths_track, morph_track, time_ms, interpolate_text_paths)
    }

    /// Evaluate vector paths at `time_ms`, applying morphing if configured.
    pub fn evaluate_vector_paths(&self, time_ms: u64) -> Vec<VelloPath> {
        let default_paths = PropertyTrack::new(Vec::new());
        let paths_track = self.vector_paths.as_ref().unwrap_or(&default_paths);
        let default_morph = PropertyTrack::new(MorphOptions::default());
        let morph_track = self.morph_options.as_ref().unwrap_or(&default_morph);
        evaluate_paths_with_options(paths_track, morph_track, time_ms, interpolate_vello_paths)
    }

    /// Return the maximum keyframe time across all property tracks.
    pub fn max_keyframe_time(&self) -> Option<u64> {
        use crate::timeline::property_registry::PROPERTY_REGISTRY;
        let mut max: Option<u64> = None;
        for schema in PROPERTY_REGISTRY {
            if let Some(t) = crate::timeline::property_engine::property_keyframe_times(self, schema.field).into_iter().max() {
                max = Some(max.map_or(t, |m| m.max(t)));
            }
        }
        // Dynamic plot parameter tracks (not registry-representable)
        for pt in self.plot_param_tracks.values() {
            if let Some(t) = pt.last_keyframe_time() {
                max = Some(max.map_or(t, |m| m.max(t)));
            }
        }
        max
    }

    /// Returns true if any property track has animated keyframes.
    /// A track is "animated" if it has 2+ keyframes or 1 keyframe at time > 0.
    pub fn has_any_keyframes(&self) -> bool {
        use crate::timeline::property_registry::PROPERTY_REGISTRY;
        for schema in PROPERTY_REGISTRY {
            let times = crate::timeline::property_engine::property_keyframe_times(self, schema.field);
            if times.len() > 1 || (times.len() == 1 && times[0] > 0) {
                return true;
            }
        }
        self.plot_param_tracks.values().any(|t| !t.is_effectively_static())
    }
}

fn evaluate_paths_with_options<T: Interpolate>(
    paths: &PropertyTrack<T>,
    morph_options: &PropertyTrack<MorphOptions>,
    time_ms: u64,
    interpolate: fn(&T, &T, f32, MorphOptions) -> T,
) -> T {
    if let Some((found_time, prev_val, found_val, progress, found_easing)) = paths.interpolation_segment(time_ms) {
        let eased_progress = apply_easing(progress, *found_easing);
        let options = morph_options.keyframes.get(&found_time).map(|(value, _)| *value).unwrap_or_default();
        interpolate(prev_val, found_val, eased_progress, options)
    } else {
        // No interior segment — use default or boundary value
        if paths.keyframes.is_empty() {
            paths.default_value.clone()
        } else if let Some((&first_time, (val, _))) = paths.keyframes.iter().next() {
            if time_ms <= first_time {
                val.clone()
            } else {
                paths.last_value()
            }
        } else {
            paths.default_value.clone()
        }
    }
}

fn interpolate_text_paths(source: &Vec<TextPath>, target: &Vec<TextPath>, t: f32, options: MorphOptions) -> Vec<TextPath> {
    if options.strategy == MorphStrategy::Fade {
        if t <= 0.0 {
            return source.clone();
        }
        if t >= 1.0 {
            return target.clone();
        }
        let source_alpha = 1.0 - t;
        let target_alpha = t;
        let mut result = Vec::with_capacity(source.len() + target.len());
        for path in source {
            result.push(TextPath {
                path: path.path.clone(),
                color: path.color.clone(),
                opacity: path.opacity * source_alpha,
            });
        }
        for path in target {
            result.push(TextPath {
                path: path.path.clone(),
                color: path.color.clone(),
                opacity: path.opacity * target_alpha,
            });
        }
        return result;
    }

    let source_paths: Vec<_> = source.iter().map(|path| path.path.clone()).collect();
    let target_paths: Vec<_> = target.iter().map(|path| path.path.clone()).collect();
    let aligned_lists = align_path_lists_with_strategy(&source_paths, &target_paths, options.strategy);
    aligned_lists.into_iter().enumerate().map(|(index, (source_path, target_path))| TextPath {
        path: morph_paths_with_options(&source_path, &target_path, t as f64, options),
        color: if t < 0.5 {
            source.get(index).map(|path| path.color.clone())
                .unwrap_or_else(|| target.get(index).map(|path| path.color.clone())
                .unwrap_or_else(|| typst::visualize::Paint::Solid(typst::visualize::Color::BLACK)))
        } else {
            target.get(index).map(|path| path.color.clone())
                .unwrap_or_else(|| source.get(index).map(|path| path.color.clone())
                .unwrap_or_else(|| typst::visualize::Paint::Solid(typst::visualize::Color::BLACK)))
        },
        opacity: 1.0,
    }).collect()
}

fn interpolate_vello_paths(source: &Vec<VelloPath>, target: &Vec<VelloPath>, t: f32, options: MorphOptions) -> Vec<VelloPath> {
    if options.strategy == MorphStrategy::Fade {
        if t <= 0.0 {
            return source.clone();
        }
        if t >= 1.0 {
            return target.clone();
        }
        let source_alpha = 1.0 - t;
        let target_alpha = t;
        let mut result = Vec::with_capacity(source.len() + target.len());
        for path in source {
            result.push(VelloPath {
                path: path.path.clone(),
                fill: path.fill.map(|c| c.multiply_alpha(source_alpha)),
                stroke: path.stroke.map(|(c, w)| (c.multiply_alpha(source_alpha), w)),
                line_cap: path.line_cap,
                line_join: path.line_join,
            });
        }
        for path in target {
            result.push(VelloPath {
                path: path.path.clone(),
                fill: path.fill.map(|c| c.multiply_alpha(target_alpha)),
                stroke: path.stroke.map(|(c, w)| (c.multiply_alpha(target_alpha), w)),
                line_cap: path.line_cap,
                line_join: path.line_join,
            });
        }
        return result;
    }

    let source_paths: Vec<_> = source.iter().map(|path| path.path.clone()).collect();
    let target_paths: Vec<_> = target.iter().map(|path| path.path.clone()).collect();
    let aligned_lists = align_path_lists_with_strategy(&source_paths, &target_paths, options.strategy);
    aligned_lists.into_iter().enumerate().map(|(index, (source_path, target_path))| {
        let source_element = source.get(index);
        let target_element = target.get(index);
        VelloPath {
            path: morph_paths_with_options(&source_path, &target_path, t as f64, options),
            fill: match (source_element.and_then(|e| e.fill), target_element.and_then(|e| e.fill)) {
                (Some(c1), Some(c2)) => Some(lerp_color(c1, c2, t)),
                (Some(c), None) => Some(if t < 0.5 { c } else { vello::peniko::Color::TRANSPARENT }),
                (None, Some(c)) => Some(if t >= 0.5 { c } else { vello::peniko::Color::TRANSPARENT }),
                (None, None) => None,
            },
            stroke: match (source_element.and_then(|e| e.stroke), target_element.and_then(|e| e.stroke)) {
                (Some((c1, w1)), Some((c2, w2))) => Some((lerp_color(c1, c2, t), w1 + (w2 - w1) * t)),
                (Some((c, w)), None) => Some((if t < 0.5 { c } else { vello::peniko::Color::TRANSPARENT }, if t < 0.5 { w } else { 0.0 })),
                (None, Some((c, w))) => Some((if t >= 0.5 { c } else { vello::peniko::Color::TRANSPARENT }, if t >= 0.5 { w } else { 0.0 })),
                (None, None) => None,
            },
            line_cap: source_element.map(|e| e.line_cap).unwrap_or(0),
            line_join: source_element.map(|e| e.line_join).unwrap_or(0),
        }
    }).collect()
}

// ─────────────────────────────────────────────────────────────
// Field dispatch enums (centralise the ActorField → track field mapping)
// ─────────────────────────────────────────────────────────────

use crate::timeline::property_registry::ActorField;

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
    ShapeType(&'a Option<PropertyTrack<ShapeType>>),
    /// Placement mode property track.
    PlacementMode(&'a Option<PropertyTrack<PlacementMode>>),
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
    ShapeType(&'a mut Option<PropertyTrack<ShapeType>>),
    /// Placement mode property track.
    PlacementMode(&'a mut Option<PropertyTrack<PlacementMode>>),
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
}

// ─────────────────────────────────────────────────────────────
// TrackFieldRef convenience methods
// ─────────────────────────────────────────────────────────────

impl<'a> TrackFieldRef<'a> {
    /// Evaluate the track value at `time_ms`, returning the result as a `PropertyValue`.
    /// Returns `None` for types that cannot be represented as `PropertyValue`.
    pub fn evaluate_value(&self, time_ms: u64) -> Option<crate::timeline::property_engine::PropertyValue> {
        match self {
            Self::F32(opt) => opt.as_ref().map(|pt| crate::timeline::property_engine::PropertyValue::F32(pt.evaluate_copy(time_ms))),
            Self::Vec2(opt) => opt.as_ref().map(|pt| crate::timeline::property_engine::PropertyValue::Vec2(pt.evaluate_copy(time_ms))),
            Self::Vec4(opt) => opt.as_ref().map(|pt| crate::timeline::property_engine::PropertyValue::Color(pt.evaluate_copy(time_ms))),
            Self::Transform(opt) => opt.as_ref().map(|pt| crate::timeline::property_engine::PropertyValue::Transform(pt.evaluate_copy(time_ms))),
            Self::U32(opt) => opt.as_ref().map(|pt| crate::timeline::property_engine::PropertyValue::U32(pt.evaluate_copy(time_ms))),
            Self::ShapeType(opt) => opt.as_ref().map(|pt| crate::timeline::property_engine::PropertyValue::U32(shape_type_to_u32(pt.evaluate_copy(time_ms)))),
            Self::PlacementMode(opt) => opt.as_ref().map(|pt| crate::timeline::property_engine::PropertyValue::PlacementMode(pt.evaluate_copy(time_ms))),
            Self::MorphOptions(opt) => opt.as_ref().map(|pt| crate::timeline::property_engine::PropertyValue::MorphOptions(pt.evaluate_copy(time_ms))),
            Self::String(opt) => opt.as_ref().map(|pt| crate::timeline::property_engine::PropertyValue::String(pt.evaluate(time_ms))),
            Self::PointList(opt) => opt.as_ref().map(|pt| crate::timeline::property_engine::PropertyValue::PointList(pt.evaluate(time_ms))),
            Self::CommandList(opt) => opt.as_ref().map(|pt| crate::timeline::property_engine::PropertyValue::CommandList(pt.evaluate(time_ms))),
            Self::VectorPaths(_) | Self::TextPaths(_)
            | Self::PositionBinding(_) => None,
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
            Self::Transform(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::String(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::U32(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::PointList(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::CommandList(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::ShapeType(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::PlacementMode(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::MorphOptions(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::VectorPaths(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::TextPaths(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            #[cfg(feature = "render")]
            Self::Image(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
            Self::PositionBinding(opt) => opt.as_ref().is_some_and(|pt| pt.keyframes.contains_key(&time_ms)),
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
            Self::MorphOptions(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::VectorPaths(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::TextPaths(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            #[cfg(feature = "render")]
            Self::Image(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
            Self::PositionBinding(opt) => opt.as_ref().map_or(0, |pt| pt.keyframes.len()),
        }
    }

    /// Returns all keyframe timestamps (ms), sorted.
    pub fn keyframe_times(&self) -> Vec<u64> {
        match self {
            Self::F32(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::Vec2(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::Vec4(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::Transform(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::String(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::U32(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::PointList(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::CommandList(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::ShapeType(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::PlacementMode(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::MorphOptions(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::VectorPaths(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::TextPaths(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            #[cfg(feature = "render")]
            Self::Image(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
            Self::PositionBinding(opt) => opt.as_ref().map_or(Vec::new(), |pt| pt.keyframes.keys().copied().collect()),
        }
    }

    /// Returns the easing at a specific keyframe time, if one exists.
    pub fn keyframe_easing(&self, time_ms: u64) -> Option<Easing> {
        match self {
            Self::F32(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::Vec2(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::Vec4(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::Transform(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::String(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::U32(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::PointList(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::CommandList(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::ShapeType(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::PlacementMode(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::MorphOptions(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::VectorPaths(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::TextPaths(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            #[cfg(feature = "render")]
            Self::Image(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
            Self::PositionBinding(opt) => opt.as_ref().and_then(|pt| pt.keyframes.get(&time_ms).map(|(_, e)| *e)),
        }
    }
}

/// Helper: convert a `ShapeType` to its `u32` representation.
fn shape_type_to_u32(st: ShapeType) -> u32 {
    match st {
        ShapeType::Rect => 0,
        ShapeType::Ellipse => 1,
        ShapeType::Line => 2,
        ShapeType::Polygon => 3,
        ShapeType::Path => 4,
        ShapeType::Graph => 5,
        ShapeType::Plot => 6,
        ShapeType::Arrow => 7,
    }
}

impl AnimationTrack {
    /// Get an immutable reference to the track field identified by `field`.
    pub fn field_ref(&self, field: ActorField) -> Option<TrackFieldRef<'_>> {
        use ActorField::*;
        Some(match field {
            Position => TrackFieldRef::Vec2(&self.position),
            MotionOffset => TrackFieldRef::Vec2(&self.motion_offset),
            Size => TrackFieldRef::Vec2(&self.size),
            LayoutSize => TrackFieldRef::Vec2(&self.layout_size),
            Rotation => TrackFieldRef::F32(&self.rotation),
            Scale => TrackFieldRef::F32(&self.scale),
            Transform => TrackFieldRef::Transform(&self.transform),
            Color => TrackFieldRef::Vec4(&self.color),
            Opacity => TrackFieldRef::F32(&self.opacity),
            StrokeWidth => TrackFieldRef::F32(&self.stroke_width),
            StrokeColor => TrackFieldRef::Vec4(&self.stroke_color),
            StrokeProgress => TrackFieldRef::F32(&self.stroke_progress),
            FillOpacity => TrackFieldRef::F32(&self.fill_opacity),
            FilterBlur => TrackFieldRef::F32(&self.filter.filter_blur),
            FilterBrightness => TrackFieldRef::F32(&self.filter.filter_brightness),
            FilterContrast => TrackFieldRef::F32(&self.filter.filter_contrast),
            FilterSaturate => TrackFieldRef::F32(&self.filter.filter_saturate),
            FilterHueRotate => TrackFieldRef::F32(&self.filter.filter_hue_rotate),
            FilterSepia => TrackFieldRef::F32(&self.filter.filter_sepia),
            ShapeType => TrackFieldRef::ShapeType(&self.shape_type),
            LineFrom => TrackFieldRef::Vec2(&self.line_from),
            LineTo => TrackFieldRef::Vec2(&self.line_to),
            HeadSize => TrackFieldRef::F32(&self.head_size),
            LineCap => TrackFieldRef::U32(&self.line_cap),
            LineJoin => TrackFieldRef::U32(&self.line_join),
            ArcAngles => TrackFieldRef::Vec2(&self.arc_angles),
            Points => TrackFieldRef::PointList(&self.points),
            Commands => TrackFieldRef::CommandList(&self.commands),
            TextContent => TrackFieldRef::String(&self.text_content),
            TextMaxWidth => TrackFieldRef::F32(&self.text_max_width),
            TextAlign => TrackFieldRef::String(&self.text_align),
            Overflow => TrackFieldRef::String(&self.overflow),
            FontFamily => TrackFieldRef::String(&self.font_family),
            FontSize => TrackFieldRef::F32(&self.font_size),
            PlacementMode => TrackFieldRef::PlacementMode(&self.placement_mode),
            MorphOptions => TrackFieldRef::MorphOptions(&self.morph_options),
            Ascent => TrackFieldRef::F32(&self.ascent),
            Descent => TrackFieldRef::F32(&self.descent),
            Baseline => TrackFieldRef::F32(&self.baseline),
            HighlightColor => TrackFieldRef::Vec4(&self.highlight.highlight_color),
            HighlightOpacity => TrackFieldRef::F32(&self.highlight.highlight_opacity),
            HighlightPadding => TrackFieldRef::F32(&self.highlight.highlight_padding),
            HighlightRadius => TrackFieldRef::F32(&self.highlight.highlight_radius),
            FontWeight => TrackFieldRef::F32(&self.font_weight),
            FontStyle => TrackFieldRef::String(&self.font_style),
            LineHeight => TrackFieldRef::F32(&self.line_height),
            LetterSpacing => TrackFieldRef::F32(&self.letter_spacing),
            WordSpacing => TrackFieldRef::F32(&self.word_spacing),
            MinWidth => TrackFieldRef::F32(&self.min_width),
            MinHeight => TrackFieldRef::F32(&self.min_height),
            MaxHeight => TrackFieldRef::F32(&self.max_height),
            VectorPaths => TrackFieldRef::VectorPaths(&self.vector_paths),
            TextPaths => TrackFieldRef::TextPaths(&self.text_paths),
            #[cfg(feature = "render")]
            ImageData => TrackFieldRef::Image(&self.image),
            #[cfg(not(feature = "render"))]
            ImageData => return None,
            PositionBinding => TrackFieldRef::PositionBinding(&self.position_binding),
            // Remaining variants without track storage
            SvgPaths | AudioSource | AudioVolume
            | PositionBindingGroup | VectorShapeGroup | PlotDomainGroup
            | ContainerLayoutGroup | NoStorage => return None,
        })
    }

    /// Get a mutable reference to the track field identified by `field`.
    pub fn field_mut(&mut self, field: ActorField) -> Option<TrackFieldMut<'_>> {
        use ActorField::*;
        Some(match field {
            Position => TrackFieldMut::Vec2(&mut self.position),
            MotionOffset => TrackFieldMut::Vec2(&mut self.motion_offset),
            Size => TrackFieldMut::Vec2(&mut self.size),
            LayoutSize => TrackFieldMut::Vec2(&mut self.layout_size),
            Rotation => TrackFieldMut::F32(&mut self.rotation),
            Scale => TrackFieldMut::F32(&mut self.scale),
            Transform => TrackFieldMut::Transform(&mut self.transform),
            Color => TrackFieldMut::Vec4(&mut self.color),
            Opacity => TrackFieldMut::F32(&mut self.opacity),
            StrokeWidth => TrackFieldMut::F32(&mut self.stroke_width),
            StrokeColor => TrackFieldMut::Vec4(&mut self.stroke_color),
            StrokeProgress => TrackFieldMut::F32(&mut self.stroke_progress),
            FillOpacity => TrackFieldMut::F32(&mut self.fill_opacity),
            FilterBlur => TrackFieldMut::F32(&mut self.filter.filter_blur),
            FilterBrightness => TrackFieldMut::F32(&mut self.filter.filter_brightness),
            FilterContrast => TrackFieldMut::F32(&mut self.filter.filter_contrast),
            FilterSaturate => TrackFieldMut::F32(&mut self.filter.filter_saturate),
            FilterHueRotate => TrackFieldMut::F32(&mut self.filter.filter_hue_rotate),
            FilterSepia => TrackFieldMut::F32(&mut self.filter.filter_sepia),
            ShapeType => TrackFieldMut::ShapeType(&mut self.shape_type),
            LineFrom => TrackFieldMut::Vec2(&mut self.line_from),
            LineTo => TrackFieldMut::Vec2(&mut self.line_to),
            HeadSize => TrackFieldMut::F32(&mut self.head_size),
            LineCap => TrackFieldMut::U32(&mut self.line_cap),
            LineJoin => TrackFieldMut::U32(&mut self.line_join),
            ArcAngles => TrackFieldMut::Vec2(&mut self.arc_angles),
            Points => TrackFieldMut::PointList(&mut self.points),
            Commands => TrackFieldMut::CommandList(&mut self.commands),
            TextContent => TrackFieldMut::String(&mut self.text_content),
            TextMaxWidth => TrackFieldMut::F32(&mut self.text_max_width),
            TextAlign => TrackFieldMut::String(&mut self.text_align),
            Overflow => TrackFieldMut::String(&mut self.overflow),
            FontFamily => TrackFieldMut::String(&mut self.font_family),
            FontSize => TrackFieldMut::F32(&mut self.font_size),
            PlacementMode => TrackFieldMut::PlacementMode(&mut self.placement_mode),
            MorphOptions => TrackFieldMut::MorphOptions(&mut self.morph_options),
            Ascent => TrackFieldMut::F32(&mut self.ascent),
            Descent => TrackFieldMut::F32(&mut self.descent),
            Baseline => TrackFieldMut::F32(&mut self.baseline),
            HighlightColor => TrackFieldMut::Vec4(&mut self.highlight.highlight_color),
            HighlightOpacity => TrackFieldMut::F32(&mut self.highlight.highlight_opacity),
            HighlightPadding => TrackFieldMut::F32(&mut self.highlight.highlight_padding),
            HighlightRadius => TrackFieldMut::F32(&mut self.highlight.highlight_radius),
            FontWeight => TrackFieldMut::F32(&mut self.font_weight),
            FontStyle => TrackFieldMut::String(&mut self.font_style),
            LineHeight => TrackFieldMut::F32(&mut self.line_height),
            LetterSpacing => TrackFieldMut::F32(&mut self.letter_spacing),
            WordSpacing => TrackFieldMut::F32(&mut self.word_spacing),
            MinWidth => TrackFieldMut::F32(&mut self.min_width),
            MinHeight => TrackFieldMut::F32(&mut self.min_height),
            MaxHeight => TrackFieldMut::F32(&mut self.max_height),
            VectorPaths => TrackFieldMut::VectorPaths(&mut self.vector_paths),
            TextPaths => TrackFieldMut::TextPaths(&mut self.text_paths),
            #[cfg(feature = "render")]
            ImageData => TrackFieldMut::Image(&mut self.image),
            #[cfg(not(feature = "render"))]
            ImageData => return None,
            PositionBinding => TrackFieldMut::PositionBinding(&mut self.position_binding),
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
            TrackFieldRef::F32(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::Vec2(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::Vec4(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::Transform(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::String(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::U32(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::PointList(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::CommandList(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::ShapeType(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::PlacementMode(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::MorphOptions(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::VectorPaths(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::TextPaths(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            #[cfg(feature = "render")]
            TrackFieldRef::Image(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
            TrackFieldRef::PositionBinding(opt) => opt.as_ref().is_some_and(|t| t.is_currently_animating(time_ms)),
        })
    }

    /// Check if a keyframe exists for the given property at exactly `time_ms`.
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    pub fn has_keyframe_at(&self, property: &str, time_ms: u64) -> bool {
        use crate::timeline::property_registry::ActorField;
        let field = match property {
            "position" => ActorField::Position,
            "motion_offset" => ActorField::MotionOffset,
            "size" => ActorField::Size,
            "layout_size" => ActorField::LayoutSize,
            "rotation" => ActorField::Rotation,
            "scale" => ActorField::Scale,
            "transform" => ActorField::Transform,
            "color" => ActorField::Color,
            "opacity" => ActorField::Opacity,
            "stroke_width" => ActorField::StrokeWidth,
            "stroke_color" => ActorField::StrokeColor,
            "stroke_progress" => ActorField::StrokeProgress,
            "fill_opacity" => ActorField::FillOpacity,
            "filter_blur" => ActorField::FilterBlur,
            "filter_brightness" => ActorField::FilterBrightness,
            "filter_contrast" => ActorField::FilterContrast,
            "filter_saturate" => ActorField::FilterSaturate,
            "filter_hue_rotate" => ActorField::FilterHueRotate,
            "filter_sepia" => ActorField::FilterSepia,
            "shape_type" => ActorField::ShapeType,
            "line_from" => ActorField::LineFrom,
            "line_to" => ActorField::LineTo,
            "arc_angles" => ActorField::ArcAngles,
            "points" => ActorField::Points,
            "commands" => ActorField::Commands,
            "head_size" => ActorField::HeadSize,
            "text_content" => ActorField::TextContent,
            "font_family" => ActorField::FontFamily,
            "font_size" => ActorField::FontSize,
            "font_weight" => ActorField::FontWeight,
            "font_style" => ActorField::FontStyle,
            "line_height" => ActorField::LineHeight,
            "letter_spacing" => ActorField::LetterSpacing,
            "word_spacing" => ActorField::WordSpacing,
            "max_width" => ActorField::TextMaxWidth,
            "text_align" => ActorField::TextAlign,
            "overflow" => ActorField::Overflow,
            "placement_mode" => ActorField::PlacementMode,
            "morph_options" => ActorField::MorphOptions,
            _ => return false,
        };

        self.field_ref(field).is_some_and(|f| f.has_keyframe_at(time_ms))
    }

    /// Check if the property has any keyframes at all (regardless of time).
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    pub fn has_keyframes_for(&self, property: &str) -> bool {
        use crate::timeline::property_registry::ActorField;
        let field = match property {
            "position" => ActorField::Position,
            "motion_offset" => ActorField::MotionOffset,
            "size" => ActorField::Size,
            "layout_size" => ActorField::LayoutSize,
            "rotation" => ActorField::Rotation,
            "scale" => ActorField::Scale,
            "transform" => ActorField::Transform,
            "color" => ActorField::Color,
            "opacity" => ActorField::Opacity,
            "stroke_width" => ActorField::StrokeWidth,
            "stroke_color" => ActorField::StrokeColor,
            "stroke_progress" => ActorField::StrokeProgress,
            "fill_opacity" => ActorField::FillOpacity,
            "filter_blur" => ActorField::FilterBlur,
            "filter_brightness" => ActorField::FilterBrightness,
            "filter_contrast" => ActorField::FilterContrast,
            "filter_saturate" => ActorField::FilterSaturate,
            "filter_hue_rotate" => ActorField::FilterHueRotate,
            "filter_sepia" => ActorField::FilterSepia,
            "shape_type" => ActorField::ShapeType,
            "line_from" => ActorField::LineFrom,
            "line_to" => ActorField::LineTo,
            "arc_angles" => ActorField::ArcAngles,
            "points" => ActorField::Points,
            "commands" => ActorField::Commands,
            "head_size" => ActorField::HeadSize,
            "text_content" => ActorField::TextContent,
            "font_family" => ActorField::FontFamily,
            "font_size" => ActorField::FontSize,
            "font_weight" => ActorField::FontWeight,
            "font_style" => ActorField::FontStyle,
            "line_height" => ActorField::LineHeight,
            "letter_spacing" => ActorField::LetterSpacing,
            "word_spacing" => ActorField::WordSpacing,
            "max_width" => ActorField::TextMaxWidth,
            "text_align" => ActorField::TextAlign,
            "overflow" => ActorField::Overflow,
            "placement_mode" => ActorField::PlacementMode,
            "morph_options" => ActorField::MorphOptions,
            _ => return false,
        };

        self.field_ref(field).is_some_and(|f| f.keyframe_count() > 0)
    }

    /// List all keyframe times (in ms) for the given property.
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    /// Returns a sorted, deduplicated list of timestamps.
    pub fn list_keyframes(&self, property: &str) -> Vec<u64> {
        use crate::timeline::property_registry::ActorField;
        let field = match property {
            "position" => ActorField::Position,
            "motion_offset" => ActorField::MotionOffset,
            "size" => ActorField::Size,
            "layout_size" => ActorField::LayoutSize,
            "rotation" => ActorField::Rotation,
            "scale" => ActorField::Scale,
            "transform" => ActorField::Transform,
            "color" => ActorField::Color,
            "opacity" => ActorField::Opacity,
            "stroke_width" => ActorField::StrokeWidth,
            "stroke_color" => ActorField::StrokeColor,
            "stroke_progress" => ActorField::StrokeProgress,
            "fill_opacity" => ActorField::FillOpacity,
            "filter_blur" => ActorField::FilterBlur,
            "filter_brightness" => ActorField::FilterBrightness,
            "filter_contrast" => ActorField::FilterContrast,
            "filter_saturate" => ActorField::FilterSaturate,
            "filter_hue_rotate" => ActorField::FilterHueRotate,
            "filter_sepia" => ActorField::FilterSepia,
            "shape_type" => ActorField::ShapeType,
            "line_from" => ActorField::LineFrom,
            "line_to" => ActorField::LineTo,
            "arc_angles" => ActorField::ArcAngles,
            "points" => ActorField::Points,
            "commands" => ActorField::Commands,
            "head_size" => ActorField::HeadSize,
            "text_content" => ActorField::TextContent,
            "font_family" => ActorField::FontFamily,
            "font_size" => ActorField::FontSize,
            "font_weight" => ActorField::FontWeight,
            "font_style" => ActorField::FontStyle,
            "line_height" => ActorField::LineHeight,
            "letter_spacing" => ActorField::LetterSpacing,
            "word_spacing" => ActorField::WordSpacing,
            "max_width" => ActorField::TextMaxWidth,
            "text_align" => ActorField::TextAlign,
            "overflow" => ActorField::Overflow,
            "placement_mode" => ActorField::PlacementMode,
            "morph_options" => ActorField::MorphOptions,
            _ => return Vec::new(),
        };

        let mut times: Vec<u64> = self.field_ref(field).map(|f| f.keyframe_times()).unwrap_or_default();
        times.sort_unstable();
        times.dedup();
        times
    }
}

// ─────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::BezPath;

    /// Every ActorKindId variant must have a corresponding ActorKindMeta entry.
    /// This test enumerates all variants and verifies they are in the registry.
    #[test]
    fn actor_kind_registry_is_complete() {
        // Build a set of all registered kinds
        let registered: std::collections::HashSet<ActorKindId> =
            actor_kind_registry().iter().map(|m| m.kind).collect();

        // Enumerate all ShapeKind variants
        let shape_kinds = [
            ShapeKind::Rect,
            ShapeKind::Ellipse,
            ShapeKind::Line,
            ShapeKind::Polygon,
            ShapeKind::Path,
        ];

        for sk in &shape_kinds {
            let kind = ActorKindId::Shape(*sk);
            assert!(
                registered.contains(&kind),
                "ActorKindMeta missing for ShapeKind::{:?}",
                sk
            );
        }

        // Non-shape kinds
        let non_shapes = [
            ActorKindId::Text,
            ActorKindId::Code,
            ActorKindId::Typst,
            ActorKindId::Image,
            ActorKindId::Svg,
            ActorKindId::Graph,
            ActorKindId::PlotCurve,
            ActorKindId::VectorField,
            ActorKindId::Heatmap,
            ActorKindId::ContourSet,
            ActorKindId::NumberPlane,
            ActorKindId::BarChart,
            ActorKindId::Row,
            ActorKindId::Col,
            ActorKindId::Grid,
            ActorKindId::Stack,
            ActorKindId::Group,
        ];

        for kind in &non_shapes {
            assert!(
                registered.contains(kind),
                "ActorKindMeta missing for {:?}",
                kind
            );
        }
    }

    /// Every registry entry must have a non-empty type_name and display_name.
    #[test]
    fn actor_kind_meta_has_valid_fields() {
        for meta in actor_kind_registry().iter() {
            assert!(
                !meta.type_name.is_empty(),
                "ActorKindMeta for {:?} has empty type_name",
                meta.kind
            );
            assert!(
                !meta.display_name.is_empty(),
                "ActorKindMeta for {:?} has empty display_name",
                meta.kind
            );
            assert!(
                !meta.icon_id.is_empty(),
                "ActorKindMeta for {:?} has empty icon_id",
                meta.kind
            );
        }
    }

    /// type_name must round-trip through from_type_name.
    #[test]
    fn actor_kind_type_name_roundtrips() {
        for meta in actor_kind_registry().iter() {
            let parsed = ActorKindId::from_type_name(meta.type_name);
            assert!(
                parsed.is_some(),
                "ActorKindId::from_type_name({:?}) returned None",
                meta.type_name
            );
            assert_eq!(
                parsed.unwrap(), meta.kind,
                "ActorKindId::from_type_name({:?}) returned {:?}, expected {:?}",
                meta.type_name, parsed, meta.kind
            );
        }
    }

    #[test]
    fn fade_vello_paths_at_start_returns_only_source() {
        let source = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(255, 0, 0, 255)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let target = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 255)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let result = interpolate_vello_paths(&source, &target, 0.0, MorphOptions { strategy: MorphStrategy::Fade, path_arc: 0.0, stretch: false });
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].fill.unwrap().to_rgba8().a, 255);
    }

    #[test]
    fn fade_vello_paths_at_end_returns_only_target() {
        let source = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(255, 0, 0, 255)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let target = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 255)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let result = interpolate_vello_paths(&source, &target, 1.0, MorphOptions { strategy: MorphStrategy::Fade, path_arc: 0.0, stretch: false });
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].fill.unwrap().to_rgba8().a, 255);
    }

    #[test]
    fn fade_vello_paths_at_midpoint_returns_both_halved() {
        let source = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(255, 0, 0, 200)),
            stroke: Some((vello::peniko::Color::from_rgba8(255, 255, 255, 100), 2.0)),
            line_cap: 0,
            line_join: 0,
        }];
        let target = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 128)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let result = interpolate_vello_paths(&source, &target, 0.5, MorphOptions { strategy: MorphStrategy::Fade, path_arc: 0.0, stretch: false });
        assert_eq!(result.len(), 2);
        // Source path alpha should be halved: 200 * 0.5 = 100
        assert_eq!(result[0].fill.unwrap().to_rgba8().a, 100);
        // Source stroke alpha should be halved: 100 * 0.5 = 50
        assert_eq!(result[0].stroke.unwrap().0.to_rgba8().a, 50);
        assert_eq!(result[0].stroke.unwrap().1, 2.0);
        // Target path alpha should be halved: 128 * 0.5 = 64
        assert_eq!(result[1].fill.unwrap().to_rgba8().a, 64);
        assert!(result[1].stroke.is_none());
    }

    #[test]
    fn fade_text_paths_at_midpoint_returns_both_halved() {
        let source = vec![TextPath {
            path: BezPath::new(),
            color: typst::visualize::Paint::Solid(typst::visualize::Color::BLACK),
            opacity: 1.0,
        }];
        let target = vec![TextPath {
            path: BezPath::new(),
            color: typst::visualize::Paint::Solid(typst::visualize::Color::WHITE),
            opacity: 0.8,
        }];
        let result = interpolate_text_paths(&source, &target, 0.5, MorphOptions { strategy: MorphStrategy::Fade, path_arc: 0.0, stretch: false });
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].opacity, 0.5);
        assert_eq!(result[1].opacity, 0.4);
    }

    #[test]
    fn fade_vello_paths_empty_source() {
        let source: Vec<VelloPath> = vec![];
        let target = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 128)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let result = interpolate_vello_paths(&source, &target, 0.25, MorphOptions { strategy: MorphStrategy::Fade, path_arc: 0.0, stretch: false });
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].fill.unwrap().to_rgba8().a, 32); // 128 * 0.25
    }

    #[test]
    fn fade_vello_paths_empty_target() {
        let source = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(255, 0, 0, 200)),
            stroke: None,
            line_cap: 0,
            line_join: 0,
        }];
        let target: Vec<VelloPath> = vec![];
        let result = interpolate_vello_paths(&source, &target, 0.75, MorphOptions { strategy: MorphStrategy::Fade, path_arc: 0.0, stretch: false });
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].fill.unwrap().to_rgba8().a, 50); // 200 * 0.25
    }

    #[test]
    fn test_interpolation() {
        let p1: [f32; 2] = [0.0, 0.0];
        let p2: [f32; 2] = [100.0, 50.0];

        let interpolated = p1.interpolate(&p2, 0.5);
        assert_eq!(interpolated, [50.0, 25.0]);
    }

    #[test]
    fn test_property_track_evaluation() {
        let mut track = PropertyTrack::new([0.0, 0.0]);

        track.add_keyframe(0, [0.0, 0.0], Easing::Linear);
        track.add_keyframe(1000, [100.0, 0.0], Easing::Linear);
        track.add_keyframe(2000, [100.0, 100.0], Easing::Linear);

        // Exactly at first keyframe
        assert_eq!(track.evaluate(0), [0.0, 0.0]);

        // Midway between 1st and 2nd
        assert_eq!(track.evaluate(500), [50.0, 0.0]);

        // Exactly at 2nd keyframe
        assert_eq!(track.evaluate(1000), [100.0, 0.0]);

        // Midway between 2nd and 3rd
        assert_eq!(track.evaluate(1500), [100.0, 50.0]);

        // Beyond last keyframe
        assert_eq!(track.evaluate(2500), [100.0, 100.0]);
    }

    // ────────────────────────────────────────────────────────
    // 4.1: field_ref/field_mut coverage tests
    // ────────────────────────────────────────────────────────

    /// Helper: create a track with a keyframe added to a specific field.
    fn create_track_with_f32_keyframe(field: ActorField, time_ms: u64, value: f32) -> AnimationTrack {
        let mut track = AnimationTrack::new("test".to_string());
        if let Some(mut f) = track.field_mut(field) {
            match &mut f {
                TrackFieldMut::F32(opt) => {
                    opt.ensure(0.0).add_keyframe(time_ms, value, Easing::Linear);
                }
                _ => panic!("Expected F32 track"),
            }
        }
        track
    }

    fn create_track_with_vec4_keyframe(field: ActorField, time_ms: u64, value: [f32; 4]) -> AnimationTrack {
        let mut track = AnimationTrack::new("test".to_string());
        if let Some(mut f) = track.field_mut(field) {
            match &mut f {
                TrackFieldMut::Vec4(opt) => {
                    opt.ensure(value).add_keyframe(time_ms, value, Easing::Linear);
                }
                _ => panic!("Expected Vec4 track"),
            }
        }
        track
    }

    fn create_track_with_vec2_keyframe(field: ActorField, time_ms: u64, value: [f32; 2]) -> AnimationTrack {
        let mut track = AnimationTrack::new("test".to_string());
        if let Some(mut f) = track.field_mut(field) {
            match &mut f {
                TrackFieldMut::Vec2(opt) => {
                    opt.ensure(value).add_keyframe(time_ms, value, Easing::Linear);
                }
                _ => panic!("Expected Vec2 track"),
            }
        }
        track
    }

    fn create_track_with_string_keyframe(field: ActorField, time_ms: u64, value: &str) -> AnimationTrack {
        let mut track = AnimationTrack::new("test".to_string());
        if let Some(mut f) = track.field_mut(field) {
            match &mut f {
                TrackFieldMut::String(opt) => {
                    opt.ensure(value.to_string()).add_keyframe(time_ms, value.to_string(), Easing::Linear);
                }
                _ => panic!("Expected String track"),
            }
        }
        track
    }

    #[test]
    fn test_field_ref_ascent_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::Ascent, 0, 10.0);
        let rf = track.field_ref(ActorField::Ascent).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
        assert_eq!(rf.evaluate_value(0), Some(super::super::property_engine::PropertyValue::F32(10.0)));
    }

    #[test]
    fn test_field_ref_descent_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::Descent, 0, 5.0);
        let rf = track.field_ref(ActorField::Descent).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
    }

    #[test]
    fn test_field_ref_baseline_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::Baseline, 0, 2.0);
        let rf = track.field_ref(ActorField::Baseline).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
    }

    #[test]
    fn test_field_ref_highlight_color_returns_vec4() {
        let track = create_track_with_vec4_keyframe(ActorField::HighlightColor, 0, [1.0, 0.0, 0.0, 1.0]);
        let rf = track.field_ref(ActorField::HighlightColor).unwrap();
        assert!(matches!(rf, TrackFieldRef::Vec4(_)));
    }

    #[test]
    fn test_field_ref_highlight_opacity_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::HighlightOpacity, 0, 0.5);
        let rf = track.field_ref(ActorField::HighlightOpacity).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
    }

    #[test]
    fn test_field_ref_highlight_padding_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::HighlightPadding, 0, 8.0);
        let rf = track.field_ref(ActorField::HighlightPadding).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
    }

    #[test]
    fn test_field_ref_highlight_radius_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::HighlightRadius, 0, 6.0);
        let rf = track.field_ref(ActorField::HighlightRadius).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
    }

    #[test]
    fn test_field_ref_font_weight_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::FontWeight, 0, 700.0);
        let rf = track.field_ref(ActorField::FontWeight).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
    }

    #[test]
    fn test_field_ref_font_style_returns_string() {
        let track = create_track_with_string_keyframe(ActorField::FontStyle, 0, "italic");
        let rf = track.field_ref(ActorField::FontStyle).unwrap();
        assert!(matches!(rf, TrackFieldRef::String(_)));
    }

    #[test]
    fn test_field_ref_line_height_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::LineHeight, 0, 1.5);
        let rf = track.field_ref(ActorField::LineHeight).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
    }

    #[test]
    fn test_field_ref_letter_spacing_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::LetterSpacing, 0, 0.5);
        let rf = track.field_ref(ActorField::LetterSpacing).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
    }

    #[test]
    fn test_field_ref_word_spacing_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::WordSpacing, 0, 2.0);
        let rf = track.field_ref(ActorField::WordSpacing).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
    }

    #[test]
    fn test_field_ref_min_width_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::MinWidth, 0, 100.0);
        let rf = track.field_ref(ActorField::MinWidth).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
    }

    #[test]
    fn test_field_ref_min_height_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::MinHeight, 0, 200.0);
        let rf = track.field_ref(ActorField::MinHeight).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
    }

    #[test]
    fn test_field_ref_max_height_returns_f32() {
        let track = create_track_with_f32_keyframe(ActorField::MaxHeight, 0, 500.0);
        let rf = track.field_ref(ActorField::MaxHeight).unwrap();
        assert!(matches!(rf, TrackFieldRef::F32(_)));
    }

    #[test]
    fn test_field_ref_vector_paths_returns_vector_paths() {
        let track = AnimationTrack::new("test".to_string());
        let rf = track.field_ref(ActorField::VectorPaths);
        assert!(rf.is_some());
        assert!(matches!(rf.unwrap(), TrackFieldRef::VectorPaths(_)));
    }

    #[test]
    fn test_field_ref_text_paths_returns_text_paths() {
        let track = AnimationTrack::new("test".to_string());
        let rf = track.field_ref(ActorField::TextPaths);
        assert!(rf.is_some());
        assert!(matches!(rf.unwrap(), TrackFieldRef::TextPaths(_)));
    }

    #[test]
    fn test_field_ref_position_binding_returns_position_binding() {
        let track = AnimationTrack::new("test".to_string());
        let rf = track.field_ref(ActorField::PositionBinding);
        assert!(rf.is_some());
        assert!(matches!(rf.unwrap(), TrackFieldRef::PositionBinding(_)));
    }

    #[test]
    fn test_field_ref_svg_paths_returns_none() {
        let track = AnimationTrack::new("test".to_string());
        let rf = track.field_ref(ActorField::SvgPaths);
        assert!(rf.is_none());
    }

    #[test]
    fn test_field_ref_returns_correct_type_for_position() {
        let track = AnimationTrack::new("test".to_string());
        let rf = track.field_ref(ActorField::Position).unwrap();
        assert!(matches!(rf, TrackFieldRef::Vec2(_)));
    }

    #[test]
    fn test_field_ref_returns_correct_type_for_transform() {
        let track = AnimationTrack::new("test".to_string());
        let rf = track.field_ref(ActorField::Transform).unwrap();
        assert!(matches!(rf, TrackFieldRef::Transform(_)));
    }

    #[test]
    fn test_field_mut_returns_correct_type_for_highlight_color() {
        let mut track = AnimationTrack::new("test".to_string());
        let f = track.field_mut(ActorField::HighlightColor).unwrap();
        assert!(matches!(f, TrackFieldMut::Vec4(_)));
    }

    #[test]
    fn test_field_mut_returns_correct_type_for_font_weight() {
        let mut track = AnimationTrack::new("test".to_string());
        let f = track.field_mut(ActorField::FontWeight).unwrap();
        assert!(matches!(f, TrackFieldMut::F32(_)));
    }

    #[test]
    fn test_field_mut_returns_correct_type_for_font_style() {
        let mut track = AnimationTrack::new("test".to_string());
        let f = track.field_mut(ActorField::FontStyle).unwrap();
        assert!(matches!(f, TrackFieldMut::String(_)));
    }

    // ────────────────────────────────────────────────────────
    // 4.3: max_keyframe_time / has_any_keyframes regression tests
    // ────────────────────────────────────────────────────────

    #[test]
    fn test_max_keyframe_time_with_transform() {
        let mut track = AnimationTrack::new("test".to_string());
        track.transform.ensure([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
            .add_keyframe(5000, [2.0, 0.0, 0.0, 2.0, 100.0, 200.0], Easing::Linear);
        assert_eq!(track.max_keyframe_time(), Some(5000));
    }

    #[test]
    fn test_max_keyframe_time_with_highlight_color() {
        let mut track = AnimationTrack::new("test".to_string());
        track.highlight.highlight_color.ensure([0.3, 0.5, 1.0, 1.0])
            .add_keyframe(3000, [1.0, 0.0, 0.0, 0.5], Easing::Linear);
        assert_eq!(track.max_keyframe_time(), Some(3000));
    }

    #[test]
    fn test_max_keyframe_time_with_filter_blur() {
        let mut track = AnimationTrack::new("test".to_string());
        track.filter.filter_blur.ensure(0.0)
            .add_keyframe(2000, 5.0, Easing::Linear);
        assert_eq!(track.max_keyframe_time(), Some(2000));
    }

    #[test]
    fn test_max_keyframe_time_with_filter_brightness() {
        let mut track = AnimationTrack::new("test".to_string());
        track.filter.filter_brightness.ensure(1.0)
            .add_keyframe(1500, 2.0, Easing::Linear);
        assert_eq!(track.max_keyframe_time(), Some(1500));
    }

    #[test]
    fn test_max_keyframe_time_returns_max_across_all_fields() {
        let mut track = AnimationTrack::new("test".to_string());
        track.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);
        track.position.ensure([0.0, 0.0]).add_keyframe(5000, [100.0, 100.0], Easing::Linear);
        track.highlight.highlight_opacity.ensure(0.0).add_keyframe(3000, 0.8, Easing::Linear);
        assert_eq!(track.max_keyframe_time(), Some(5000));
    }

    #[test]
    fn test_max_keyframe_time_returns_none_for_empty_track() {
        let track = AnimationTrack::new("test".to_string());
        assert_eq!(track.max_keyframe_time(), None);
    }

    #[test]
    fn test_has_any_keyframes_returns_true_for_highlight_fields() {
        let mut track = AnimationTrack::new("test".to_string());
        track.highlight.highlight_color.ensure([0.3, 0.5, 1.0, 1.0])
            .add_keyframe(1000, [1.0, 0.0, 0.0, 0.5], Easing::Linear);
        assert!(track.has_any_keyframes());
    }

    #[test]
    fn test_has_any_keyframes_returns_true_for_font_metrics() {
        let mut track = AnimationTrack::new("test".to_string());
        track.ascent.ensure(0.0).add_keyframe(500, 10.0, Easing::Linear);
        assert!(track.has_any_keyframes());
    }

    #[test]
    fn test_has_any_keyframes_returns_false_for_empty_track() {
        let track = AnimationTrack::new("test".to_string());
        assert!(!track.has_any_keyframes());
    }

    #[test]
    fn test_has_any_keyframes_returns_false_for_single_keyframe_at_time_zero() {
        let mut track = AnimationTrack::new("test".to_string());
        track.opacity.ensure(1.0).add_keyframe(0, 0.5, Easing::Linear);
        assert!(!track.has_any_keyframes());
    }

    #[test]
    fn test_has_any_keyframes_returns_true_for_multiple_keyframes() {
        let mut track = AnimationTrack::new("test".to_string());
        track.opacity.ensure(1.0).add_keyframe(0, 0.5, Easing::Linear);
        track.opacity.ensure(0.5).add_keyframe(1000, 1.0, Easing::Linear);
        assert!(track.has_any_keyframes());
    }

    #[test]
    fn test_has_any_keyframes_returns_true_for_filter_contrast() {
        let mut track = AnimationTrack::new("test".to_string());
        track.filter.filter_contrast.ensure(1.0).add_keyframe(2000, 2.0, Easing::Linear);
        assert!(track.has_any_keyframes());
    }

    // ────────────────────────────────────────────────────────
    // 4.4: is_currently_animating tests
    // ────────────────────────────────────────────────────────

    #[test]
    fn test_is_currently_animating_false_with_single_keyframe() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(1000, 1.0, Easing::Linear);
        // Single keyframe in the future is not animating at any time
        assert!(!track.is_currently_animating(0));
        assert!(!track.is_currently_animating(500));
        assert!(!track.is_currently_animating(1000));
        assert!(!track.is_currently_animating(2000));
    }

    #[test]
    fn test_is_currently_animating_true_between_two_keyframes() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(0, 0.0, Easing::Linear);
        track.add_keyframe(1000, 1.0, Easing::Linear);
        assert!(track.is_currently_animating(500));
    }

    #[test]
    fn test_is_currently_animating_false_before_first_keyframe() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(500, 0.5, Easing::Linear);
        track.add_keyframe(1000, 1.0, Easing::Linear);
        assert!(!track.is_currently_animating(0));
        assert!(!track.is_currently_animating(400));
    }

    #[test]
    fn test_is_currently_animating_false_after_last_keyframe() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(0, 0.0, Easing::Linear);
        track.add_keyframe(500, 0.5, Easing::Linear);
        assert!(!track.is_currently_animating(600));
        assert!(!track.is_currently_animating(1000));
    }

    #[test]
    fn test_is_currently_animating_at_exact_keyframe_not_animating() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(0, 0.0, Easing::Linear);
        track.add_keyframe(1000, 1.0, Easing::Linear);
        // At exact keyframe time, the next keyframe is at > time_ms
        assert!(!track.is_currently_animating(1000));
    }

    #[test]
    fn test_is_currently_animating_works_with_linear_easing() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(0, 0.0, Easing::Linear);
        track.add_keyframe(2000, 1.0, Easing::Linear);
        assert!(track.is_currently_animating(1000));
    }

    // ────────────────────────────────────────────────────────
    // 4.5: interpolation_segment tests
    // ────────────────────────────────────────────────────────

    #[test]
    fn test_interpolation_segment_none_for_empty_track() {
        let track: PropertyTrack<f32> = PropertyTrack::new(0.0);
        assert!(track.interpolation_segment(500).is_none());
    }

    #[test]
    fn test_interpolation_segment_none_before_first_keyframe() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(1000, 1.0, Easing::Linear);
        track.add_keyframe(2000, 2.0, Easing::Linear);
        assert!(track.interpolation_segment(500).is_none());
    }

    #[test]
    fn test_interpolation_segment_none_after_last_keyframe() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(0, 0.0, Easing::Linear);
        track.add_keyframe(1000, 1.0, Easing::Linear);
        assert!(track.interpolation_segment(1500).is_none());
    }

    #[test]
    fn test_interpolation_segment_returns_correct_segment() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(0, 0.0, Easing::Linear);
        track.add_keyframe(1000, 100.0, Easing::Linear);
        let result = track.interpolation_segment(500);
        assert!(result.is_some());
        let (found_time, prev_val, found_val, progress, easing) = result.unwrap();
        assert_eq!(found_time, 1000);
        assert_eq!(*prev_val, 0.0);
        assert_eq!(*found_val, 100.0);
        assert!((progress - 0.5).abs() < 0.001);
        assert_eq!(*easing, Easing::Linear);
    }

    #[test]
    fn test_interpolation_segment_progress_at_start() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(0, 0.0, Easing::Linear);
        track.add_keyframe(2000, 200.0, Easing::EaseIn);
        let result = track.interpolation_segment(500);
        assert!(result.is_some());
        let (_, _, _, progress, easing) = result.unwrap();
        assert!((progress - 0.25).abs() < 0.001);
        assert_eq!(*easing, Easing::EaseIn);
    }

    #[test]
    fn test_interpolation_segment_with_single_keyframe_returns_none() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(500, 1.0, Easing::Linear);
        assert!(track.interpolation_segment(0).is_none());
        assert!(track.interpolation_segment(500).is_none());
        assert!(track.interpolation_segment(1000).is_none());
    }

    // ────────────────────────────────────────────────────────
    // 4.6: evaluate_paths_with_options tests
    // ────────────────────────────────────────────────────────

    fn identity_interpolate<T: Interpolate>(source: &T, target: &T, t: f32, _options: MorphOptions) -> T {
        source.interpolate(target, t)
    }

    #[test]
    fn test_evaluate_paths_with_options_empty_track() {
        let paths: PropertyTrack<f32> = PropertyTrack::new(42.0);
        let morph: PropertyTrack<MorphOptions> = PropertyTrack::new(MorphOptions::default());
        let result = evaluate_paths_with_options(&paths, &morph, 500, identity_interpolate);
        assert_eq!(result, 42.0);
    }

    #[test]
    fn test_evaluate_paths_with_options_before_first_keyframe() {
        let mut paths = PropertyTrack::new(0.0);
        paths.add_keyframe(1000, 100.0, Easing::Linear);
        let morph = PropertyTrack::new(MorphOptions::default());
        let result = evaluate_paths_with_options(&paths, &morph, 500, identity_interpolate);
        // Before first keyframe, evaluate_paths_with_options returns the first keyframe's value
        assert_eq!(result, 100.0);
    }

    #[test]
    fn test_evaluate_paths_with_options_after_last_keyframe() {
        let mut paths = PropertyTrack::new(0.0);
        paths.add_keyframe(0, 10.0, Easing::Linear);
        paths.add_keyframe(1000, 100.0, Easing::Linear);
        let morph = PropertyTrack::new(MorphOptions::default());
        let result = evaluate_paths_with_options(&paths, &morph, 2000, identity_interpolate);
        assert_eq!(result, 100.0);
    }

    #[test]
    fn test_evaluate_paths_with_options_between_two_keyframes() {
        let mut paths = PropertyTrack::new(0.0);
        paths.add_keyframe(0, 0.0, Easing::Linear);
        paths.add_keyframe(1000, 100.0, Easing::Linear);
        let morph = PropertyTrack::new(MorphOptions::default());
        let result = evaluate_paths_with_options(&paths, &morph, 500, identity_interpolate);
        assert_eq!(result, 50.0);
    }

    #[test]
    fn test_evaluate_paths_with_options_uses_morph_at_second_keyframe() {
        let mut paths = PropertyTrack::new(0.0);
        paths.add_keyframe(0, 0.0, Easing::Linear);
        paths.add_keyframe(1000, 100.0, Easing::Linear);
        let mut morph = PropertyTrack::new(MorphOptions::default());
        // Morph at the same time as the second keyframe
        morph.add_keyframe(1000, MorphOptions { strategy: MorphStrategy::Fade, path_arc: 0.0, stretch: false }, Easing::Linear);
        // Should use Fade morph strategy (which just returns source+target)
        let result = evaluate_paths_with_options(&paths, &morph, 500, identity_interpolate);
        // identity_interpolate just does normal interpolation regardless of morph
        // So result should be 50.0
        assert_eq!(result, 50.0);
    }

    // ────────────────────────────────────────────────────────
    // 4.8: Registry-driven iteration tests
    // ────────────────────────────────────────────────────────

    #[test]
    fn test_max_keyframe_time_iterates_through_all_registry_fields() {
        let mut track = AnimationTrack::new("test".to_string());
        // Expect max_keyframe_time to iterate registry-driven fields
        // Put a keyframe on an unconventional registry field
        track.min_width.ensure(0.0).add_keyframe(7777, 50.0, Easing::Linear);
        assert_eq!(track.max_keyframe_time(), Some(7777));
    }

    #[test]
    fn test_has_any_keyframes_iterates_through_all_registry_fields() {
        let mut track = AnimationTrack::new("test".to_string());
        // Put a keyframe on a field that might have been missed
        track.highlight.highlight_padding.ensure(4.0).add_keyframe(500, 8.0, Easing::Linear);
        assert!(track.has_any_keyframes());
    }

    #[test]
    fn test_registry_driven_max_keyframe_time_finds_min_width() {
        let mut track = AnimationTrack::new("test".to_string());
        track.min_height.ensure(0.0).add_keyframe(4000, 200.0, Easing::Linear);
        assert_eq!(track.max_keyframe_time(), Some(4000));
    }

    #[test]
    fn test_registry_driven_max_keyframe_time_finds_letter_spacing() {
        let mut track = AnimationTrack::new("test".to_string());
        track.letter_spacing.ensure(0.0).add_keyframe(2500, 1.0, Easing::Linear);
        assert_eq!(track.max_keyframe_time(), Some(2500));
    }

    #[test]
    fn test_registry_driven_has_any_keyframes_finds_word_spacing() {
        let mut track = AnimationTrack::new("test".to_string());
        track.word_spacing.ensure(0.0).add_keyframe(1000, 5.0, Easing::Linear);
        assert!(track.has_any_keyframes());
    }

    // ────────────────────────────────────────────────────────
    // 4.9: TrackFieldRef helper methods
    // ────────────────────────────────────────────────────────

    #[test]
    fn test_track_field_ref_evaluate_value_f32() {
        let track = create_track_with_f32_keyframe(ActorField::Opacity, 0, 0.5);
        let rf = track.field_ref(ActorField::Opacity).unwrap();
        let val = rf.evaluate_value(0);
        assert_eq!(val, Some(super::super::property_engine::PropertyValue::F32(0.5)));
    }

    #[test]
    fn test_track_field_ref_evaluate_value_vec2() {
        let track = create_track_with_vec2_keyframe(ActorField::Position, 0, [100.0, 200.0]);
        let rf = track.field_ref(ActorField::Position).unwrap();
        let val = rf.evaluate_value(0);
        assert_eq!(val, Some(super::super::property_engine::PropertyValue::Vec2([100.0, 200.0])));
    }

    #[test]
    fn test_track_field_ref_evaluate_value_color() {
        let track = create_track_with_vec4_keyframe(ActorField::Color, 0, [1.0, 0.0, 0.0, 1.0]);
        let rf = track.field_ref(ActorField::Color).unwrap();
        let val = rf.evaluate_value(0);
        assert_eq!(val, Some(super::super::property_engine::PropertyValue::Color([1.0, 0.0, 0.0, 1.0])));
    }

    #[test]
    fn test_track_field_ref_has_keyframe_at() {
        let mut track = AnimationTrack::new("test".to_string());
        track.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
        let rf = track.field_ref(ActorField::Opacity).unwrap();
        assert!(rf.has_keyframe_at(500));
        assert!(!rf.has_keyframe_at(0));
        assert!(!rf.has_keyframe_at(1000));
    }

    #[test]
    fn test_track_field_ref_keyframe_count() {
        let mut track = AnimationTrack::new("test".to_string());
        track.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        track.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
        track.opacity.ensure(0.5).add_keyframe(1000, 0.0, Easing::Linear);
        let rf = track.field_ref(ActorField::Opacity).unwrap();
        assert_eq!(rf.keyframe_count(), 3);
    }

    #[test]
    fn test_track_field_ref_keyframe_times() {
        let mut track = AnimationTrack::new("test".to_string());
        track.opacity.ensure(1.0).add_keyframe(1000, 0.0, Easing::Linear);
        track.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        let rf = track.field_ref(ActorField::Opacity).unwrap();
        let mut times = rf.keyframe_times();
        times.sort();
        assert_eq!(times, vec![0, 1000]);
    }

    #[test]
    fn test_track_field_ref_keyframe_easing() {
        let mut track = AnimationTrack::new("test".to_string());
        track.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::EaseOut);
        track.opacity.ensure(0.0).add_keyframe(500, 0.0, Easing::Linear);
        let rf = track.field_ref(ActorField::Opacity).unwrap();
        assert_eq!(rf.keyframe_easing(0), Some(Easing::EaseOut));
        assert_eq!(rf.keyframe_easing(500), Some(Easing::Linear));
        assert_eq!(rf.keyframe_easing(999), None);
    }

    #[test]
    fn test_track_field_ref_keyframe_count_none_for_empty() {
        let track = AnimationTrack::new("test".to_string());
        let rf = track.field_ref(ActorField::Opacity).unwrap();
        assert_eq!(rf.keyframe_count(), 0);
    }

    #[test]
    fn test_track_field_ref_evaluate_value_none_for_vector_paths() {
        let track = AnimationTrack::new("test".to_string());
        let rf = track.field_ref(ActorField::VectorPaths).unwrap();
        assert!(rf.evaluate_value(0).is_none());
    }

    #[test]
    fn test_track_field_ref_evaluate_value_none_for_position_binding() {
        let track = AnimationTrack::new("test".to_string());
        let rf = track.field_ref(ActorField::PositionBinding).unwrap();
        assert!(rf.evaluate_value(0).is_none());
    }

    // ────────────────────────────────────────────────────────
    // 4.10: Cache invalidation tests
    // ────────────────────────────────────────────────────────

    #[test]
    fn test_set_default_value_invalidates_cache() {
        let mut track = PropertyTrack::new(10.0);
        track.add_keyframe(0, 10.0, Easing::Linear);
        track.add_keyframe(1000, 20.0, Easing::Linear);
        // Evaluate to populate cache
        assert_eq!(track.evaluate(500), 15.0);
        // Change default value (should invalidate cache)
        track.set_default_value(0.0);
        // Evaluate at different time to verify cache was invalidated
        // (the new default won't affect interpolation between keyframes, but cache was reset)
        assert_eq!(track.evaluate(500), 15.0); // Interpolation still works

        // Clear keyframes via keyframes_mut (invalidates cache) and verify new default is used
        track.keyframes_mut().clear();
        assert_eq!(track.evaluate(500), 0.0);
    }

    #[test]
    fn test_keyframes_mut_invalidates_cache() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(0, 0.0, Easing::Linear);
        track.add_keyframe(1000, 100.0, Easing::Linear);
        // Evaluate to populate cache
        assert_eq!(track.evaluate(500), 50.0);
        // Mutate keyframes (should invalidate)
        track.keyframes_mut().insert(2000, (200.0, Easing::Linear));
        // Evaluate at different time to verify
        assert_eq!(track.evaluate(1500), 150.0);
    }

    #[test]
    fn test_clone_resets_cache_to_none() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(0, 0.0, Easing::Linear);
        track.add_keyframe(1000, 100.0, Easing::Linear);
        // Evaluate to populate cache
        let _ = track.evaluate(500);
        // Clone should reset cache
        let cloned = track.clone();
        // Cloned track should evaluate correctly (cache starting fresh)
        assert_eq!(cloned.evaluate(500), 50.0);
    }

    // ────────────────────────────────────────────────────────
    // 4.11: Accessor method tests
    // ────────────────────────────────────────────────────────

    #[test]
    fn test_default_value_returns_correct_reference() {
        let track = PropertyTrack::new(42.0);
        assert_eq!(*track.default_value(), 42.0);
    }

    #[test]
    fn test_keyframes_returns_correct_reference() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(0, 10.0, Easing::Linear);
        track.add_keyframe(1000, 20.0, Easing::EaseOut);
        let kfs = track.keyframes();
        assert_eq!(kfs.len(), 2);
        assert_eq!(kfs.get(&0), Some(&(10.0, Easing::Linear)));
        assert_eq!(kfs.get(&1000), Some(&(20.0, Easing::EaseOut)));
    }

    #[test]
    fn test_set_default_value_updates_value() {
        let mut track = PropertyTrack::new(10.0);
        assert_eq!(*track.default_value(), 10.0);
        track.set_default_value(99.0);
        assert_eq!(*track.default_value(), 99.0);
    }

    #[test]
    fn test_keyframes_mut_allows_mutation() {
        let mut track = PropertyTrack::new(0.0);
        track.add_keyframe(0, 10.0, Easing::Linear);
        track.add_keyframe(1000, 20.0, Easing::Linear);
        {
            let kfs = track.keyframes_mut();
            // Insert a new keyframe
            kfs.insert(500, (15.0, Easing::EaseIn));
        }
        assert_eq!(track.keyframes().len(), 3);
        assert_eq!(track.keyframes().get(&500), Some(&(15.0, Easing::EaseIn)));
    }

    #[test]
    fn test_missing_properties() {
        let track = AnimationTrack::new("empty_actor".to_string());

        assert_eq!(track.position.get(0, [0.0, 0.0]), [0.0, 0.0]);
        assert_eq!(
            track.placement_mode.get(0, PlacementMode::LayoutManaged),
            PlacementMode::LayoutManaged
        );
        assert_eq!(
            track.position_binding.get(0, PositionBinding::Absolute),
            PositionBinding::Absolute
        );
        assert_eq!(track.size.get(0, [50.0, 50.0]), [50.0, 50.0]);
        assert_eq!(track.line_from.get(0, [-50.0, 0.0]), [-50.0, 0.0]);
        assert_eq!(track.line_to.get(0, [50.0, 0.0]), [50.0, 0.0]);
        assert_eq!(track.arc_angles.get(0, [0.0, std::f32::consts::PI]), [0.0, std::f32::consts::PI]);
        assert_eq!(track.color.get(0, [1.0, 1.0, 1.0, 1.0]), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(track.shape_type.get(0, ShapeType::Rect), ShapeType::Rect);
        assert_eq!(track.opacity.get(0, 1.0), 1.0);
    }
}
