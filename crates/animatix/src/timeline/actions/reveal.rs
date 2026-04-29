use super::registry::{ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::track::TrackAccessor;
use crate::timeline::{ModifierHost, Timeline, parse_timing_modifiers};

pub struct DrawIn;

impl BuiltinAction for DrawIn {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "draw-in".to_string(),
            category: "Reveal".to_string(),
            description:
                "Draws in vector targets by animating stroke progress first, then revealing fill at the end."
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

            if duration_ms > 0.0 && t_end_ms > t_start_ms {
                track
                    .fill_opacity
                    .ensure(1.0)
                    .add_keyframe(t_end_ms.saturating_sub(1), 0.0, Easing::Linear);
            }

            track.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
            track
                .fill_opacity
                .ensure(1.0)
                .add_keyframe(t_end_ms, 1.0, easing);
        }
    }
}

pub struct WipeOut;

impl BuiltinAction for WipeOut {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "wipe-out".to_string(),
            category: "Exit".to_string(),
            description:
                "Wipes out vector targets by animating stroke progress and fill opacity down together."
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

            let start_stroke = track.stroke_progress.get(t_start_ms, 1.0);
            let start_fill = track.fill_opacity.get(t_start_ms, 1.0);

            if duration_ms > 0.0 {
                track
                    .stroke_progress
                    .ensure(1.0)
                    .add_keyframe(t_start_ms, start_stroke, Easing::Linear);
                track
                    .fill_opacity
                    .ensure(1.0)
                    .add_keyframe(t_start_ms, start_fill, Easing::Linear);
            } else if delay_ms > 0.0 && t_start_ms > 0 {
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

            track.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
            track.fill_opacity.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
        }
    }
}

pub struct RevealOut;

impl BuiltinAction for RevealOut {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "reveal-out".to_string(),
            category: "Exit".to_string(),
            description:
                "Exits vector targets by hiding fill at the start, then erasing stroke progress over time."
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

            let start_stroke = track.stroke_progress.get(t_start_ms, 1.0);

            if duration_ms > 0.0 {
                track
                    .stroke_progress
                    .ensure(1.0)
                    .add_keyframe(t_start_ms, start_stroke, Easing::Linear);
            } else if delay_ms > 0.0 && t_start_ms > 0 {
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
                .fill_opacity
                .ensure(1.0)
                .add_keyframe(t_start_ms, 0.0, Easing::Linear);
            track.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
        }
    }
}

pub struct DrawOut;

impl BuiltinAction for DrawOut {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "draw-out".to_string(),
            category: "Exit".to_string(),
            description:
                "Exits vector targets by erasing stroke progress over time while keeping fill until the end."
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

            let start_stroke = track.stroke_progress.get(t_start_ms, 1.0);
            let start_fill = track.fill_opacity.get(t_start_ms, 1.0);

            if duration_ms > 0.0 {
                track
                    .stroke_progress
                    .ensure(1.0)
                    .add_keyframe(t_start_ms, start_stroke, Easing::Linear);
                track
                    .fill_opacity
                    .ensure(1.0)
                    .add_keyframe(t_start_ms, start_fill, Easing::Linear);
                track.fill_opacity.ensure(1.0).add_keyframe(
                    t_end_ms.saturating_sub(1),
                    start_fill,
                    Easing::Linear,
                );
            } else if delay_ms > 0.0 && t_start_ms > 0 {
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

            track.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
            track
                .fill_opacity
                .ensure(1.0)
                .add_keyframe(t_end_ms, 0.0, Easing::Linear);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Modifier, Property, Stmt, Time};
    use crate::diagnostics::DiagnosticCode;

    fn circle_decl(label: &str) -> Stmt {
        Stmt::ActorDecl {
            is_pub: false,
            label: label.to_string(),
            ty: "Circle".to_string(),
            props: vec![
                Property {
                    name: "radius".to_string(),
                    value: crate::ast::Expr::Num(40.0),
                },
                Property {
                    name: "at".to_string(),
                    value: crate::ast::Expr::Tuple(vec![
                        crate::ast::Expr::Num(320.0),
                        crate::ast::Expr::Num(240.0),
                    ]),
                },
            ],
            modifiers: vec![],
            children: vec![],
        }
    }

    fn image_decl(label: &str) -> Stmt {
        Stmt::Image {
            label: Some(label.to_string()),
            url: "../../examples/checker.ppm".to_string(),
            at: Some(Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(240.0)])),
            anchor: None,
            offset: None,
            size: Some((120.0, 120.0)),
        }
    }

    fn text_decl(label: &str) -> Stmt {
        Stmt::Text {
            label: Some(label.to_string()),
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
        }
    }

    fn action_stmt(verb: &str, target: &str, duration_s: f64) -> Stmt {
        Stmt::Action(Action {
            verb: verb.to_string(),
            targets: vec![target.to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: crate::ast::Expr::Ident(format!("{duration_s}s")),
            }],
        })
    }

    #[test]
    fn draw_in_sets_stroke_progress_and_delays_fill_until_end() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![circle_decl("shape"), action_stmt("draw-in", "shape", 1.0)],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("shape").expect("shape track");

        assert_eq!(track.stroke_progress.get(0, 1.0), 0.0);
        assert_eq!(track.stroke_progress.get(1000, 1.0), 1.0);
        assert_eq!(track.fill_opacity.get(500, 1.0), 0.0);
        assert_eq!(track.fill_opacity.get(1000, 1.0), 1.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn wipe_out_reports_unsupported_image_targets() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![image_decl("photo"), action_stmt("wipe-out", "photo", 1.0)],
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

    #[test]
    fn reveal_out_hides_fill_then_erases_stroke() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                circle_decl("shape"),
                action_stmt("reveal-out", "shape", 1.0),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("shape").expect("shape track");

        assert_eq!(track.fill_opacity.get(0, 1.0), 0.0);
        assert_eq!(track.stroke_progress.get(0, 1.0), 1.0);
        assert!(track.stroke_progress.get(500, 1.0) > 0.0);
        assert!(track.stroke_progress.get(500, 1.0) < 1.0);
        assert_eq!(track.stroke_progress.get(1000, 1.0), 0.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn reveal_out_preserves_prior_state_for_delayed_instant_change() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                circle_decl("shape"),
                Stmt::Action(Action {
                    verb: "reveal-out".to_string(),
                    targets: vec!["shape".to_string()],
                    args: vec![],
                    modifiers: vec![
                        Modifier {
                            name: Some("delay".to_string()),
                            value: Expr::Ident("250ms".to_string()),
                        },
                        Modifier {
                            name: None,
                            value: Expr::Ident("0s".to_string()),
                        },
                    ],
                }),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("shape").expect("shape track");

        assert_eq!(track.fill_opacity.get(249, 1.0), 1.0);
        assert_eq!(track.stroke_progress.get(249, 1.0), 1.0);
        assert_eq!(track.fill_opacity.get(250, 1.0), 0.0);
        assert_eq!(track.stroke_progress.get(250, 1.0), 0.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn reveal_out_reports_unsupported_text_targets() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                text_decl("headline"),
                action_stmt("reveal-out", "headline", 1.0),
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

    #[test]
    fn draw_out_keeps_fill_until_the_end() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![circle_decl("shape"), action_stmt("draw-out", "shape", 1.0)],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("shape").expect("shape track");

        assert_eq!(track.stroke_progress.get(0, 1.0), 1.0);
        assert_eq!(track.fill_opacity.get(0, 1.0), 1.0);
        assert!(track.stroke_progress.get(500, 1.0) > 0.0);
        assert!(track.stroke_progress.get(500, 1.0) < 1.0);
        assert_eq!(track.fill_opacity.get(500, 1.0), 1.0);
        assert_eq!(track.stroke_progress.get(1000, 1.0), 0.0);
        assert_eq!(track.fill_opacity.get(1000, 1.0), 0.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn draw_out_reports_unsupported_text_targets() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                text_decl("headline"),
                action_stmt("draw-out", "headline", 1.0),
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
