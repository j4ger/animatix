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

pub struct WipeIn;

impl BuiltinAction for WipeIn {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "wipe-in".to_string(),
            category: "Entrance".to_string(),
            description: "Wipes in the target by animating stroke progress and fill opacity."
                .to_string(),
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
            if !super::ensure_vector_reveal_target(timeline, target, &action.verb, diagnostics) {
                continue;
            }

            let track = timeline
                .tracks
                .get_mut(target)
                .expect("validated target track");

            if delay_ms > 0.0 && duration_ms == 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_stroke = track.stroke_progress.evaluate(guard_time);
                let prior_fill = track.fill_opacity.evaluate(guard_time);
                if !track.stroke_progress.keyframes.contains_key(&guard_time) {
                    track
                        .stroke_progress
                        .add_keyframe(guard_time, prior_stroke, Easing::Linear);
                }
                if !track.fill_opacity.keyframes.contains_key(&guard_time) {
                    track
                        .fill_opacity
                        .add_keyframe(guard_time, prior_fill, Easing::Linear);
                }
            }

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
            description: "Fades in the target by animating its overall opacity.".to_string(),
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

            if delay_ms > 0.0 && duration_ms == 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_opacity = track.opacity.evaluate(guard_time);
                if !track.opacity.keyframes.contains_key(&guard_time) {
                    track
                        .opacity
                        .add_keyframe(guard_time, prior_opacity, Easing::Linear);
                }
            }

            track.opacity.add_keyframe(t_start_ms, 0.0, Easing::Linear);

            track.opacity.add_keyframe(t_end_ms, 1.0, easing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Modifier, Property, Stmt, Time};

    #[test]
    fn fade_in_animates_text_opacity() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                Stmt::Text {
                    label: Some("headline".to_string()),
                    props: vec![
                        Property {
                            name: "text".to_string(),
                            value: Expr::Str("Hello".to_string()),
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(32.0),
                        },
                        Property {
                            name: "at".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(180.0)]),
                        },
                    ],
                    modifiers: vec![],
                },
                Stmt::Action(Action {
                    verb: "fade-in".to_string(),
                    targets: vec!["headline".to_string()],
                    args: vec![],
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("1s".to_string()),
                    }],
                }),
            ],
        }];

        let report = Timeline::build_with_diagnostics(&ast);
        let track = report
            .output
            .tracks
            .get("headline")
            .expect("headline track");

        assert_eq!(track.opacity.evaluate(0), 0.0);
        assert!(track.opacity.evaluate(500) > 0.0);
        assert_eq!(track.opacity.evaluate(1000), 1.0);
        assert!(report.diagnostics.is_empty());
    }
}
