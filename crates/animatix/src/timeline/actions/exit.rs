use super::registry::{ActionParam, ActionSignature, BuiltinAction};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::{parse_timing_modifiers, ModifierHost, Timeline};

fn timing_modifier_params() -> Vec<ActionParam> {
    vec![
        ActionParam {
            name: "ease".to_string(),
            description: "Easing function for the animation".to_string(),
            type_info: "string".to_string(),
        },
        ActionParam {
            name: "duration-shorthand".to_string(),
            description: "Bare positional duration shorthand in brackets (e.g. [1s], [500ms])"
                .to_string(),
            type_info: "positional time literal".to_string(),
        },
        ActionParam {
            name: "delay".to_string(),
            description: "Delay before the action starts (e.g. [delay: 250ms])".to_string(),
            type_info: "time literal".to_string(),
        },
    ]
}

pub struct FadeOut;

impl BuiltinAction for FadeOut {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "fade-out".to_string(),
            category: "Exit".to_string(),
            description: "Fades out the target by animating its overall opacity to 0.".to_string(),
            params: vec![],
            modifiers: timing_modifier_params(),
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let parsed = parse_timing_modifiers(
            &action.modifiers,
            ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let duration_ms = parsed.duration_ms;
        let delay_ms = parsed.delay_ms;
        let easing = parsed.easing;

        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

        for target in &action.targets {
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics) {
                continue;
            }

            let track = timeline
                .tracks
                .get_mut(target)
                .expect("validated target track");

            let start_opacity = track.opacity.evaluate(t_start_ms);
            if duration_ms > 0.0 {
                track
                    .opacity
                    .add_keyframe(t_start_ms, start_opacity, Easing::Linear);
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_opacity = track.opacity.evaluate(guard_time);
                if !track.opacity.keyframes.contains_key(&guard_time) {
                    track
                        .opacity
                        .add_keyframe(guard_time, prior_opacity, Easing::Linear);
                }
            }
            track.opacity.add_keyframe(t_end_ms, 0.0, easing);
        }
    }
}
