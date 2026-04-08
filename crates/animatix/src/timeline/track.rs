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

#[derive(Clone)]
pub struct PropertyTrack<T> {
    pub keyframes: BTreeMap<u64, (T, Easing)>,
    pub default_value: T,
}

impl<T: Interpolate + Copy + Clone> PropertyTrack<T> {
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
            return self.default_value;
        }

        let mut prev_time = 0;
        let mut prev_val = self.default_value;

        // Initialize prev_val with the first keyframe if it exists
        if let Some((&t, &(val, _))) = self.keyframes.iter().next() {
            if time_ms <= t {
                return val; // Before first keyframe
            }
        }

        for (&t, &(val, easing)) in &self.keyframes {
            if t > time_ms {
                let duration = (t - prev_time) as f32;
                let elapsed = (time_ms - prev_time) as f32;
                let progress = elapsed / duration;
                let eased_progress = apply_easing(progress, easing);
                return prev_val.interpolate(&val, eased_progress);
            }
            prev_time = t;
            prev_val = val;
        }

        prev_val
    }

    pub fn last_value(&self) -> T {
        self.keyframes
            .iter()
            .next_back()
            .map(|(_, &(val, _))| val)
            .unwrap_or(self.default_value)
    }
}

#[derive(Clone)]
pub struct AnimationTrack {
    pub label: String,
    pub position: PropertyTrack<[f32; 2]>,
    pub size: PropertyTrack<[f32; 2]>,
    pub color: PropertyTrack<[f32; 4]>,
    pub shape_type: PropertyTrack<u32>,
    pub opacity: PropertyTrack<f32>,
    pub stroke_width: PropertyTrack<f32>,
    pub stroke_color: PropertyTrack<[f32; 4]>,
    pub stroke_progress: PropertyTrack<f32>,
    pub fill_opacity: PropertyTrack<f32>,
    pub text_paths: Vec<crate::renderer::text::TextPath>,
    pub svg_paths: Vec<crate::timeline::VelloPath>,
}

impl AnimationTrack {
    pub fn new(label: String) -> Self {
        Self {
            label,
            position: PropertyTrack::new([0.0, 0.0]),
            size: PropertyTrack::new([50.0, 50.0]),
            color: PropertyTrack::new([1.0, 1.0, 1.0, 1.0]),
            shape_type: PropertyTrack::new(0),
            opacity: PropertyTrack::new(1.0),
            stroke_width: PropertyTrack::new(2.0),
            stroke_color: PropertyTrack::new([1.0, 1.0, 1.0, 1.0]),
            stroke_progress: PropertyTrack::new(1.0),
            fill_opacity: PropertyTrack::new(1.0),
            text_paths: Vec::new(),
            svg_paths: Vec::new(),
        }
    }
}
