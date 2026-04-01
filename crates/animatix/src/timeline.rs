use crate::ast::{Expr, Stmt, Time};
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

#[derive(Debug, Clone, Copy)]
pub struct ActorState {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub shape_type: u32,
    pub opacity: f32,
}

impl ActorState {
    pub fn new() -> Self {
        Self {
            position: [0.0, 0.0],
            size: [50.0, 50.0],
            color: [1.0, 1.0, 1.0, 1.0],
            shape_type: 0,
            opacity: 1.0,
        }
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            position: [
                self.position[0] + (other.position[0] - self.position[0]) * t,
                self.position[1] + (other.position[1] - self.position[1]) * t,
            ],
            size: [
                self.size[0] + (other.size[0] - self.size[0]) * t,
                self.size[1] + (other.size[1] - self.size[1]) * t,
            ],
            color: [
                self.color[0] + (other.color[0] - self.color[0]) * t,
                self.color[1] + (other.color[1] - self.color[1]) * t,
                self.color[2] + (other.color[2] - self.color[2]) * t,
                self.color[3] + (other.color[3] - self.color[3]) * t,
            ],
            shape_type: if t < 0.5 {
                self.shape_type
            } else {
                other.shape_type
            },
            opacity: self.opacity + (other.opacity - self.opacity) * t,
        }
    }
}

impl Default for ActorState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct AnimationTrack {
    pub label: String,
    pub keyframes: BTreeMap<u64, ActorState>,
}

impl AnimationTrack {
    pub fn new(label: String) -> Self {
        Self {
            label,
            keyframes: BTreeMap::new(),
        }
    }

    pub fn add_keyframe(&mut self, time_ms: f64, state: ActorState) {
        self.keyframes.insert(time_ms as u64, state);
    }

    pub fn evaluate(&self, time_ms: f64) -> ActorState {
        if self.keyframes.is_empty() {
            return ActorState::new();
        }

        let time_u64 = time_ms as u64;
        let mut prev_time = 0;
        let mut prev_state: Option<ActorState> = None;

        for (&t, state) in &self.keyframes {
            if t > time_u64 {
                if let Some(prev) = prev_state {
                    let duration = (t - prev_time) as f32;
                    let elapsed = (time_u64 - prev_time) as f32;
                    let progress = elapsed / duration;
                    return prev.interpolate(state, progress);
                } else {
                    return *state;
                }
            }
            prev_time = t;
            prev_state = Some(*state);
        }

        prev_state.unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct Timeline {
    pub tracks: BTreeMap<String, AnimationTrack>,
}

impl Timeline {
    pub fn new() -> Self {
        Self {
            tracks: BTreeMap::new(),
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
                _ => {}
            }
        }
        timeline
    }

    fn process_body(&mut self, time_ms: f64, body: &[Stmt]) {
        for stmt in body {
            if let Stmt::ActorDecl {
                label, ty, props, ..
            } = stmt
            {
                let track = self
                    .tracks
                    .entry(label.clone())
                    .or_insert_with(|| AnimationTrack::new(label.clone()));

                let mut state = ActorState::new();
                if let Some((_, last_state)) = track.keyframes.iter().next_back() {
                    state = *last_state;
                }

                state.shape_type = if ty == "Circle" { 1 } else { 0 };

                for prop in props {
                    match prop.name.as_str() {
                        "at" => {
                            if let Expr::Tuple(arr) = &prop.value {
                                if arr.len() == 2 {
                                    if let Expr::Num(x) = arr[0] {
                                        state.position[0] = x as f32;
                                    }
                                    if let Expr::Num(y) = arr[1] {
                                        state.position[1] = y as f32;
                                    }
                                }
                            }
                        }
                        "radius" => {
                            if let Expr::Num(r) = prop.value {
                                state.size = [r as f32, r as f32];
                            }
                        }
                        "size" => {
                            if let Expr::Tuple(arr) = &prop.value {
                                if arr.len() == 2 {
                                    if let Expr::Num(w) = arr[0] {
                                        state.size[0] = w as f32 / 2.0;
                                    }
                                    if let Expr::Num(h) = arr[1] {
                                        state.size[1] = h as f32 / 2.0;
                                    }
                                }
                            }
                        }
                        "color" => {
                            state.color = parse_color(&prop.value);
                        }
                        _ => {}
                    }
                }
                track.add_keyframe(time_ms, state);
            }
        }
    }

    pub fn evaluate(&self, time_s: f64) -> Vec<SdfInstance> {
        let time_ms = time_s * 1000.0;
        let mut instances = Vec::new();

        for track in self.tracks.values() {
            let state = track.evaluate(time_ms);

            instances.push(SdfInstance {
                position: state.position,
                size: state.size,
                uv_rect: [0.0; 4],
                shape_params: [0.0; 4],
                fill_color: state.color,
                stroke_color: [0.0; 4],
                stroke_width: 0.0,
                glow_radius: 0.0,
                opacity: state.opacity,
                shape_type: state.shape_type,
                target_position: state.position,
                target_size: state.size,
                target_shape_params: [0.0; 4],
                target_shape_type: state.shape_type,
                shape_blend: 0.0,
                _padding1: [0.0; 2],
                morph_params: [0.0; 4],
            });
        }

        instances
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}
