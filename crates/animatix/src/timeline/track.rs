use crate::easing::{Easing, apply_easing};
use super::property_track::{Interpolate, PropertyTrack};
use crate::renderer::types::{TextPath, VelloPath};
use crate::timeline::morph::{
    MorphOptions, MorphStrategy, align_path_lists_with_strategy, morph_paths_with_options,
};
use crate::timeline::shapes::ShapeType;

pub use super::dispatch::{AnimationTrack, TrackFieldRef, TrackFieldMut};

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
    /// Entrance actions (fade-in, wipe-in, etc.) - green.
    Entrance,
    /// Motion actions (move, shift, rotate, scale) - blue.
    Motion,
    /// Exit actions (fade-out, wipe-out) - red.
    Exit,
    /// Effect actions (bounce, pulse, shake) - amber.
    Effect,
    /// Reorder actions (swap, reorder) - purple.
    Reorder,
    /// Reveal actions (draw-in, reveal-in, draw-out, reveal-out) - cyan.
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
        /// Horizontal percentage (0-1).
        x: f32,
        /// Vertical percentage (0-1).
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
        /// Horizontal percentage (0-1).
        x: f32,
        /// Vertical percentage (0-1).
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
// ShapeTracks sub-struct
// ─────────────────────────────────────────────────────────────

/// Sub-struct holding all shape-related property tracks.
#[derive(Clone, Debug, Default)]
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
    /// Polygon vertex list.
    pub points: Option<PropertyTrack<Vec<[f32; 2]>>>,
    /// Path command string (e.g. SVG path data).
    pub commands: Option<PropertyTrack<String>>,
    /// Pre-built vector paths for shape rendering.
    pub vector_paths: Option<PropertyTrack<Vec<VelloPath>>>,
}

// ─────────────────────────────────────────────────────────────
// TextTracks sub-struct
// ─────────────────────────────────────────────────────────────

/// Sub-struct holding all text-related property tracks.
#[derive(Clone, Debug, Default)]
pub struct TextTracks {
    /// Raw text content.
    pub text_content: Option<PropertyTrack<String>>,
    /// Font family name.
    pub font_family: Option<PropertyTrack<String>>,
    /// Font size in points.
    pub font_size: Option<PropertyTrack<f32>>,
    /// Font weight (100-900).
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
    /// Font ascent in scene units (points), used for baseline alignment.
    pub ascent: Option<PropertyTrack<f32>>,
    /// Font descent in scene units (points), used for baseline alignment.
    pub descent: Option<PropertyTrack<f32>>,
    /// Baseline offset from text center, used for baseline alignment.
    /// A positive value means the baseline is above the text center.
    pub baseline: Option<PropertyTrack<f32>>,
}

// ─────────────────────────────────────────────────────────────
// StyleTracks sub-struct
// ─────────────────────────────────────────────────────────────

/// Sub-struct holding all style-related property tracks.
#[derive(Clone, Debug, Default)]
pub struct StyleTracks {
    /// Fill color in RGBA.
    pub color: Option<PropertyTrack<[f32; 4]>>,
    /// Overall opacity multiplier.
    pub opacity: Option<PropertyTrack<f32>>,
    /// Stroke width.
    pub stroke_width: Option<PropertyTrack<f32>>,
    /// Stroke color in RGBA.
    pub stroke_color: Option<PropertyTrack<[f32; 4]>>,
    /// Stroke draw progress (0-1).
    pub stroke_progress: Option<PropertyTrack<f32>>,
    /// Fill opacity multiplier.
    pub fill_opacity: Option<PropertyTrack<f32>>,
    /// Stroke line cap (0=Butt, 1=Round, 2=Square).
    pub line_cap: Option<PropertyTrack<u32>>,
    /// Stroke line join (0=Miter, 1=Round, 2=Bevel).
    pub line_join: Option<PropertyTrack<u32>>,
    /// Path morphing options.
    pub morph_options: Option<PropertyTrack<MorphOptions>>,
}

// ─────────────────────────────────────────────────────────────
// GeometryTracks sub-struct
// ─────────────────────────────────────────────────────────────

/// Sub-struct holding all geometry-related property tracks.
#[derive(Clone, Debug, Default)]
pub struct GeometryTracks {
    /// Position track (x, y).
    pub position: Option<PropertyTrack<[f32; 2]>>,
    /// Motion offset applied after layout.
    pub motion_offset: Option<PropertyTrack<[f32; 2]>>,
    /// Rotation angle in radians.
    pub rotation: Option<PropertyTrack<f32>>,
    /// Scale factor.
    pub scale: Option<PropertyTrack<f32>>,
    /// 2×3 affine transform matrix.
    pub transform: Option<PropertyTrack<[f32; 6]>>,
    /// Whether the actor is layout-managed or manually placed.
    pub placement_mode: Option<PropertyTrack<PlacementMode>>,
    /// Position binding strategy.
    pub position_binding: Option<PropertyTrack<PositionBinding>>,
    /// Width and height.
    pub size: Option<PropertyTrack<[f32; 2]>>,
    /// Size allocated by the layout system.
    pub layout_size: Option<PropertyTrack<[f32; 2]>>,
    /// Size specification for percentage/auto/fill sizing (non-animated, set at build time).
    pub size_spec: Option<crate::timeline::taffy_layout::ChildSizeSpec>,
    /// Minimum width constraint.
    pub min_width: Option<PropertyTrack<f32>>,
    /// Minimum height constraint.
    pub min_height: Option<PropertyTrack<f32>>,
    /// Maximum height constraint.
    pub max_height: Option<PropertyTrack<f32>>,
}

// ─────────────────────────────────────────────────────────────


pub(crate) fn evaluate_paths_with_options<T: Interpolate>(
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
        // No interior segment - use default or boundary value
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

pub(crate) fn interpolate_text_paths(source: &Vec<TextPath>, target: &Vec<TextPath>, t: f32, options: MorphOptions) -> Vec<TextPath> {
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

pub(crate) fn interpolate_vello_paths(source: &Vec<VelloPath>, target: &Vec<VelloPath>, t: f32, options: MorphOptions) -> Vec<VelloPath> {
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



// ─────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::property_registry::ActorField;
    use crate::timeline::property_track::TrackAccessor;
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
        track.geometry.transform.ensure([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
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
        track.style.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);
        track.geometry.position.ensure([0.0, 0.0]).add_keyframe(5000, [100.0, 100.0], Easing::Linear);
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
        track.text.ascent.ensure(0.0).add_keyframe(500, 10.0, Easing::Linear);
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
        track.style.opacity.ensure(1.0).add_keyframe(0, 0.5, Easing::Linear);
        assert!(!track.has_any_keyframes());
    }

    #[test]
    fn test_has_any_keyframes_returns_true_for_multiple_keyframes() {
        let mut track = AnimationTrack::new("test".to_string());
        track.style.opacity.ensure(1.0).add_keyframe(0, 0.5, Easing::Linear);
        track.style.opacity.ensure(0.5).add_keyframe(1000, 1.0, Easing::Linear);
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
        track.geometry.min_width.ensure(0.0).add_keyframe(7777, 50.0, Easing::Linear);
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
        track.geometry.min_height.ensure(0.0).add_keyframe(4000, 200.0, Easing::Linear);
        assert_eq!(track.max_keyframe_time(), Some(4000));
    }

    #[test]
    fn test_registry_driven_max_keyframe_time_finds_letter_spacing() {
        let mut track = AnimationTrack::new("test".to_string());
        track.text.letter_spacing.ensure(0.0).add_keyframe(2500, 1.0, Easing::Linear);
        assert_eq!(track.max_keyframe_time(), Some(2500));
    }

    #[test]
    fn test_registry_driven_has_any_keyframes_finds_word_spacing() {
        let mut track = AnimationTrack::new("test".to_string());
        track.text.word_spacing.ensure(0.0).add_keyframe(1000, 5.0, Easing::Linear);
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
        track.style.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
        let rf = track.field_ref(ActorField::Opacity).unwrap();
        assert!(rf.has_keyframe_at(500));
        assert!(!rf.has_keyframe_at(0));
        assert!(!rf.has_keyframe_at(1000));
    }

    #[test]
    fn test_track_field_ref_keyframe_count() {
        let mut track = AnimationTrack::new("test".to_string());
        track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        track.style.opacity.ensure(1.0).add_keyframe(500, 0.5, Easing::Linear);
        track.style.opacity.ensure(0.5).add_keyframe(1000, 0.0, Easing::Linear);
        let rf = track.field_ref(ActorField::Opacity).unwrap();
        assert_eq!(rf.keyframe_count(), 3);
    }

    #[test]
    fn test_track_field_ref_keyframe_times() {
        let mut track = AnimationTrack::new("test".to_string());
        track.style.opacity.ensure(1.0).add_keyframe(1000, 0.0, Easing::Linear);
        track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::Linear);
        let rf = track.field_ref(ActorField::Opacity).unwrap();
        let mut times = rf.keyframe_times();
        times.sort();
        assert_eq!(times, vec![0, 1000]);
    }

    #[test]
    fn test_track_field_ref_keyframe_easing() {
        let mut track = AnimationTrack::new("test".to_string());
        track.style.opacity.ensure(1.0).add_keyframe(0, 1.0, Easing::EaseOut);
        track.style.opacity.ensure(0.0).add_keyframe(500, 0.0, Easing::Linear);
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

        assert_eq!(track.geometry.position.get(0, [0.0, 0.0]), [0.0, 0.0]);
        assert_eq!(
            track.geometry.placement_mode.get(0, PlacementMode::LayoutManaged),
            PlacementMode::LayoutManaged
        );
        assert_eq!(
            track.geometry.position_binding.get(0, PositionBinding::Absolute),
            PositionBinding::Absolute
        );
        assert_eq!(track.geometry.size.get(0, [50.0, 50.0]), [50.0, 50.0]);
        assert_eq!(track.shape.line_from.get(0, [-50.0, 0.0]), [-50.0, 0.0]);
        assert_eq!(track.shape.line_to.get(0, [50.0, 0.0]), [50.0, 0.0]);
        assert_eq!(track.shape.arc_angles.get(0, [0.0, std::f32::consts::PI]), [0.0, std::f32::consts::PI]);
        assert_eq!(track.style.color.get(0, [1.0, 1.0, 1.0, 1.0]), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(track.shape.shape_type.get(0, ShapeType::Rect), ShapeType::Rect);
        assert_eq!(track.style.opacity.get(0, 1.0), 1.0);
    }
}
