use super::registry::{ActionParam, ActionSignature, BuiltinAction};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::track::AnimationTrack;
use crate::timeline::{parse_timing_modifiers, ModifierHost, Timeline};

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
                    name: "duration-shorthand".to_string(),
                    description:
                        "Bare positional duration shorthand in brackets (e.g. [1s], [500ms])"
                            .to_string(),
                    type_info: "positional time literal".to_string(),
                },
            ],
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
        let easing = parsed.easing;

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
