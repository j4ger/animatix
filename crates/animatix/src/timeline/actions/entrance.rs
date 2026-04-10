use super::registry::{ActionParam, ActionSignature, BuiltinAction};
use crate::ast::{Action, Expr};
use crate::easing::Easing;
use crate::timeline::Timeline;

pub struct WipeIn;

impl BuiltinAction for WipeIn {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "wipe-in".to_string(),
            category: "Entrance".to_string(),
            description: "Wipes in the target by animating stroke progress and fill opacity."
                .to_string(),
            params: vec![],
            modifiers: vec![
                ActionParam {
                    name: "ease".to_string(),
                    description: "Easing function for the animation".to_string(),
                    type_info: "string".to_string(),
                },
                ActionParam {
                    name: "duration".to_string(),
                    description: "Duration of the animation (e.g. 1s, 500ms)".to_string(),
                    type_info: "string".to_string(),
                },
            ],
        }
    }

    fn execute(&self, action: &Action, time_ms: f64, timeline: &mut Timeline) {
        let mut duration_ms = 0.0;
        let mut easing = Easing::Linear;

        for modifier in &action.modifiers {
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

        for target in &action.targets {
            let track = timeline
                .tracks
                .entry(target.clone())
                .or_insert_with(|| crate::timeline::track::AnimationTrack::new(target.clone()));

            track
                .stroke_progress
                .add_keyframe(t_start_ms, 0.0, Easing::Linear);
            track
                .fill_opacity
                .add_keyframe(t_start_ms, 0.0, Easing::Linear);

            track.stroke_progress.add_keyframe(t_end_ms, 1.0, easing);
            track.fill_opacity.add_keyframe(t_end_ms, 1.0, easing);
        }
    }
}

pub struct FadeIn;

impl BuiltinAction for FadeIn {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "fade-in".to_string(),
            category: "Entrance".to_string(),
            description: "Fades in the target by animating fill opacity.".to_string(),
            params: vec![],
            modifiers: vec![
                ActionParam {
                    name: "ease".to_string(),
                    description: "Easing function for the animation".to_string(),
                    type_info: "string".to_string(),
                },
                ActionParam {
                    name: "duration".to_string(),
                    description: "Duration of the animation (e.g. 1s, 500ms)".to_string(),
                    type_info: "string".to_string(),
                },
            ],
        }
    }

    fn execute(&self, action: &Action, time_ms: f64, timeline: &mut Timeline) {
        let mut duration_ms = 0.0;
        let mut easing = Easing::Linear;

        for modifier in &action.modifiers {
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

        for target in &action.targets {
            let track = timeline
                .tracks
                .entry(target.clone())
                .or_insert_with(|| crate::timeline::track::AnimationTrack::new(target.clone()));

            track
                .fill_opacity
                .add_keyframe(t_start_ms, 0.0, Easing::Linear);

            track.fill_opacity.add_keyframe(t_end_ms, 1.0, easing);
        }
    }
}
