pub mod actions;
pub mod track;
pub mod utils;

use actions::process_action;
pub use track::{AnimationTrack, Interpolate, PropertyTrack};
pub use utils::{parse_color, time_to_ms};

use crate::ast::{Expr, Stmt};
use crate::easing::*;
use crate::renderer::types::SdfInstance;
use std::collections::BTreeMap;

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
                    let mut stroke_width = track.stroke_width.last_value();
                    let mut stroke_color = track.stroke_color.last_value();
                    let mut stroke_progress = track.stroke_progress.last_value();
                    let mut fill_opacity = track.fill_opacity.last_value();

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
                            "stroke_width" => {
                                if let Expr::Num(w) = prop.value {
                                    stroke_width = w as f32;
                                }
                            }
                            "stroke_color" => {
                                stroke_color = parse_color(&prop.value);
                            }
                            "stroke_progress" => {
                                if let Expr::Num(w) = prop.value {
                                    stroke_progress = w as f32;
                                }
                            }
                            "fill_opacity" => {
                                if let Expr::Num(w) = prop.value {
                                    fill_opacity = w as f32;
                                }
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
                    track.stroke_width.add_keyframe(t_ms, stroke_width, easing);
                    track.stroke_color.add_keyframe(t_ms, stroke_color, easing);
                    track
                        .stroke_progress
                        .add_keyframe(t_ms, stroke_progress, easing);
                    track.fill_opacity.add_keyframe(t_ms, fill_opacity, easing);
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
                        "stroke_width" => {
                            let mut target_width = track.stroke_width.last_value();
                            if let Expr::Num(w) = value {
                                target_width = *w as f32;
                            }
                            if duration_ms > 0.0 {
                                let start_val = track.stroke_width.evaluate(t_start_ms);
                                track.stroke_width.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            }
                            track
                                .stroke_width
                                .add_keyframe(t_end_ms, target_width, easing);
                        }
                        "stroke_color" => {
                            let target_color = parse_color(value);
                            if duration_ms > 0.0 {
                                let start_val = track.stroke_color.evaluate(t_start_ms);
                                track.stroke_color.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            }
                            track
                                .stroke_color
                                .add_keyframe(t_end_ms, target_color, easing);
                        }
                        "stroke_progress" => {
                            let mut target_val = track.stroke_progress.last_value();
                            if let Expr::Num(w) = value {
                                target_val = *w as f32;
                            }
                            if duration_ms > 0.0 {
                                let start_val = track.stroke_progress.evaluate(t_start_ms);
                                track.stroke_progress.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            }
                            track
                                .stroke_progress
                                .add_keyframe(t_end_ms, target_val, easing);
                        }
                        "fill_opacity" => {
                            let mut target_val = track.fill_opacity.last_value();
                            if let Expr::Num(w) = value {
                                target_val = *w as f32;
                            }
                            if duration_ms > 0.0 {
                                let start_val = track.fill_opacity.evaluate(t_start_ms);
                                track.fill_opacity.add_keyframe(
                                    t_start_ms,
                                    start_val,
                                    Easing::Linear,
                                );
                            }
                            track
                                .fill_opacity
                                .add_keyframe(t_end_ms, target_val, easing);
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
                Stmt::Action(action) => {
                    process_action(action, time_ms, self);
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
            let stroke_width = track.stroke_width.evaluate(time_ms);
            let stroke_color = track.stroke_color.evaluate(time_ms);
            let stroke_progress = track.stroke_progress.evaluate(time_ms);
            let fill_opacity = track.fill_opacity.evaluate(time_ms);

            instances.push(SdfInstance {
                position,
                size,
                uv_rect: [0.0; 4],
                shape_params: [stroke_progress, fill_opacity, 0.0, 0.0],
                fill_color: color,
                stroke_color,
                stroke_width,
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
