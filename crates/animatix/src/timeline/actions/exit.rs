use super::registry::{ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::track::TrackAccessor;
use crate::timeline::{ModifierHost, Timeline, parse_timing_modifiers};

pub struct FadeOut;

impl BuiltinAction for FadeOut {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "fade-out".to_string(),
            category: "Exit".to_string(),
            description: "Fades out the target by animating its overall opacity to 0.".to_string(),
            params: vec![],
            modifiers: base_timing_params(),
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

            let start_opacity = track.opacity.get(t_start_ms, 1.0);
            if duration_ms > 0.0 {
                track
                    .opacity
                    .ensure(1.0)
                    .add_keyframe(t_start_ms, start_opacity, Easing::Linear);
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_opacity = track.opacity.get(guard_time, 1.0);
                if !track.opacity.as_ref().map(|t| t.keyframes.contains_key(&guard_time)).unwrap_or(false) {
                    track
                        .opacity
                        .ensure(1.0)
                        .add_keyframe(guard_time, prior_opacity, Easing::Linear);
                }
            }
            track.opacity.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Action, Expr, Modifier, Property, Stmt, Time};

    fn text_decl(label: &str) -> Stmt {
        Stmt::Text {
            label: Some(label.to_string()),
            props: vec![
                Property {
                    name: "text".to_string(),
                    value: Expr::Str("Bye".to_string()),
                    value_span: None,
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(32.0),
                    value_span: None,
                },
                Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(180.0)]),
                    value_span: None,
                },
            ],
            modifiers: vec![],
        }
    }

    #[test]
    fn fade_out_animates_opacity_to_zero() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                text_decl("headline"),
                Stmt::Action(Action {
                    verb: "fade-out".to_string(),
                    targets: vec!["headline".to_string()],
                    args: vec![],
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("1s".to_string()),
                    }],
                }),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report
            .output
            .tracks
            .get("headline")
            .expect("headline track");

        assert_eq!(track.opacity.get(0, 1.0), 1.0);
        assert!(track.opacity.get(500, 1.0) < 1.0);
        assert!(track.opacity.get(500, 1.0) > 0.0);
        assert_eq!(track.opacity.get(1000, 1.0), 0.0);
        assert!(report.diagnostics.is_empty());
    }
}
