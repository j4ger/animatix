use crate::easing::{Easing, apply_easing};
use crate::renderer::types::{TextPath, VelloPath};
use crate::timeline::morph::{
    MorphOptions, align_path_lists_with_strategy, morph_paths_with_options,
};
use crate::timeline::shapes::ShapeType;
use std::collections::BTreeMap;

pub const DEFAULT_LAYOUT_HALF_SIZE: [f32; 2] = [50.0, 50.0];

#[derive(Clone)]
pub enum LayoutSizeState {
    Unseeded,
    Seeded(PropertyTrack<[f32; 2]>),
}

impl Default for LayoutSizeState {
    fn default() -> Self {
        Self::Unseeded
    }
}

/// Extension trait for lazy property track access
pub trait TrackAccessor<T: Interpolate + Clone> {
    /// Get the track's value at time_ms, falling back to default if track doesn't exist
    fn get(&self, time_ms: u64, default: T) -> T;
    /// Ensure the track exists and return a mutable reference to it
    fn ensure(&mut self, default: T) -> &mut PropertyTrack<T>;
    /// Get the last value, falling back to default
    fn last(&self, default: T) -> T;
    /// Get the last keyframe time
    fn last_time(&self) -> Option<u64>;
    /// Check if there's a keyframe at or before the given time
    fn has_keyframe_at(&self, time_ms: u64) -> bool;
}

impl<T: Interpolate + Clone> TrackAccessor<T> for Option<PropertyTrack<T>> {
    fn get(&self, time_ms: u64, default: T) -> T {
        self.as_ref()
            .map(|t| t.evaluate(time_ms))
            .unwrap_or(default)
    }

    fn ensure(&mut self, default: T) -> &mut PropertyTrack<T> {
        self.get_or_insert_with(|| PropertyTrack::new(default))
    }

    fn last(&self, default: T) -> T {
        self.as_ref()
            .map(|t| t.last_value())
            .unwrap_or(default)
    }

    fn last_time(&self) -> Option<u64> {
        self.as_ref().and_then(|t| t.last_keyframe_time())
    }

    fn has_keyframe_at(&self, time_ms: u64) -> bool {
        self.as_ref()
            .map(|t| t.keyframes.contains_key(&time_ms))
            .unwrap_or(false)
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
        [
            self[0] + (other[0] - self[0]) * t,
            self[1] + (other[1] - self[1]) * t,
        ]
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

impl Interpolate for PlacementMode {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneAnchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Interpolate for SceneAnchor {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PositionBinding {
    Absolute,
    SceneAnchor {
        anchor: SceneAnchor,
        offset: [f32; 2],
    },
    ScenePercent {
        x: f32,
        y: f32,
        offset: [f32; 2],
    },
    ContainerDefault {
        anchor: SceneAnchor,
    },
    /// Layout-managed by parent container, but with a percentage offset.
    /// The container computes the base position, then applies (x%, y%) offset.
    ContainerPercent {
        x: f32,
        y: f32,
    },
}

impl Interpolate for PositionBinding {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        match (*self, *other) {
            (Self::Absolute, Self::Absolute) => Self::Absolute,
            (
                Self::SceneAnchor {
                    anchor,
                    offset: start_offset,
                },
                Self::SceneAnchor {
                    anchor: other_anchor,
                    offset: end_offset,
                },
            ) if anchor == other_anchor => Self::SceneAnchor {
                anchor,
                offset: start_offset.interpolate(&end_offset, t),
            },
            (
                Self::ScenePercent {
                    x: start_x,
                    y: start_y,
                    offset: start_offset,
                },
                Self::ScenePercent {
                    x: end_x,
                    y: end_y,
                    offset: end_offset,
                },
            ) => Self::ScenePercent {
                x: start_x.interpolate(&end_x, t),
                y: start_y.interpolate(&end_y, t),
                offset: start_offset.interpolate(&end_offset, t),
            },
            (
                Self::ContainerDefault { anchor },
                Self::ContainerDefault {
                    anchor: other_anchor,
                },
            ) if anchor == other_anchor => Self::ContainerDefault { anchor },
            (
                Self::ContainerPercent { x: x1, y: y1 },
                Self::ContainerPercent { x: x2, y: y2 },
            ) => Self::ContainerPercent {
                x: x1.interpolate(&x2, t),
                y: y1.interpolate(&y2, t),
            },
            _ => {
                if t < 0.5 {
                    *self
                } else {
                    *other
                }
            }
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

impl Interpolate for Vec<[f32; 2]> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if self.is_empty() || other.is_empty() || self.len() != other.len() {
            // If lengths don't match or either is empty, just switch at t=0.5
            if t < 0.5 { self.clone() } else { other.clone() }
        } else {
            // Interpolate each point pair
            self.iter()
                .zip(other.iter())
                .map(|(a, b)| [
                    a[0] + (b[0] - a[0]) * t,
                    a[1] + (b[1] - a[1]) * t,
                ])
                .collect()
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
        Self {
            keyframes: BTreeMap::new(),
            default_value,
        }
    }

    pub fn add_keyframe(&mut self, time_ms: u64, value: T, easing: Easing) {
        self.keyframes.insert(time_ms, (value, easing));
    }

    pub fn evaluate(&self, time_ms: u64) -> T {
        if self.keyframes.is_empty() {
            return self.default_value.clone();
        }

        // Find the first keyframe at or after time_ms
        let found = match self.keyframes.range(time_ms..).next() {
            Some(entry) => entry,
            None => {
                // time_ms is past all keyframes, return last value
                return self.last_value();
            }
        };
        let (&found_time, (found_val, found_easing)) = found;

        // If time_ms is at or before the first keyframe, return found value directly
        if let Some((&first_time, _)) = self.keyframes.iter().next() {
            if time_ms <= first_time {
                return found_val.clone();
            }
        }

        // Find the previous keyframe
        let (prev_time, prev_val) = match self.keyframes.range(..time_ms).next_back() {
            Some((&t, (val, _))) => (t, val.clone()),
            None => (0, self.default_value.clone()),
        };

        // Interpolate between prev_val and found_val
        let duration = (found_time - prev_time) as f32;
        let elapsed = (time_ms - prev_time) as f32;
        let progress = elapsed / duration;
        let eased_progress = apply_easing(progress, *found_easing);
        prev_val.interpolate(found_val, eased_progress)
    }

    pub fn last_value(&self) -> T {
        self.keyframes
            .iter()
            .next_back()
            .map(|(_, (val, _))| val.clone())
            .unwrap_or_else(|| self.default_value.clone())
    }

    pub fn last_keyframe_time(&self) -> Option<u64> {
        self.keyframes.keys().next_back().copied()
    }
}

#[derive(Clone)]
pub struct AnimationTrack {
    pub label: String,
    pub position: Option<PropertyTrack<[f32; 2]>>,
    pub motion_offset: Option<PropertyTrack<[f32; 2]>>,
    pub rotation: Option<PropertyTrack<f32>>,
    pub scale: Option<PropertyTrack<f32>>,
    pub placement_mode: Option<PropertyTrack<PlacementMode>>,
    pub position_binding: Option<PropertyTrack<PositionBinding>>,
    /// Legacy/general geometric half-extents used by rendering/runtime paths.
    ///
    /// This remains populated for compatibility and non-layout consumers, but
    /// container layout no longer reads this field directly.
    pub size: Option<PropertyTrack<[f32; 2]>>,
    /// Dedicated layout half-extents consumed by container layout.
    ///
    /// This is the authoritative layout measurement source for admitted layout
    /// children. The legacy `size` track remains alongside it for rendering and
    /// runtime compatibility.
    pub layout_size: LayoutSizeState,
    pub line_from: Option<PropertyTrack<[f32; 2]>>,
    pub line_to: Option<PropertyTrack<[f32; 2]>>,
    pub arc_angles: Option<PropertyTrack<[f32; 2]>>,
    pub color: Option<PropertyTrack<[f32; 4]>>,
    pub shape_type: Option<PropertyTrack<ShapeType>>,
    pub opacity: Option<PropertyTrack<f32>>,
    pub stroke_width: Option<PropertyTrack<f32>>,
    pub stroke_color: Option<PropertyTrack<[f32; 4]>>,
    pub stroke_progress: Option<PropertyTrack<f32>>,
    pub fill_opacity: Option<PropertyTrack<f32>>,
    pub morph_options: Option<PropertyTrack<MorphOptions>>,
    pub text_content: Option<PropertyTrack<String>>,
    pub text_paths: Option<PropertyTrack<Vec<TextPath>>>,
    pub vector_paths: Option<PropertyTrack<Vec<VelloPath>>>,
    pub svg_paths: Vec<crate::timeline::VelloPath>,
    pub image: Option<PropertyTrack<Option<crate::timeline::image::SceneImage>>>,
    pub points: Option<PropertyTrack<Vec<[f32; 2]>>>,
    /// The first time this actor was seen in the timeline (ms).
    /// Used to hide actors before their first declaration.
    pub first_seen_ms: u64,
    /// Children of this actor in the scene graph, by label.
    pub children: Vec<String>,
}

impl AnimationTrack {
    pub fn new(label: String) -> Self {
        Self {
            label,
            position: None,
            motion_offset: None,
            rotation: None,
            scale: None,
            placement_mode: None,
            position_binding: None,
            size: None,
            layout_size: LayoutSizeState::Unseeded,
            line_from: None,
            line_to: None,
            arc_angles: None,
            color: None,
            shape_type: None,
            opacity: None,
            stroke_width: None,
            stroke_color: None,
            stroke_progress: None,
            fill_opacity: None,
            morph_options: None,
            text_content: None,
            text_paths: None,
            vector_paths: None,
            svg_paths: Vec::new(),
            image: None,
            points: None,
            first_seen_ms: u64::MAX,
            children: Vec::new(),
        }
    }

    pub fn evaluate_text_paths(&self, time_ms: u64) -> Vec<TextPath> {
        // Check if text content has been dynamically changed via runtime assignment.
        // If text_content has keyframes, it means the text was modified after the
        // initial declaration. Since we can't recompile Typst text to paths at
        // runtime, we return empty paths to indicate the text should be invisible
        // until the next scene build.
        if let Some(content_track) = &self.text_content {
            if !content_track.keyframes.is_empty() {
                let current_text = content_track.evaluate(time_ms);
                if !current_text.is_empty() {
                    // Text was assigned at runtime but we can't recompile paths.
                    // Return empty to avoid showing stale compiled paths.
                    return Vec::new();
                }
            }
        }

        let default_paths = PropertyTrack::new(Vec::new());
        let paths_track = self.text_paths.as_ref().unwrap_or(&default_paths);
        let default_morph = PropertyTrack::new(MorphOptions::default());
        let morph_track = self.morph_options.as_ref().unwrap_or(&default_morph);
        evaluate_paths_with_options(
            paths_track,
            morph_track,
            time_ms,
            interpolate_text_paths,
        )
    }

    pub fn evaluate_vector_paths(&self, time_ms: u64) -> Vec<VelloPath> {
        let default_paths = PropertyTrack::new(Vec::new());
        let paths_track = self.vector_paths.as_ref().unwrap_or(&default_paths);
        let default_morph = PropertyTrack::new(MorphOptions::default());
        let morph_track = self.morph_options.as_ref().unwrap_or(&default_morph);
        evaluate_paths_with_options(
            paths_track,
            morph_track,
            time_ms,
            interpolate_vello_paths,
        )
    }

    pub fn max_keyframe_time(&self) -> Option<u64> {
        let times: Vec<Option<u64>> = vec![
            self.position.last_time(),
            self.motion_offset.last_time(),
            self.rotation.last_time(),
            self.scale.last_time(),
            self.placement_mode.last_time(),
            self.position_binding.last_time(),
            self.size.last_time(),
            self.layout_size.last_time(),
            self.line_from.last_time(),
            self.line_to.last_time(),
            self.arc_angles.last_time(),
            self.color.last_time(),
            self.shape_type.last_time(),
            self.opacity.last_time(),
            self.stroke_width.last_time(),
            self.stroke_color.last_time(),
            self.stroke_progress.last_time(),
            self.fill_opacity.last_time(),
            self.morph_options.last_time(),
            self.text_paths.last_time(),
            self.vector_paths.last_time(),
            self.image.last_time(),
            self.points.last_time(),
        ];
        times.into_iter().flatten().max()
    }

    pub fn layout_size_get(&self, time_ms: u64) -> Option<[f32; 2]> {
        self.layout_size.get(time_ms)
    }

    pub fn layout_size_last(&self) -> Option<[f32; 2]> {
        self.layout_size.last()
    }

    pub fn ensure_layout_size(&mut self, default: [f32; 2]) -> &mut PropertyTrack<[f32; 2]> {
        self.layout_size.ensure(default)
    }

    pub fn has_layout_size(&self) -> bool {
        self.layout_size.is_seeded()
    }
}

impl LayoutSizeState {
    pub fn get(&self, time_ms: u64) -> Option<[f32; 2]> {
        match self {
            Self::Unseeded => None,
            Self::Seeded(track) => Some(track.evaluate(time_ms)),
        }
    }

    pub fn last(&self) -> Option<[f32; 2]> {
        match self {
            Self::Unseeded => None,
            Self::Seeded(track) => Some(track.last_value()),
        }
    }

    pub fn last_time(&self) -> Option<u64> {
        match self {
            Self::Unseeded => None,
            Self::Seeded(track) => track.last_keyframe_time(),
        }
    }

    pub fn ensure(&mut self, default: [f32; 2]) -> &mut PropertyTrack<[f32; 2]> {
        match self {
            Self::Unseeded => {
                *self = Self::Seeded(PropertyTrack::new(default));
                match self {
                    Self::Seeded(track) => track,
                    Self::Unseeded => unreachable!(),
                }
            }
            Self::Seeded(track) => track,
        }
    }

    pub fn is_seeded(&self) -> bool {
        matches!(self, Self::Seeded(_))
    }

    pub fn preserve_instant_delayed_value(&mut self, default: [f32; 2], t_start_ms: u64) {
        if t_start_ms == 0 {
            return;
        }

        let previous_time = t_start_ms.saturating_sub(1);
        let inner = self.ensure(default);

        if inner.keyframes.contains_key(&previous_time) {
            return;
        }

        let previous_value = inner.evaluate(previous_time);
        inner.add_keyframe(previous_time, previous_value, Easing::Linear);
    }
}

fn evaluate_paths_with_options<T: Clone + Interpolate>(
    paths: &PropertyTrack<T>,
    morph_options: &PropertyTrack<MorphOptions>,
    time_ms: u64,
    interpolate: fn(&T, &T, f32, MorphOptions) -> T,
) -> T {
    if paths.keyframes.is_empty() {
        return paths.default_value.clone();
    }

    // Find the first keyframe at or after time_ms
    let found = match paths.keyframes.range(time_ms..).next() {
        Some(entry) => entry,
        None => {
            // time_ms is past all keyframes, return last value
            return paths.last_value();
        }
    };
    let (&found_time, (found_val, found_easing)) = found;

    // If time_ms is at or before the first keyframe, return found value directly
    if let Some((&first_time, _)) = paths.keyframes.iter().next() {
        if time_ms <= first_time {
            return found_val.clone();
        }
    }

    // Find the previous keyframe
    let (prev_time, prev_val) = match paths.keyframes.range(..time_ms).next_back() {
        Some((&t, (val, _))) => (t, val.clone()),
        None => (0, paths.default_value.clone()),
    };

    // Interpolate
    let duration = (found_time - prev_time) as f32;
    let elapsed = (time_ms - prev_time) as f32;
    let progress = elapsed / duration;
    let eased_progress = apply_easing(progress, *found_easing);
    let options = morph_options
        .keyframes
        .get(&found_time)
        .map(|(value, _)| *value)
        .unwrap_or_default();
    interpolate(&prev_val, found_val, eased_progress, options)
}

fn interpolate_text_paths(
    source: &Vec<TextPath>,
    target: &Vec<TextPath>,
    t: f32,
    options: MorphOptions,
) -> Vec<TextPath> {
    let source_paths: Vec<_> = source.iter().map(|path| path.path.clone()).collect();
    let target_paths: Vec<_> = target.iter().map(|path| path.path.clone()).collect();
    let aligned_lists =
        align_path_lists_with_strategy(&source_paths, &target_paths, options.strategy);

    aligned_lists
        .into_iter()
        .enumerate()
        .map(|(index, (source_path, target_path))| TextPath {
            path: morph_paths_with_options(&source_path, &target_path, t as f64, options),
            color: if t < 0.5 {
                source
                    .get(index)
                    .map(|path| path.color.clone())
                    .unwrap_or_else(|| {
                        target
                            .get(index)
                            .map(|path| path.color.clone())
                            .unwrap_or_else(|| {
                                typst::visualize::Paint::Solid(typst::visualize::Color::BLACK)
                            })
                    })
            } else {
                target
                    .get(index)
                    .map(|path| path.color.clone())
                    .unwrap_or_else(|| {
                        source
                            .get(index)
                            .map(|path| path.color.clone())
                            .unwrap_or_else(|| {
                                typst::visualize::Paint::Solid(typst::visualize::Color::BLACK)
                            })
                    })
            },
        })
        .collect()
}

fn interpolate_vello_paths(
    source: &Vec<VelloPath>,
    target: &Vec<VelloPath>,
    t: f32,
    options: MorphOptions,
) -> Vec<VelloPath> {
    let source_paths: Vec<_> = source.iter().map(|path| path.path.clone()).collect();
    let target_paths: Vec<_> = target.iter().map(|path| path.path.clone()).collect();
    let aligned_lists =
        align_path_lists_with_strategy(&source_paths, &target_paths, options.strategy);

    aligned_lists
        .into_iter()
        .enumerate()
        .map(|(index, (source_path, target_path))| {
            let source_element = source.get(index);
            let target_element = target.get(index);

            VelloPath {
                path: morph_paths_with_options(&source_path, &target_path, t as f64, options),
                fill: match (
                    source_element.and_then(|element| element.fill),
                    target_element.and_then(|element| element.fill),
                ) {
                    (Some(c1), Some(c2)) => Some(lerp_color(c1, c2, t)),
                    (Some(color), None) => Some(if t < 0.5 {
                        color
                    } else {
                        vello::peniko::Color::TRANSPARENT
                    }),
                    (None, Some(color)) => Some(if t >= 0.5 {
                        color
                    } else {
                        vello::peniko::Color::TRANSPARENT
                    }),
                    (None, None) => None,
                },
                stroke: match (
                    source_element.and_then(|element| element.stroke),
                    target_element.and_then(|element| element.stroke),
                ) {
                    (Some((c1, w1)), Some((c2, w2))) => {
                        Some((lerp_color(c1, c2, t), w1 + (w2 - w1) * t))
                    }
                    (Some((color, width)), None) => Some((
                        if t < 0.5 {
                            color
                        } else {
                            vello::peniko::Color::TRANSPARENT
                        },
                        if t < 0.5 { width } else { 0.0 },
                    )),
                    (None, Some((color, width))) => Some((
                        if t >= 0.5 {
                            color
                        } else {
                            vello::peniko::Color::TRANSPARENT
                        },
                        if t >= 0.5 { width } else { 0.0 },
                    )),
                    (None, None) => None,
                },
            }
        })
        .collect()
}
