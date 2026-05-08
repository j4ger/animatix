use super::registry::{ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::track::TrackAccessor;
use crate::timeline::{ModifierHost, Timeline, parse_timing_modifiers};

pub struct WipeIn;

impl BuiltinAction for WipeIn {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "wipe-in".to_string(),
            category: "Entrance".to_string(),
            description: "Wipes in the target by animating stroke progress and fill opacity."
                .to_string(),
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
            if !super::ensure_vector_reveal_target(timeline, target, &action.verb, diagnostics) {
                continue;
            }

            let track = timeline
                .tracks
                .get_mut(target)
                .expect("validated target track");

            if delay_ms > 0.0 && duration_ms == 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_stroke = track.stroke_progress.get(guard_time, 1.0);
                let prior_fill = track.fill_opacity.get(guard_time, 1.0);
                if !track.stroke_progress.as_ref().map(|t| t.keyframes.contains_key(&guard_time)).unwrap_or(false) {
                    track
                        .stroke_progress
                        .ensure(1.0)
                        .add_keyframe(guard_time, prior_stroke, Easing::Linear);
                }
                if !track.fill_opacity.as_ref().map(|t| t.keyframes.contains_key(&guard_time)).unwrap_or(false) {
                    track
                        .fill_opacity
                        .ensure(1.0)
                        .add_keyframe(guard_time, prior_fill, Easing::Linear);
                }
            }

            track
                .stroke_progress
                .ensure(1.0)
                .add_keyframe(t_start_ms, 0.0, Easing::Linear);
            track
                .fill_opacity
                .ensure(1.0)
                .add_keyframe(t_start_ms, 0.0, Easing::Linear);

            track.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
            track.fill_opacity.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
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

            if delay_ms > 0.0 && duration_ms == 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_opacity = track.opacity.get(guard_time, 1.0);
                if !track.opacity.as_ref().map(|t| t.keyframes.contains_key(&guard_time)).unwrap_or(false) {
                    track
                        .opacity
                        .ensure(1.0)
                        .add_keyframe(guard_time, prior_opacity, Easing::Linear);
                }
            }

            track.opacity.ensure(1.0).add_keyframe(t_start_ms, 0.0, Easing::Linear);

            track.opacity.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Action, Expr, Modifier, Property, Stmt, Time};
    use crate::diagnostics::DiagnosticCode;

    fn rect_decl(label: &str) -> Stmt {
        Stmt::ActorDecl {
            is_pub: false,
            label: label.to_string(),
            ty: "Rect".to_string(),
            props: vec![
                Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(160.0), Expr::Num(80.0)]),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(240.0)]),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    }

    fn text_decl(label: &str) -> Stmt {
        Stmt::Text {
            label: Some(label.to_string()),
            props: vec![
                Property {
                    name: "text".to_string(),
                    value: Expr::Str("Hello".to_string()),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "font_size".to_string(),
                    value: Expr::Num(32.0),
                    value_span: None,
                trailing_comment: None,
                },
                Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(180.0)]),
                    value_span: None,
                trailing_comment: None,
                },
            ],
            modifiers: vec![],
            span: None,
        }
    }

    fn action_stmt(verb: &str, target: &str, duration_s: f64) -> Stmt {
        Stmt::Action(Action {
            verb: verb.to_string(),
            targets: vec![target.to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: Expr::Ident(format!("{duration_s}s")),
            }],
        }, None)
    }

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
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(32.0),
                            value_span: None,
                        trailing_comment: None,
                        },
                        Property {
                            name: "at".to_string(),
                            value: Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(180.0)]),
                            value_span: None,
                        trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    span: None,
                },
                Stmt::Action(Action {
                    verb: "fade-in".to_string(),
                    targets: vec!["headline".to_string()],
                    args: vec![],
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("1s".to_string()),
                    }],
                }, None),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report
            .output
            .tracks
            .get("headline")
            .expect("headline track");

        assert_eq!(track.opacity.get(0, 1.0), 0.0);
        assert!(track.opacity.get(500, 1.0) > 0.0);
        assert_eq!(track.opacity.get(1000, 1.0), 1.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn wipe_in_animates_stroke_and_fill_together() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![rect_decl("panel"), action_stmt("wipe-in", "panel", 1.0)],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("panel").expect("panel track");

        assert_eq!(track.stroke_progress.get(0, 1.0), 0.0);
        assert_eq!(track.fill_opacity.get(0, 1.0), 0.0);
        assert!(track.stroke_progress.get(500, 1.0) > 0.0);
        assert!(track.fill_opacity.get(500, 1.0) > 0.0);
        assert_eq!(track.stroke_progress.get(1000, 1.0), 1.0);
        assert_eq!(track.fill_opacity.get(1000, 1.0), 1.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn wipe_in_reports_unsupported_text_targets() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                text_decl("headline"),
                action_stmt("wipe-in", "headline", 1.0),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::UnsupportedActionTarget)
        );
    }
}
