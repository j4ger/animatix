use crate::easing::{Easing, apply_easing};
use crate::timeline::morph::{
    MorphOptions, align_path_lists_with_strategy, morph_paths_with_options,
};
use std::collections::BTreeMap;

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
}

impl Interpolate for PositionBinding {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

impl Interpolate for MorphOptions {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

impl Interpolate for Vec<crate::renderer::text::TextPath> {
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

impl Interpolate for Vec<crate::timeline::vello_path::VelloPath> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        interpolate_vello_paths(self, other, t, MorphOptions::default())
    }
}

impl Interpolate for Option<crate::timeline::image::SceneImage> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { self.clone() } else { other.clone() }
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

        let mut prev_time = 0;
        let mut prev_val = self.default_value.clone();

        // Initialize prev_val with the first keyframe if it exists
        if let Some((&t, (val, _))) = self.keyframes.iter().next() {
            if time_ms <= t {
                return val.clone(); // Before first keyframe
            }
        }

        for (&t, (val, easing)) in &self.keyframes {
            if t > time_ms {
                let duration = (t - prev_time) as f32;
                let elapsed = (time_ms - prev_time) as f32;
                let progress = elapsed / duration;
                let eased_progress = apply_easing(progress, *easing);
                return prev_val.interpolate(&val, eased_progress);
            }
            prev_time = t;
            prev_val = val.clone();
        }

        prev_val
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
    pub position: PropertyTrack<[f32; 2]>,
    pub motion_offset: PropertyTrack<[f32; 2]>,
    pub rotation: PropertyTrack<f32>,
    pub scale: PropertyTrack<f32>,
    pub placement_mode: PropertyTrack<PlacementMode>,
    pub position_binding: PropertyTrack<PositionBinding>,
    pub size: PropertyTrack<[f32; 2]>,
    pub line_from: PropertyTrack<[f32; 2]>,
    pub line_to: PropertyTrack<[f32; 2]>,
    pub arc_angles: PropertyTrack<[f32; 2]>,
    pub color: PropertyTrack<[f32; 4]>,
    pub shape_type: PropertyTrack<u32>,
    pub opacity: PropertyTrack<f32>,
    pub stroke_width: PropertyTrack<f32>,
    pub stroke_color: PropertyTrack<[f32; 4]>,
    pub stroke_progress: PropertyTrack<f32>,
    pub fill_opacity: PropertyTrack<f32>,
    pub morph_options: PropertyTrack<MorphOptions>,
    pub text_paths: PropertyTrack<Vec<crate::renderer::text::TextPath>>,
    pub vector_paths: PropertyTrack<Vec<crate::timeline::vello_path::VelloPath>>,
    pub svg_paths: Vec<crate::timeline::VelloPath>,
    pub image: PropertyTrack<Option<crate::timeline::image::SceneImage>>,
}

impl AnimationTrack {
    pub fn new(label: String) -> Self {
        Self {
            label,
            position: PropertyTrack::new([0.0, 0.0]),
            motion_offset: PropertyTrack::new([0.0, 0.0]),
            rotation: PropertyTrack::new(0.0),
            scale: PropertyTrack::new(1.0),
            placement_mode: PropertyTrack::new(PlacementMode::LayoutManaged),
            position_binding: PropertyTrack::new(PositionBinding::Absolute),
            size: PropertyTrack::new([50.0, 50.0]),
            line_from: PropertyTrack::new([-50.0, 0.0]),
            line_to: PropertyTrack::new([50.0, 0.0]),
            arc_angles: PropertyTrack::new([0.0, std::f32::consts::PI]),
            color: PropertyTrack::new([1.0, 1.0, 1.0, 1.0]),
            shape_type: PropertyTrack::new(0),
            opacity: PropertyTrack::new(1.0),
            stroke_width: PropertyTrack::new(2.0),
            stroke_color: PropertyTrack::new([1.0, 1.0, 1.0, 1.0]),
            stroke_progress: PropertyTrack::new(1.0),
            fill_opacity: PropertyTrack::new(1.0),
            morph_options: PropertyTrack::new(MorphOptions::default()),
            text_paths: PropertyTrack::new(Vec::new()),
            vector_paths: PropertyTrack::new(Vec::new()),
            svg_paths: Vec::new(),
            image: PropertyTrack::new(None),
        }
    }

    pub fn evaluate_text_paths(&self, time_ms: u64) -> Vec<crate::renderer::text::TextPath> {
        evaluate_paths_with_options(
            &self.text_paths,
            &self.morph_options,
            time_ms,
            interpolate_text_paths,
        )
    }

    pub fn evaluate_vector_paths(
        &self,
        time_ms: u64,
    ) -> Vec<crate::timeline::vello_path::VelloPath> {
        evaluate_paths_with_options(
            &self.vector_paths,
            &self.morph_options,
            time_ms,
            interpolate_vello_paths,
        )
    }

    pub fn max_keyframe_time(&self) -> Option<u64> {
        [
            self.position.last_keyframe_time(),
            self.motion_offset.last_keyframe_time(),
            self.rotation.last_keyframe_time(),
            self.scale.last_keyframe_time(),
            self.placement_mode.last_keyframe_time(),
            self.position_binding.last_keyframe_time(),
            self.size.last_keyframe_time(),
            self.line_from.last_keyframe_time(),
            self.line_to.last_keyframe_time(),
            self.arc_angles.last_keyframe_time(),
            self.color.last_keyframe_time(),
            self.shape_type.last_keyframe_time(),
            self.opacity.last_keyframe_time(),
            self.stroke_width.last_keyframe_time(),
            self.stroke_color.last_keyframe_time(),
            self.stroke_progress.last_keyframe_time(),
            self.fill_opacity.last_keyframe_time(),
            self.morph_options.last_keyframe_time(),
            self.text_paths.last_keyframe_time(),
            self.vector_paths.last_keyframe_time(),
            self.image.last_keyframe_time(),
        ]
        .into_iter()
        .flatten()
        .max()
    }
}

fn evaluate_paths_with_options<T: Clone>(
    paths: &PropertyTrack<T>,
    morph_options: &PropertyTrack<MorphOptions>,
    time_ms: u64,
    interpolate: fn(&T, &T, f32, MorphOptions) -> T,
) -> T {
    if paths.keyframes.is_empty() {
        return paths.default_value.clone();
    }

    if let Some((&first_time, (first_value, _))) = paths.keyframes.iter().next() {
        if time_ms <= first_time {
            return first_value.clone();
        }
    }

    let mut prev_time = 0;
    let mut prev_val = paths.default_value.clone();

    for (&next_time, (next_val, easing)) in &paths.keyframes {
        if next_time > time_ms {
            let duration = (next_time - prev_time) as f32;
            let elapsed = (time_ms - prev_time) as f32;
            let progress = elapsed / duration;
            let eased_progress = apply_easing(progress, *easing);
            let options = morph_options
                .keyframes
                .get(&next_time)
                .map(|(value, _)| *value)
                .unwrap_or_default();
            return interpolate(&prev_val, next_val, eased_progress, options);
        }
        prev_time = next_time;
        prev_val = next_val.clone();
    }

    prev_val
}

fn interpolate_text_paths(
    source: &Vec<crate::renderer::text::TextPath>,
    target: &Vec<crate::renderer::text::TextPath>,
    t: f32,
    options: MorphOptions,
) -> Vec<crate::renderer::text::TextPath> {
    let source_paths: Vec<_> = source.iter().map(|path| path.path.clone()).collect();
    let target_paths: Vec<_> = target.iter().map(|path| path.path.clone()).collect();
    let aligned_lists =
        align_path_lists_with_strategy(&source_paths, &target_paths, options.strategy);

    aligned_lists
        .into_iter()
        .enumerate()
        .map(
            |(index, (source_path, target_path))| crate::renderer::text::TextPath {
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
            },
        )
        .collect()
}

fn interpolate_vello_paths(
    source: &Vec<crate::timeline::vello_path::VelloPath>,
    target: &Vec<crate::timeline::vello_path::VelloPath>,
    t: f32,
    options: MorphOptions,
) -> Vec<crate::timeline::vello_path::VelloPath> {
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

            crate::timeline::vello_path::VelloPath {
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
