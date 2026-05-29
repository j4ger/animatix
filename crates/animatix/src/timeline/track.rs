use crate::easing::{Easing, apply_easing};
use crate::renderer::types::{TextPath, VelloPath};
use crate::timeline::morph::{
    MorphOptions, MorphStrategy, align_path_lists_with_strategy, morph_paths_with_options,
};
use crate::timeline::plot::ProceduralPlot;
use crate::timeline::shapes::ShapeType;
use std::collections::BTreeMap;

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
    /// Math / LaTeX actor.
    Math,
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
    /// Audio track actor.
    Audio,
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
pub fn actor_kind_meta(kind: ActorKindId) -> &'static ActorKindMeta {
    crate::primitives::actor_kind_meta(kind)
}

/// Lookup metadata by the actor's type name (e.g. `"rect"`, `"text"`).
pub fn actor_kind_meta_by_name(name: &str) -> Option<&'static ActorKindMeta> {
    crate::primitives::actor_kind_meta_by_name(name)
}

/// Extension trait for lazy property track access.
pub trait TrackAccessor<T: Interpolate + Clone> {
    /// Evaluate the track at `time_ms`, falling back to `default` if empty.
    fn get(&self, time_ms: u64, default: T) -> T;
    /// Ensure the track exists, creating it with `default` if absent.
    fn ensure(&mut self, default: T) -> &mut PropertyTrack<T>;
    /// Return the value of the last keyframe, or `default` if empty.
    fn last(&self, default: T) -> T;
    /// Return the timestamp of the last keyframe, if any.
    fn last_time(&self) -> Option<u64>;
    /// Check whether a keyframe exists at exactly `time_ms`.
    fn has_keyframe_at(&self, time_ms: u64) -> bool;
}

impl<T: Interpolate + Clone> TrackAccessor<T> for Option<PropertyTrack<T>> {
    fn get(&self, time_ms: u64, default: T) -> T {
        self.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(default)
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
pub trait Interpolate {
    /// Interpolate between `self` and `other` using parameter `t` in `[0, 1]`.
    fn interpolate(&self, other: &Self, t: f32) -> Self;
}

impl Interpolate for f32 {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        self + (other - self) * t.clamp(0.0, 1.0)
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
#[derive(Clone)]
pub struct PropertyTrack<T> {
    /// Map from timestamp (ms) to `(value, easing)` pairs.
    pub keyframes: BTreeMap<u64, (T, Easing)>,
    /// Value used when no keyframes are defined.
    pub default_value: T,
    /// P2.20: Memoization cache for repeated time queries.
    last_evaluated: std::cell::RefCell<Option<(u64, T)>>,
}

impl<T: Interpolate + Clone> PropertyTrack<T> {
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
        // P2.20: Memoization — return cached value if time matches
        if let Some((cached_time, cached_value)) = self.last_evaluated.borrow().as_ref() {
            if *cached_time == time_ms {
                return cached_value.clone();
            }
        }

        let result = if self.keyframes.is_empty() {
            self.default_value.clone()
        } else {
            let found = match self.keyframes.range(time_ms..).next() {
                Some(entry) => entry,
                None => return self.last_value(),
            };
            let (&found_time, (found_val, found_easing)) = found;
            if let Some((&first_time, _)) = self.keyframes.iter().next() {
                if time_ms <= first_time {
                    let value = found_val.clone();
                    *self.last_evaluated.borrow_mut() = Some((time_ms, value.clone()));
                    return value;
                }
            }
            let (prev_time, prev_val) = match self.keyframes.range(..time_ms).next_back() {
                Some((&t, (val, _))) => (t, val.clone()),
                None => (0, self.default_value.clone()),
            };
            let duration = (found_time - prev_time) as f32;
            let elapsed = (time_ms - prev_time) as f32;
            let progress = elapsed / duration;
            let eased_progress = apply_easing(progress, *found_easing);
            prev_val.interpolate(found_val, eased_progress)
        };

        *self.last_evaluated.borrow_mut() = Some((time_ms, result.clone()));
        result
    }
    /// Return the value of the most recent keyframe, or the default.
    pub fn last_value(&self) -> T {
        self.keyframes.iter().next_back().map(|(_, (val, _))| val.clone())
            .unwrap_or_else(|| self.default_value.clone())
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
}

// ─────────────────────────────────────────────────────────────
// AnimationTrack
// ─────────────────────────────────────────────────────────────

/// Per-actor animation track holding all animatable properties.
#[derive(Clone)]
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
    /// Path morphing options.
    pub morph_options: Option<PropertyTrack<MorphOptions>>,

    // ── Effects tier ──
    /// Shadow offset vector.
    pub shadow_offset: Option<PropertyTrack<[f32; 2]>>,
    /// Shadow blur radius.
    pub shadow_blur: Option<PropertyTrack<f32>>,
    /// Shadow color in RGBA.
    pub shadow_color: Option<PropertyTrack<[f32; 4]>>,
    /// Glow / blur radius.
    pub glow_radius: Option<PropertyTrack<f32>>,
    /// Glow color in RGBA.
    pub glow_color: Option<PropertyTrack<[f32; 4]>>,
    /// Backdrop blur strength.
    pub backdrop_blur: Option<PropertyTrack<f32>>,

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
    /// Pre-built text paths for rendering.
    pub text_paths: Option<PropertyTrack<Vec<TextPath>>>,
    /// Static SVG paths.
    pub svg_paths: Vec<crate::timeline::VelloPath>,
    /// Raster image data.
    pub image: Option<PropertyTrack<Option<crate::timeline::image::SceneImage>>>,

    // ── Layout ──
    /// Size allocated by the layout system.
    pub layout_size: Option<PropertyTrack<[f32; 2]>>,

    // ── Procedural plot (re-sampled at frame time) ──
    /// Procedural plot generator, re-sampled each frame.
    pub procedural_plot: Option<ProceduralPlot>,
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
            morph_options: None,

            // Effects flat fields
            shadow_offset: None,
            shadow_blur: None,
            shadow_color: None,
            glow_radius: None,
            glow_color: None,
            backdrop_blur: None,

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
            text_paths: None,
            svg_paths: Vec::new(),
            image: None,

            // Layout flat fields
            layout_size: None,

            // Procedural plot
            procedural_plot: None,
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

    // ── Path evaluation ──
    /// Evaluate text paths at `time_ms`, applying morphing if configured.
    pub fn evaluate_text_paths(&self, time_ms: u64) -> Vec<TextPath> {
        if let Some(content_track) = &self.text_content {
            if !content_track.keyframes.is_empty() {
                let current_text = content_track.evaluate(time_ms);
                if !current_text.is_empty() { return Vec::new(); }
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
        let times: Vec<Option<u64>> = vec![
            self.position.last_time(), self.motion_offset.last_time(),
            self.rotation.last_time(), self.scale.last_time(),
            self.placement_mode.last_time(), self.position_binding.last_time(),
            self.size.last_time(), self.layout_size.last_time(),
            self.line_from.last_time(), self.line_to.last_time(),
            self.arc_angles.last_time(), self.color.last_time(),
            self.shape_type.last_time(), self.opacity.last_time(),
            self.stroke_width.last_time(), self.stroke_color.last_time(),
            self.stroke_progress.last_time(), self.fill_opacity.last_time(),
            self.morph_options.last_time(), self.text_paths.last_time(),
            self.vector_paths.last_time(), self.image.last_time(),
            self.points.last_time(), self.commands.last_time(),
            self.font_family.last_time(), self.font_size.last_time(),
            self.shadow_offset.last_time(), self.shadow_blur.last_time(),
            self.shadow_color.last_time(), self.glow_radius.last_time(),
            self.glow_color.last_time(), self.backdrop_blur.last_time(),
            self.head_size.last_time(),
        ];
        times.into_iter().flatten().max()
    }

    /// Returns true if any property track has animated keyframes.
    /// A track is "animated" if it has 2+ keyframes or 1 keyframe at time > 0.
    pub fn has_any_keyframes(&self) -> bool {
        self.position.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.motion_offset.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.rotation.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.scale.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.transform.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.placement_mode.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.position_binding.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.size.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.layout_size.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.color.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.opacity.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.stroke_width.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.stroke_color.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.stroke_progress.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.fill_opacity.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.morph_options.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.shape_type.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.line_from.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.line_to.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.arc_angles.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.points.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.commands.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.vector_paths.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.text_content.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.font_family.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.font_size.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.text_paths.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.image.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.shadow_offset.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.shadow_blur.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.shadow_color.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.glow_radius.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.glow_color.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.backdrop_blur.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
            || self.head_size.as_ref().map(|t| !t.is_effectively_static()).unwrap_or(false)
    }
}

fn evaluate_paths_with_options<T: Clone + Interpolate>(
    paths: &PropertyTrack<T>,
    morph_options: &PropertyTrack<MorphOptions>,
    time_ms: u64,
    interpolate: fn(&T, &T, f32, MorphOptions) -> T,
) -> T {
    if paths.keyframes.is_empty() { return paths.default_value.clone(); }
    let found = match paths.keyframes.range(time_ms..).next() {
        Some(entry) => entry,
        None => return paths.last_value(),
    };
    let (&found_time, (found_val, found_easing)) = found;
    if let Some((&first_time, _)) = paths.keyframes.iter().next() {
        if time_ms <= first_time { return found_val.clone(); }
    }
    let (prev_time, prev_val) = match paths.keyframes.range(..time_ms).next_back() {
        Some((&t, (val, _))) => (t, val.clone()),
        None => (0, paths.default_value.clone()),
    };
    let duration = (found_time - prev_time) as f32;
    let elapsed = (time_ms - prev_time) as f32;
    let progress = elapsed / duration;
    let eased_progress = apply_easing(progress, *found_easing);
    let options = morph_options.keyframes.get(&found_time).map(|(value, _)| *value).unwrap_or_default();
    interpolate(&prev_val, found_val, eased_progress, options)
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
            });
        }
        for path in target {
            result.push(VelloPath {
                path: path.path.clone(),
                fill: path.fill.map(|c| c.multiply_alpha(target_alpha)),
                stroke: path.stroke.map(|(c, w)| (c.multiply_alpha(target_alpha), w)),
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
            ShadowOffset => TrackFieldRef::Vec2(&self.shadow_offset),
            ShadowBlur => TrackFieldRef::F32(&self.shadow_blur),
            ShadowColor => TrackFieldRef::Vec4(&self.shadow_color),
            GlowRadius => TrackFieldRef::F32(&self.glow_radius),
            GlowColor => TrackFieldRef::Vec4(&self.glow_color),
            BackdropBlur => TrackFieldRef::F32(&self.backdrop_blur),
            ShapeType => TrackFieldRef::ShapeType(&self.shape_type),
            LineFrom => TrackFieldRef::Vec2(&self.line_from),
            LineTo => TrackFieldRef::Vec2(&self.line_to),
            HeadSize => TrackFieldRef::F32(&self.head_size),
            ArcAngles => TrackFieldRef::Vec2(&self.arc_angles),
            Points => TrackFieldRef::PointList(&self.points),
            Commands => TrackFieldRef::CommandList(&self.commands),
            TextContent => TrackFieldRef::String(&self.text_content),
            FontFamily => TrackFieldRef::String(&self.font_family),
            FontSize => TrackFieldRef::F32(&self.font_size),
            PlacementMode => TrackFieldRef::PlacementMode(&self.placement_mode),
            MorphOptions => TrackFieldRef::MorphOptions(&self.morph_options),
            _ => return None,
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
            ShadowOffset => TrackFieldMut::Vec2(&mut self.shadow_offset),
            ShadowBlur => TrackFieldMut::F32(&mut self.shadow_blur),
            ShadowColor => TrackFieldMut::Vec4(&mut self.shadow_color),
            GlowRadius => TrackFieldMut::F32(&mut self.glow_radius),
            GlowColor => TrackFieldMut::Vec4(&mut self.glow_color),
            BackdropBlur => TrackFieldMut::F32(&mut self.backdrop_blur),
            ShapeType => TrackFieldMut::ShapeType(&mut self.shape_type),
            LineFrom => TrackFieldMut::Vec2(&mut self.line_from),
            LineTo => TrackFieldMut::Vec2(&mut self.line_to),
            HeadSize => TrackFieldMut::F32(&mut self.head_size),
            ArcAngles => TrackFieldMut::Vec2(&mut self.arc_angles),
            Points => TrackFieldMut::PointList(&mut self.points),
            Commands => TrackFieldMut::CommandList(&mut self.commands),
            TextContent => TrackFieldMut::String(&mut self.text_content),
            FontFamily => TrackFieldMut::String(&mut self.font_family),
            FontSize => TrackFieldMut::F32(&mut self.font_size),
            PlacementMode => TrackFieldMut::PlacementMode(&mut self.placement_mode),
            MorphOptions => TrackFieldMut::MorphOptions(&mut self.morph_options),
            _ => return None,
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
            "shadow_offset" => ActorField::ShadowOffset,
            "shadow_blur" => ActorField::ShadowBlur,
            "shadow_color" => ActorField::ShadowColor,
            "glow_radius" => ActorField::GlowRadius,
            "glow_color" => ActorField::GlowColor,
            "backdrop_blur" => ActorField::BackdropBlur,
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
            "placement_mode" => ActorField::PlacementMode,
            "morph_options" => ActorField::MorphOptions,
            _ => return false,
        };

        match self.field_ref(field) {
            Some(TrackFieldRef::F32(track)) => track.has_keyframe_at(time_ms),
            Some(TrackFieldRef::Vec2(track)) => track.has_keyframe_at(time_ms),
            Some(TrackFieldRef::Vec4(track)) => track.has_keyframe_at(time_ms),
            Some(TrackFieldRef::Transform(track)) => track.has_keyframe_at(time_ms),
            Some(TrackFieldRef::String(track)) => track.has_keyframe_at(time_ms),
            Some(TrackFieldRef::U32(track)) => track.has_keyframe_at(time_ms),
            Some(TrackFieldRef::PointList(track)) => track.has_keyframe_at(time_ms),
            Some(TrackFieldRef::CommandList(track)) => track.has_keyframe_at(time_ms),
            Some(TrackFieldRef::ShapeType(track)) => track.has_keyframe_at(time_ms),
            Some(TrackFieldRef::PlacementMode(track)) => track.has_keyframe_at(time_ms),
            Some(TrackFieldRef::MorphOptions(track)) => track.has_keyframe_at(time_ms),
            None => false,
        }
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
            "shadow_offset" => ActorField::ShadowOffset,
            "shadow_blur" => ActorField::ShadowBlur,
            "shadow_color" => ActorField::ShadowColor,
            "glow_radius" => ActorField::GlowRadius,
            "glow_color" => ActorField::GlowColor,
            "backdrop_blur" => ActorField::BackdropBlur,
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
            "placement_mode" => ActorField::PlacementMode,
            "morph_options" => ActorField::MorphOptions,
            _ => return Vec::new(),
        };

        let mut times: Vec<u64> = match self.field_ref(field) {
            Some(TrackFieldRef::F32(track)) => track.as_ref().map(|t| t.keyframes.keys().copied().collect()).unwrap_or_default(),
            Some(TrackFieldRef::Vec2(track)) => track.as_ref().map(|t| t.keyframes.keys().copied().collect()).unwrap_or_default(),
            Some(TrackFieldRef::Vec4(track)) => track.as_ref().map(|t| t.keyframes.keys().copied().collect()).unwrap_or_default(),
            Some(TrackFieldRef::Transform(track)) => track.as_ref().map(|t| t.keyframes.keys().copied().collect()).unwrap_or_default(),
            Some(TrackFieldRef::String(track)) => track.as_ref().map(|t| t.keyframes.keys().copied().collect()).unwrap_or_default(),
            Some(TrackFieldRef::U32(track)) => track.as_ref().map(|t| t.keyframes.keys().copied().collect()).unwrap_or_default(),
            Some(TrackFieldRef::PointList(track)) => track.as_ref().map(|t| t.keyframes.keys().copied().collect()).unwrap_or_default(),
            Some(TrackFieldRef::CommandList(track)) => track.as_ref().map(|t| t.keyframes.keys().copied().collect()).unwrap_or_default(),
            Some(TrackFieldRef::ShapeType(track)) => track.as_ref().map(|t| t.keyframes.keys().copied().collect()).unwrap_or_default(),
            Some(TrackFieldRef::PlacementMode(track)) => track.as_ref().map(|t| t.keyframes.keys().copied().collect()).unwrap_or_default(),
            Some(TrackFieldRef::MorphOptions(track)) => track.as_ref().map(|t| t.keyframes.keys().copied().collect()).unwrap_or_default(),
            None => Vec::new(),
        };
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
            ActorKindId::Math,
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
        }];
        let target = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 255)),
            stroke: None,
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
        }];
        let target = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 255)),
            stroke: None,
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
        }];
        let target = vec![VelloPath {
            path: BezPath::new(),
            fill: Some(vello::peniko::Color::from_rgba8(0, 255, 0, 128)),
            stroke: None,
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
