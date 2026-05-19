use crate::easing::{Easing, apply_easing};
use crate::renderer::types::{TextPath, VelloPath};
use crate::timeline::morph::{
    MorphOptions, MorphStrategy, align_path_lists_with_strategy, morph_paths_with_options,
};
use crate::timeline::plot::ProceduralPlot;
use crate::timeline::shapes::ShapeType;
use std::collections::BTreeMap;

pub const DEFAULT_LAYOUT_HALF_SIZE: [f32; 2] = [50.0, 50.0];
pub const DEFAULT_WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

// ─────────────────────────────────────────────────────────────
// Actor kind identification
// ─────────────────────────────────────────────────────────────

/// Stable, compile-time constant identifying an actor's type.
/// Set once at first declaration and never changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActorKindId {
    Shape(ShapeKind),
    Text,
    Math,
    Code,
    Typst,
    Image,
    Svg,
    Graph,
    PlotCurve,
    VectorField,
    Heatmap,
    ContourSet,
    Row,
    Col,
    Grid,
    Stack,
    Group,
}

impl ActorKindId {
    pub fn from_type_name(ty: &str) -> Option<Self> {
        crate::primitives::find_primitive(ty).map(|p| p.kind_id())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    Rect, Ellipse, Line, Polygon, Path,
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
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Actor kind metadata registry
// ─────────────────────────────────────────────────────────────

/// High-level category for grouping actor kinds in UI palettes and docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActorCategory {
    Shape,
    Text,
    Media,
    Plot,
    Container,
}

impl ActorCategory {
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

pub fn actor_kind_registry() -> &'static [ActorKindMeta] {
    crate::primitives::actor_kind_registry()
}

pub fn actor_kind_meta(kind: ActorKindId) -> &'static ActorKindMeta {
    crate::primitives::actor_kind_meta(kind)
}

pub fn actor_kind_meta_by_name(name: &str) -> Option<&'static ActorKindMeta> {
    crate::primitives::actor_kind_meta_by_name(name)
}

/// Extension trait for lazy property track access
pub trait TrackAccessor<T: Interpolate + Clone> {
    fn get(&self, time_ms: u64, default: T) -> T;
    fn ensure(&mut self, default: T) -> &mut PropertyTrack<T>;
    fn last(&self, default: T) -> T;
    fn last_time(&self) -> Option<u64>;
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

pub trait Interpolate {
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

impl Interpolate for u32 {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementMode {
    LayoutManaged,
    Manual,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeMode {
    Size,
    Scale,
}

impl Interpolate for PlacementMode {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneAnchor {
    TopLeft, Top, TopRight, Left, Center, Right, BottomLeft, Bottom, BottomRight,
}

impl Interpolate for SceneAnchor {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PositionBinding {
    Absolute,
    SceneAnchor { anchor: SceneAnchor, offset: [f32; 2] },
    ScenePercent { x: f32, y: f32, offset: [f32; 2] },
    ContainerDefault { anchor: SceneAnchor },
    ContainerPercent { x: f32, y: f32 },
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

#[derive(Clone)]
pub struct PropertyTrack<T> {
    pub keyframes: BTreeMap<u64, (T, Easing)>,
    pub default_value: T,
}

impl<T: Interpolate + Clone> PropertyTrack<T> {
    pub fn new(default_value: T) -> Self {
        Self { keyframes: BTreeMap::new(), default_value }
    }
    pub fn add_keyframe(&mut self, time_ms: u64, value: T, easing: Easing) {
        self.keyframes.insert(time_ms, (value, easing));
    }
    pub fn evaluate(&self, time_ms: u64) -> T {
        if self.keyframes.is_empty() { return self.default_value.clone(); }
        let found = match self.keyframes.range(time_ms..).next() {
            Some(entry) => entry,
            None => return self.last_value(),
        };
        let (&found_time, (found_val, found_easing)) = found;
        if let Some((&first_time, _)) = self.keyframes.iter().next() {
            if time_ms <= first_time { return found_val.clone(); }
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
    }
    pub fn last_value(&self) -> T {
        self.keyframes.iter().next_back().map(|(_, (val, _))| val.clone())
            .unwrap_or_else(|| self.default_value.clone())
    }
    pub fn last_keyframe_time(&self) -> Option<u64> {
        self.keyframes.keys().next_back().copied()
    }
}

// ─────────────────────────────────────────────────────────────
// NEW: Tier 1 — Always-present header (alongside old fields)
// ─────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ActorHeader {
    pub label: String,
    pub kind: ActorKindId,
    pub first_seen_ms: u64,
    pub children: Vec<String>,
    pub parent_label: Option<String>,
}

// ─────────────────────────────────────────────────────────────
// NEW: Tier 2 — Universal geometry + style (alongside old fields)
// ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct GeometryTier {
    pub position: Option<PropertyTrack<[f32; 2]>>,
    pub motion_offset: Option<PropertyTrack<[f32; 2]>>,
    pub size: Option<PropertyTrack<[f32; 2]>>,
    pub layout_size: Option<PropertyTrack<[f32; 2]>>,
    pub rotation: Option<PropertyTrack<f32>>,
    pub scale: Option<PropertyTrack<f32>>,
    pub placement_mode: Option<PropertyTrack<PlacementMode>>,
    pub position_binding: Option<PropertyTrack<PositionBinding>>,
}

#[derive(Clone)]
pub struct StyleTier {
    pub color: Option<PropertyTrack<[f32; 4]>>,
    pub opacity: Option<PropertyTrack<f32>>,
    pub stroke_width: Option<PropertyTrack<f32>>,
    pub stroke_color: Option<PropertyTrack<[f32; 4]>>,
    pub stroke_progress: Option<PropertyTrack<f32>>,
    pub fill_opacity: Option<PropertyTrack<f32>>,
    pub morph_options: Option<PropertyTrack<MorphOptions>>,
}

// ─────────────────────────────────────────────────────────────
// NEW: Tier 3 — Kind-specific payload (alongside old fields)
// ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub enum ActorPayload {
    Empty,
    Shape {
        shape_type: Option<PropertyTrack<ShapeType>>,
        line_from: Option<PropertyTrack<[f32; 2]>>,
        line_to: Option<PropertyTrack<[f32; 2]>>,
        arc_angles: Option<PropertyTrack<[f32; 2]>>,
        points: Option<PropertyTrack<Vec<[f32; 2]>>>,
        vector_paths: Option<PropertyTrack<Vec<VelloPath>>>,
    },
    Text {
        content: Option<PropertyTrack<String>>,
        text_paths: Option<PropertyTrack<Vec<TextPath>>>,
    },
    Image {
        image: Option<PropertyTrack<Option<crate::timeline::image::SceneImage>>>,
    },
    Svg {
        svg_paths: Vec<VelloPath>,
    },
    Plot {
        vector_paths: Option<PropertyTrack<Vec<VelloPath>>>,
    },
}

// ─────────────────────────────────────────────────────────────
// AnimationTrack — ORIGINAL flat struct preserved for compat
// ─────────────────────────────────────────────────────────────
// The old flat fields remain so all existing code compiles without changes.
// The NEW tiered fields (`geom`, `sty`, `pay`) are also present for gradual
// migration. New code should use the tiered fields; old code continues to
// use the flat fields.
//
// MIGRATION:
//   Phase 2: Keep both. All code compiles.
//   Phase 3: Migrate call sites from flat fields to tiered fields.
//   Phase 4: Remove flat fields.

#[derive(Clone)]
pub struct AnimationTrack {
    // ── Identity / metadata ──
    pub label: String,
    pub kind: ActorKindId,
    pub first_seen_ms: u64,
    pub children: Vec<String>,

    // ── Geometry tier (flat compat fields) ──
    pub position: Option<PropertyTrack<[f32; 2]>>,
    pub motion_offset: Option<PropertyTrack<[f32; 2]>>,
    pub rotation: Option<PropertyTrack<f32>>,
    pub scale: Option<PropertyTrack<f32>>,
    pub placement_mode: Option<PropertyTrack<PlacementMode>>,
    pub position_binding: Option<PropertyTrack<PositionBinding>>,
    pub size: Option<PropertyTrack<[f32; 2]>>,

    // ── Style tier (flat compat fields) ──
    pub color: Option<PropertyTrack<[f32; 4]>>,
    pub opacity: Option<PropertyTrack<f32>>,
    pub stroke_width: Option<PropertyTrack<f32>>,
    pub stroke_color: Option<PropertyTrack<[f32; 4]>>,
    pub stroke_progress: Option<PropertyTrack<f32>>,
    pub fill_opacity: Option<PropertyTrack<f32>>,
    pub morph_options: Option<PropertyTrack<MorphOptions>>,

    // ── Effects tier ──
    pub shadow_offset: Option<PropertyTrack<[f32; 2]>>,
    pub shadow_blur: Option<PropertyTrack<f32>>,
    pub shadow_color: Option<PropertyTrack<[f32; 4]>>,
    pub glow_radius: Option<PropertyTrack<f32>>,
    pub glow_color: Option<PropertyTrack<[f32; 4]>>,

    // ── Shape payload (flat compat fields) ──
    pub shape_type: Option<PropertyTrack<ShapeType>>,
    pub line_from: Option<PropertyTrack<[f32; 2]>>,
    pub line_to: Option<PropertyTrack<[f32; 2]>>,
    pub arc_angles: Option<PropertyTrack<[f32; 2]>>,
    pub points: Option<PropertyTrack<Vec<[f32; 2]>>>,
    pub commands: Option<PropertyTrack<String>>,
    pub vector_paths: Option<PropertyTrack<Vec<VelloPath>>>,

    // ── Text / media payload (flat compat fields) ──
    pub text_content: Option<PropertyTrack<String>>,
    pub font_family: Option<PropertyTrack<String>>,
    pub font_size: Option<PropertyTrack<f32>>,
    pub text_paths: Option<PropertyTrack<Vec<TextPath>>>,
    pub svg_paths: Vec<crate::timeline::VelloPath>,
    pub image: Option<PropertyTrack<Option<crate::timeline::image::SceneImage>>>,

    // ── Layout ──
    pub layout_size: Option<PropertyTrack<[f32; 2]>>,

    // ── Procedural plot (re-sampled at frame time) ──
    pub procedural_plot: Option<ProceduralPlot>,
}

impl AnimationTrack {
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

            // Shape flat fields
            shape_type: None,
            line_from: None,
            line_to: None,
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
    pub fn layout_size_get(&self, time_ms: u64) -> Option<[f32; 2]> {
        self.layout_size.as_ref().map(|t| t.evaluate(time_ms))
    }
    pub fn layout_size_last(&self) -> Option<[f32; 2]> {
        self.layout_size.as_ref().map(|t| t.last_value())
    }
    pub fn ensure_layout_size(&mut self, default: [f32; 2]) -> &mut PropertyTrack<[f32; 2]> {
        self.layout_size.get_or_insert_with(|| PropertyTrack::new(default))
    }
    pub fn has_layout_size(&self) -> bool {
        self.layout_size.is_some()
    }

    // ── Path evaluation ──
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

    pub fn evaluate_vector_paths(&self, time_ms: u64) -> Vec<VelloPath> {
        let default_paths = PropertyTrack::new(Vec::new());
        let paths_track = self.vector_paths.as_ref().unwrap_or(&default_paths);
        let default_morph = PropertyTrack::new(MorphOptions::default());
        let morph_track = self.morph_options.as_ref().unwrap_or(&default_morph);
        evaluate_paths_with_options(paths_track, morph_track, time_ms, interpolate_vello_paths)
    }

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
            self.glow_color.last_time(),
        ];
        times.into_iter().flatten().max()
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
}
