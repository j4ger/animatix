use crate::easing::{apply_easing, Easing};
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
        if t < 0.5 {
            *self
        } else {
            *other
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlacementMode {
    LayoutManaged,
    Manual,
}

impl Interpolate for PlacementMode {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 {
            *self
        } else {
            *other
        }
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
        if t < 0.5 {
            *self
        } else {
            *other
        }
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
        if t < 0.5 {
            *self
        } else {
            *other
        }
    }
}

impl Interpolate for Vec<crate::renderer::text::TextPath> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        use crate::timeline::morph::{align_path_lists, align_subpaths, morph_paths};

        let source_paths: Vec<_> = self.iter().map(|p| p.path.clone()).collect();
        let target_paths: Vec<_> = other.iter().map(|p| p.path.clone()).collect();

        let aligned_lists = align_path_lists(&source_paths, &target_paths);

        let mut result = Vec::with_capacity(aligned_lists.len());

        for (i, (s_path, t_path)) in aligned_lists.into_iter().enumerate() {
            let (aligned_s, aligned_t) = align_subpaths(&s_path, &t_path);
            let morphed_path = morph_paths(&aligned_s, &aligned_t, t as f64);

            let color = if t < 0.5 {
                self.get(i).map(|p| p.color.clone()).unwrap_or_else(|| {
                    other.get(i).map(|p| p.color.clone()).unwrap_or_else(|| {
                        typst::visualize::Paint::Solid(typst::visualize::Color::BLACK)
                    })
                })
            } else {
                other.get(i).map(|p| p.color.clone()).unwrap_or_else(|| {
                    self.get(i).map(|p| p.color.clone()).unwrap_or_else(|| {
                        typst::visualize::Paint::Solid(typst::visualize::Color::BLACK)
                    })
                })
            };

            result.push(crate::renderer::text::TextPath {
                path: morphed_path,
                color,
            });
        }

        result
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
        use crate::timeline::morph::{align_path_lists, align_subpaths, morph_paths};

        let source_paths: Vec<_> = self.iter().map(|p| p.path.clone()).collect();
        let target_paths: Vec<_> = other.iter().map(|p| p.path.clone()).collect();

        let aligned_lists = align_path_lists(&source_paths, &target_paths);

        let mut result = Vec::with_capacity(aligned_lists.len());

        for (i, (s_path, t_path)) in aligned_lists.into_iter().enumerate() {
            let (aligned_s, aligned_t) = align_subpaths(&s_path, &t_path);
            let morphed_path = morph_paths(&aligned_s, &aligned_t, t as f64);

            let s_elem = self.get(i);
            let t_elem = other.get(i);

            let fill = match (s_elem.and_then(|e| e.fill), t_elem.and_then(|e| e.fill)) {
                (Some(c1), Some(c2)) => Some(lerp_color(c1, c2, t)),
                (Some(c), None) => Some(if t < 0.5 {
                    c
                } else {
                    vello::peniko::Color::TRANSPARENT
                }),
                (None, Some(c)) => Some(if t >= 0.5 {
                    c
                } else {
                    vello::peniko::Color::TRANSPARENT
                }),
                (None, None) => None,
            };

            let stroke = match (s_elem.and_then(|e| e.stroke), t_elem.and_then(|e| e.stroke)) {
                (Some((c1, w1)), Some((c2, w2))) => {
                    Some((lerp_color(c1, c2, t), w1 + (w2 - w1) * t))
                }
                (Some((c, w)), None) => Some((
                    if t < 0.5 {
                        c
                    } else {
                        vello::peniko::Color::TRANSPARENT
                    },
                    if t < 0.5 { w } else { 0.0 },
                )),
                (None, Some((c, w))) => Some((
                    if t >= 0.5 {
                        c
                    } else {
                        vello::peniko::Color::TRANSPARENT
                    },
                    if t >= 0.5 { w } else { 0.0 },
                )),
                (None, None) => None,
            };

            result.push(crate::timeline::vello_path::VelloPath {
                path: morphed_path,
                fill,
                stroke,
            });
        }

        result
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
}

#[derive(Clone)]
pub struct AnimationTrack {
    pub label: String,
    pub position: PropertyTrack<[f32; 2]>,
    pub placement_mode: PropertyTrack<PlacementMode>,
    pub position_binding: PropertyTrack<PositionBinding>,
    pub size: PropertyTrack<[f32; 2]>,
    pub line_from: PropertyTrack<[f32; 2]>,
    pub line_to: PropertyTrack<[f32; 2]>,
    pub color: PropertyTrack<[f32; 4]>,
    pub shape_type: PropertyTrack<u32>,
    pub opacity: PropertyTrack<f32>,
    pub stroke_width: PropertyTrack<f32>,
    pub stroke_color: PropertyTrack<[f32; 4]>,
    pub stroke_progress: PropertyTrack<f32>,
    pub fill_opacity: PropertyTrack<f32>,
    pub text_paths: PropertyTrack<Vec<crate::renderer::text::TextPath>>,
    pub vector_paths: PropertyTrack<Vec<crate::timeline::vello_path::VelloPath>>,
    pub svg_paths: Vec<crate::timeline::VelloPath>,
}

impl AnimationTrack {
    pub fn new(label: String) -> Self {
        Self {
            label,
            position: PropertyTrack::new([0.0, 0.0]),
            placement_mode: PropertyTrack::new(PlacementMode::LayoutManaged),
            position_binding: PropertyTrack::new(PositionBinding::Absolute),
            size: PropertyTrack::new([50.0, 50.0]),
            line_from: PropertyTrack::new([-50.0, 0.0]),
            line_to: PropertyTrack::new([50.0, 0.0]),
            color: PropertyTrack::new([1.0, 1.0, 1.0, 1.0]),
            shape_type: PropertyTrack::new(0),
            opacity: PropertyTrack::new(1.0),
            stroke_width: PropertyTrack::new(2.0),
            stroke_color: PropertyTrack::new([1.0, 1.0, 1.0, 1.0]),
            stroke_progress: PropertyTrack::new(1.0),
            fill_opacity: PropertyTrack::new(1.0),
            text_paths: PropertyTrack::new(Vec::new()),
            vector_paths: PropertyTrack::new(Vec::new()),
            svg_paths: Vec::new(),
        }
    }
}
