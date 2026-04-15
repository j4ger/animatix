use super::registry::{ActionParam, ActionSignature, BuiltinAction};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::{parse_timing_modifiers, ModifierHost, Timeline};

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
                .or_insert_with(|| crate::timeline::track::AnimationTrack::new(target.clone()));

            track
                .fill_opacity
                .add_keyframe(t_start_ms, 0.0, Easing::Linear);

            track.fill_opacity.add_keyframe(t_end_ms, 1.0, easing);
        }
    }
}
