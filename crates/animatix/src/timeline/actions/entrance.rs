use super::registry::{ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{ModifierHost, Timeline, parse_timing_modifiers};

/// Wipes in vector targets by animating stroke progress and fill opacity together.
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
            if !super::ensure_vector_reveal_target(
                timeline,
                target,
                &action.verb,
                diagnostics,
                None,
            ) {
                continue;
            }

            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };

            if delay_ms > 0.0 && duration_ms == 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                super::ensure_guard_keyframe(&mut track.style.stroke_progress, guard_time, 1.0);
                super::ensure_guard_keyframe(&mut track.style.fill_opacity, guard_time, 1.0);
            }

            // Reveal pre-keyframe ("hidden by default") targets: wipe-in only
            // animates stroke_progress/fill_opacity, so lift the seeded
            // opacity 0 alongside the wipe.
            super::lift_hidden_by_default(track, t_start_ms, t_end_ms, easing);

            // Reveal the fill to its authored value, not blindly to 1.0: a
            // stroke-only Path (fill_opacity seeded 0) must stay unfilled
            // after the wipe.
            let authored_fill = track.style.fill_opacity.get(t_start_ms, 1.0);

            track
                .style
                .stroke_progress
                .ensure(1.0)
                .add_keyframe(t_start_ms, 0.0, Easing::Linear);
            track
                .style
                .fill_opacity
                .ensure(1.0)
                .add_keyframe(t_start_ms, 0.0, Easing::Linear);

            track.style.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
            track
                .style
                .fill_opacity
                .ensure(1.0)
                .add_keyframe(t_end_ms, authored_fill, easing);
        }
    }
}

/// Fades in the target by animating its overall opacity from 0 to 1.
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
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
                continue;
            }

            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };

            if delay_ms > 0.0 && duration_ms == 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                super::ensure_guard_keyframe(&mut track.style.opacity, guard_time, 1.0);
            }

            track.style.opacity.ensure(1.0).add_keyframe(t_start_ms, 0.0, Easing::Linear);

            track.style.opacity.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
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
            is_anonymous: false,
            label: label.to_string(),
            array_index: None,
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
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: label.to_string(),
            array_index: None,
            ty: "Text".to_string(),
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
            children: vec![],
            span: None,
        }
    }

    fn action_stmt(verb: &str, target: &str, duration_s: f64) -> Stmt {
        Stmt::Action(
            Action {
                verb: verb.to_string(),
                targets: vec![target.to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident(format!("{duration_s}s")),
                }],
                byte_span: None,
                target_index: vec![],
            },
            None,
        )
    }

    #[test]
    fn fade_in_animates_text_opacity() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "headline".to_string(),
                    array_index: None,
                    ty: "Text".to_string(),
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
                    children: vec![],
                    span: None,
                },
                Stmt::Action(
                    Action {
                        verb: "fade-in".to_string(),
                        targets: vec!["headline".to_string()],
                        args: vec![],
                        modifiers: vec![Modifier {
                            name: None,
                            value: Expr::Ident("1s".to_string()),
                        }],
                        byte_span: None,
                        target_index: vec![],
                    },
                    None,
                ),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("headline").expect("headline track");

        assert_eq!(track.style.opacity.get(0, 1.0), 0.0);
        assert!(track.style.opacity.get(500, 1.0) > 0.0);
        assert_eq!(track.style.opacity.get(1000, 1.0), 1.0);
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

        assert_eq!(track.style.stroke_progress.get(0, 1.0), 0.0);
        assert_eq!(track.style.fill_opacity.get(0, 1.0), 0.0);
        assert!(track.style.stroke_progress.get(500, 1.0) > 0.0);
        assert!(track.style.fill_opacity.get(500, 1.0) > 0.0);
        assert_eq!(track.style.stroke_progress.get(1000, 1.0), 1.0);
        assert_eq!(track.style.fill_opacity.get(1000, 1.0), 1.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn wipe_in_on_text_proceeds_without_diagnostics() {
        // Text targets are now allowed through vector reveal actions.
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
            !report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::UnsupportedActionTarget),
            "wipe-in on text should not report unsupported target"
        );
    }

    #[test]
    fn wipe_in_lifts_hidden_by_default_opacity() {
        // Regression: a pre-keyframe declaration is seeded `opacity = 0`;
        // wipe-in animates only stroke_progress/fill_opacity, which used to
        // leave the target fully transparent forever.
        let ast = vec![
            rect_decl("panel"),
            Stmt::Keyframe {
                time: Time::Seconds(0.5),
                body: vec![action_stmt("wipe-in", "panel", 0.7)],
                span: None,
            },
        ];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("panel").expect("panel track");

        assert_eq!(track.style.opacity.get(0, 1.0), 0.0);
        assert_eq!(track.style.opacity.get(400, 1.0), 0.0, "still hidden before the wipe");
        assert!(track.style.opacity.get(900, 1.0) > 0.0, "opacity must rise during the wipe");
        assert_eq!(track.style.opacity.get(1200, 1.0), 1.0, "fully visible after the wipe");
        assert!(!track.hidden_by_default, "flag must be consumed by the lift");
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn wipe_in_does_not_stomp_explicit_opacity() {
        // An explicit `opacity` on the declaration means the actor is not
        // hidden-by-default; the entrance must preserve the authored value.
        let mut declared = rect_decl("panel");
        if let Stmt::ActorDecl { props, .. } = &mut declared {
            props.push(Property {
                name: "opacity".to_string(),
                value: Expr::Num(0.3),
                value_span: None,
                trailing_comment: None,
            });
        }
        let ast = vec![
            declared,
            Stmt::Keyframe {
                time: Time::Seconds(0.5),
                body: vec![action_stmt("wipe-in", "panel", 0.7)],
                span: None,
            },
        ];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("panel").expect("panel track");

        assert!(!track.hidden_by_default);
        assert_eq!(track.style.opacity.get(1200, 1.0), 0.3, "explicit opacity is preserved");
    }
}
