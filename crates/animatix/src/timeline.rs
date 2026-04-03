use crate::ast::{Expr, Stmt, Time};
use crate::easing::*;
use crate::renderer::types::SdfInstance;
use std::collections::BTreeMap;

pub fn parse_color(expr: &Expr) -> [f32; 4] {
    if let Expr::Ident(name) = expr {
        match name.as_str() {
            "red" => [1.0, 0.0, 0.0, 1.0],
            "green" => [0.0, 1.0, 0.0, 1.0],
            "blue" => [0.0, 0.0, 1.0, 1.0],
            "black" => [0.0, 0.0, 0.0, 1.0],
            "white" => [1.0, 1.0, 1.0, 1.0],
            _ => [0.8, 0.8, 0.8, 1.0],
        }
    } else {
        [0.8, 0.8, 0.8, 1.0]
    }
}

pub fn time_to_ms(time: &Time) -> f64 {
    match time {
        Time::Seconds(s) => *s * 1000.0,
        Time::Milliseconds(ms) => *ms as f64,
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct AnimationTrack {
    pub label: String,
    pub position: PropertyTrack<[f32; 2]>,
    pub size: PropertyTrack<[f32; 2]>,
    pub color: PropertyTrack<[f32; 4]>,
    pub shape_type: PropertyTrack<u32>,
    pub opacity: PropertyTrack<f32>,
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
        }
    }
}

#[derive(Debug, Clone)]
pub struct Timeline {
    pub tracks: BTreeMap<String, AnimationTrack>,
    pub background_color: PropertyTrack<[f32; 4]>,
}

impl Timeline {
    pub fn new() -> Self {
        let mut bg_track = PropertyTrack::new([0.0, 0.0, 0.0, 1.0]);
        bg_track.add_keyframe(0, [0.0, 0.0, 0.0, 1.0], Easing::Linear);
        Self {
            tracks: BTreeMap::new(),
            background_color: bg_track,
        }
    }

    pub fn build(ast: &[Stmt]) -> Self {
        let mut timeline = Self::new();
        let mut current_time_ms = 0.0;

        for stmt in ast {
            match stmt {
                Stmt::Keyframe { time, body } => {
                    current_time_ms = time_to_ms(time);
                    timeline.process_body(current_time_ms, body);
                }
                Stmt::RelativeKeyframe { offset, body } => {
                    current_time_ms += time_to_ms(offset);
                    timeline.process_body(current_time_ms, body);
                }
                Stmt::ActorDecl { .. } | Stmt::Assignment { .. } => {
                    timeline.process_body(current_time_ms, &[stmt.clone()]);
                }
                _ => {}
            }
        }
        timeline
    }

    fn process_body(&mut self, time_ms: f64, body: &[Stmt]) {
        for stmt in body {
            match stmt {
                Stmt::ActorDecl {
                    label,
                    ty,
                    props,
                    modifiers,
                    ..
                } => {
                    let track = self
                        .tracks
                        .entry(label.clone())
                        .or_insert_with(|| AnimationTrack::new(label.clone()));

                    let mut position = track.position.last_value();
                    let mut size = track.size.last_value();
                    let mut color = track.color.last_value();
                    let shape_type = if ty == "Circle" { 1 } else { 0 };
                    let opacity = track.opacity.last_value();

                    let mut easing = Easing::Linear;
                    for modifier in modifiers {
                        if modifier.name.as_deref() == Some("ease") || modifier.name.is_none() {
                            if let Expr::Ident(val) = &modifier.value {
                                match val.as_str() {
                                    "ease-in" => easing = Easing::EaseIn,
                                    "ease-out" => easing = Easing::EaseOut,
                                    "ease-in-out" => easing = Easing::EaseInOut,
                                    "bounce" => easing = Easing::Bounce,
                                    "linear" => easing = Easing::Linear,
                                    _ => {}
                                }
                            }
                        }
                    }

                    for prop in props {
                        match prop.name.as_str() {
                            "at" => {
                                if let Expr::Tuple(arr) = &prop.value {
                                    if arr.len() == 2 {
                                        if let Expr::Num(x) = arr[0] {
                                            position[0] = x as f32;
                                        }
                                        if let Expr::Num(y) = arr[1] {
                                            position[1] = y as f32;
                                        }
                                    }
                                }
                            }
                            "radius" => {
                                if let Expr::Num(r) = prop.value {
                                    size = [r as f32, r as f32];
                                }
                            }
                            "size" => {
                                if let Expr::Tuple(arr) = &prop.value {
                                    if arr.len() == 2 {
                                        if let Expr::Num(w) = arr[0] {
                                            size[0] = w as f32 / 2.0;
                                        }
                                        if let Expr::Num(h) = arr[1] {
                                            size[1] = h as f32 / 2.0;
                                        }
                                    }
                                }
                            }
                            "color" => {
                                color = parse_color(&prop.value);
                            }
                            _ => {}
                        }
                    }

                    let t_ms = time_ms as u64;
                    track.position.add_keyframe(t_ms, position, easing);
                    track.size.add_keyframe(t_ms, size, easing);
                    track.color.add_keyframe(t_ms, color, easing);
                    track.shape_type.add_keyframe(t_ms, shape_type, easing);
                    track.opacity.add_keyframe(t_ms, opacity, easing);
                }
                Stmt::Assignment {
                    target,
                    property,
                    value,
                    modifiers,
                } => {
                    let mut duration_ms = 0.0;
                    let mut easing = Easing::Linear;

                    for modifier in modifiers {
                        if modifier.name.as_deref() == Some("ease") {
                            if let Expr::Ident(val) = &modifier.value {
                                match val.as_str() {
                                    "ease-in" => easing = Easing::EaseIn,
                                    "ease-out" => easing = Easing::EaseOut,
                                    "ease-in-out" => easing = Easing::EaseInOut,
                                    "bounce" => easing = Easing::Bounce,
                                    "linear" => easing = Easing::Linear,
                                    _ => {}
                                }
                            }
                        } else if modifier.name.is_none() {
                            // Try to parse duration (e.g., 2s, 500ms)
                            if let Expr::Ident(val) = &modifier.value {
                                if val.ends_with("ms") {
                                    if let Ok(ms) = val.trim_end_matches("ms").parse::<f64>() {
                                        duration_ms = ms;
                                    }
                                } else if val.ends_with('s') {
                                    if let Ok(s) = val.trim_end_matches('s').parse::<f64>() {
                                        duration_ms = s * 1000.0;
                                    }
                                }
                            }
                        }
                    }

                    let t_start_ms = time_ms as u64;
                    let t_end_ms = (time_ms + duration_ms) as u64;

                    if target == "scene" {
                        if property == "background_color" {
                            let target_color = parse_color(value);
                            if duration_ms > 0.0 {
                                let start_val = self.background_color.evaluate(t_start_ms);
                                self.background_color.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            }
                            self.background_color
                                .add_keyframe(t_end_ms, target_color, easing);
                        }
                        continue;
                    }

                    let track = self
                        .tracks
                        .entry(target.clone())
                        .or_insert_with(|| AnimationTrack::new(target.clone()));

                    match property.as_str() {
                        "color" => {
                            let target_color = parse_color(value);
                            if duration_ms > 0.0 {
                                let start_val = track.color.evaluate(t_start_ms);
                                track
                                    .color
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            }
                            track.color.add_keyframe(t_end_ms, target_color, easing);
                        }
                        "size" => {
                            let mut target_size = track.size.last_value();
                            if let Expr::Tuple(arr) = value {
                                if arr.len() == 2 {
                                    if let Expr::Num(w) = arr[0] {
                                        target_size[0] = w as f32 / 2.0;
                                    }
                                    if let Expr::Num(h) = arr[1] {
                                        target_size[1] = h as f32 / 2.0;
                                    }
                                }
                            } else if let Expr::Num(r) = value {
                                target_size = [*r as f32, *r as f32];
                            }
                            if duration_ms > 0.0 {
                                let start_val = track.size.evaluate(t_start_ms);
                                track
                                    .size
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            }
                            track.size.add_keyframe(t_end_ms, target_size, easing);
                        }
                        "position" | "at" => {
                            let mut target_pos = track.position.last_value();
                            if let Expr::Tuple(arr) = value {
                                if arr.len() == 2 {
                                    if let Expr::Num(x) = arr[0] {
                                        target_pos[0] = x as f32;
                                    }
                                    if let Expr::Num(y) = arr[1] {
                                        target_pos[1] = y as f32;
                                    }
                                }
                            }
                            if duration_ms > 0.0 {
                                let start_val = track.position.evaluate(t_start_ms);
                                track
                                    .position
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            }
                            track.position.add_keyframe(t_end_ms, target_pos, easing);
                        }
                        "radius" => {
                            let mut target_size = track.size.last_value();
                            if let Expr::Num(r) = value {
                                target_size = [*r as f32, *r as f32];
                            }
                            if duration_ms > 0.0 {
                                let start_val = track.size.evaluate(t_start_ms);
                                track
                                    .size
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            }
                            track.size.add_keyframe(t_end_ms, target_size, easing);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }

    pub fn evaluate(&self, time_s: f64) -> (Vec<SdfInstance>, [f32; 4]) {
        let time_ms = (time_s * 1000.0) as u64;
        let mut instances = Vec::new();
        let bg_color = self.background_color.evaluate(time_ms);

        for track in self.tracks.values() {
            let position = track.position.evaluate(time_ms);
            let size = track.size.evaluate(time_ms);
            let color = track.color.evaluate(time_ms);
            let shape_type = track.shape_type.evaluate(time_ms);
            let opacity = track.opacity.evaluate(time_ms);

            instances.push(SdfInstance {
                position,
                size,
                uv_rect: [0.0; 4],
                shape_params: [0.0; 4],
                fill_color: color,
                stroke_color: [0.0; 4],
                stroke_width: 0.0,
                glow_radius: 0.0,
                opacity,
                shape_type,
                target_position: position,
                target_size: size,
                target_shape_params: [0.0; 4],
                target_shape_type: shape_type,
                shape_blend: 0.0,
                _padding1: [0.0; 2],
                morph_params: [0.0; 4],
            });
        }

        (instances, bg_color)
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}
