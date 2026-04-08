use super::registry::{ActionParam, ActionSignature, BuiltinAction};
use crate::ast::{Action, Expr};
use crate::easing::Easing;
use crate::timeline::track::AnimationTrack;
use crate::timeline::Timeline;

pub struct FadeOut;

impl BuiltinAction for FadeOut {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "fade-out".to_string(),
            category: "Exit".to_string(),
            description: "Fades out the target by animating its overall opacity to 0.".to_string(),
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
                .or_insert_with(|| AnimationTrack::new(target.clone()));

            let start_opacity = track.opacity.evaluate(t_start_ms);
            if duration_ms > 0.0 {
                track
                    .opacity
                    .add_keyframe(t_start_ms, start_opacity, Easing::Linear);
            }
            track.opacity.add_keyframe(t_end_ms, 0.0, easing);
        }
    }
}
