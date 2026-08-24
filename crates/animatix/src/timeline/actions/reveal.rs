use super::registry::{ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::actor_kind::ActorKindId;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{DEFAULT_WHITE, ModifierHost, Timeline, parse_timing_modifiers};

/// Ensure a filled shape without a visible authored stroke has an outline for
/// draw/reveal actions. The outline uses the actor's fill color so the default
/// declaration stays clean while the entrance effect remains visible.
fn ensure_reveal_stroke(track: &mut crate::timeline::AnimationTrack, time_ms: u64) {
    let current_width = track
        .style
        .stroke_width
        .get(time_ms, crate::timeline::default_stroke_width(track.kind));
    if current_width > 0.0 {
        return;
    }
    let color = track.style.color.get(time_ms, DEFAULT_WHITE);
    track.style.stroke_width.ensure(2.0).add_keyframe(time_ms, 2.0, Easing::Linear);
    track
        .style
        .stroke_color
        .ensure(DEFAULT_WHITE)
        .add_keyframe(time_ms, color, Easing::Linear);
}

/// Returns true when a primitive emits text glyph paths. The `ActorKindId`
/// fallback covers hand-built test tracks without an actor type name.
fn is_text_like(timeline: &Timeline, track: &crate::timeline::AnimationTrack) -> bool {
    if let Some(primitive) =
        track.actor_type.as_deref().and_then(|ty| timeline.primitive_registry.find(ty))
    {
        return primitive.capabilities().text_paths;
    }
    matches!(track.kind, ActorKindId::Text | ActorKindId::Code | ActorKindId::Typst)
}

/// Draws in vector targets by animating stroke progress first, then revealing fill.
pub struct DrawIn;

impl BuiltinAction for DrawIn {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "draw-in".to_string(),
            category: "Entrance".to_string(),
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
            if !super::ensure_vector_reveal_target(
                timeline,
                target,
                &action.verb,
                diagnostics,
                None,
            ) {
                continue;
            }

            let is_text =
                timeline.tracks.get(target).is_some_and(|track| is_text_like(timeline, track));

            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };

            // Reveal pre-keyframe ("hidden by default") targets: draw-in
            // animates stroke_progress/fill_opacity/char_progress but never
            // opacity, so lift the seeded opacity 0 alongside the draw.
            super::lift_hidden_by_default(track, t_start_ms, t_end_ms, easing);

            if is_text {
                // Typewriter effect: animate char_progress 0→1
                if delay_ms > 0.0 && duration_ms == 0.0 && t_start_ms > 0 {
                    let guard_time = t_start_ms.saturating_sub(1);
                    super::ensure_guard_keyframe(&mut track.text.char_progress, guard_time, 1.0);
                }

                track
                    .text
                    .char_progress
                    .ensure(1.0)
                    .add_keyframe(t_start_ms, 0.0, Easing::Linear);
                track.text.char_progress.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
            } else {
                ensure_reveal_stroke(track, t_start_ms);
                if delay_ms > 0.0 && duration_ms == 0.0 && t_start_ms > 0 {
                    let guard_time = t_start_ms.saturating_sub(1);
                    super::ensure_guard_keyframe(&mut track.style.stroke_progress, guard_time, 1.0);
                    super::ensure_guard_keyframe(&mut track.style.fill_opacity, guard_time, 1.0);
                }

                track.style.stroke_progress.ensure(1.0).add_keyframe(
                    t_start_ms,
                    0.0,
                    Easing::Linear,
                );
                track
                    .style
                    .fill_opacity
                    .ensure(1.0)
                    .add_keyframe(t_start_ms, 0.0, Easing::Linear);

                if duration_ms > 0.0 && t_end_ms > t_start_ms {
                    track.style.fill_opacity.ensure(1.0).add_keyframe(
                        t_end_ms.saturating_sub(1),
                        0.0,
                        Easing::Linear,
                    );
                }

                track.style.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
                track.style.fill_opacity.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
            }
        }
    }
}

/// Reveals vector targets by drawing stroke progress first, then popping fill at the end.
pub struct RevealIn;

impl BuiltinAction for RevealIn {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "reveal-in".to_string(),
            category: "Entrance".to_string(),
            description:
                "Reveals vector targets by drawing stroke progress first, then popping fill at the end."
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

            // Reveal pre-keyframe ("hidden by default") targets: reveal-in
            // animates stroke_progress/fill_opacity but never opacity, so
            // lift the seeded opacity 0 alongside the reveal.
            super::lift_hidden_by_default(track, t_start_ms, t_end_ms, easing);

            let has_prior_stroke = track
                .style
                .stroke_progress
                .as_ref()
                .map(|t| t.keyframes.keys().any(|&k| k < t_start_ms))
                .unwrap_or(false);
            let start_stroke = if has_prior_stroke {
                track.style.stroke_progress.get(t_start_ms, 1.0)
            } else {
                0.0
            };

            ensure_reveal_stroke(track, t_start_ms);

            if duration_ms > 0.0 {
                track.style.stroke_progress.ensure(1.0).add_keyframe(
                    t_start_ms,
                    start_stroke,
                    Easing::Linear,
                );
                track
                    .style
                    .fill_opacity
                    .ensure(1.0)
                    .add_keyframe(t_start_ms, 0.0, Easing::Linear);
                if t_end_ms > t_start_ms {
                    track.style.fill_opacity.ensure(1.0).add_keyframe(
                        t_end_ms.saturating_sub(1),
                        0.0,
                        Easing::Linear,
                    );
                }
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                super::ensure_guard_keyframe(&mut track.style.stroke_progress, guard_time, 1.0);
                super::ensure_guard_keyframe(&mut track.style.fill_opacity, guard_time, 1.0);
            }

            track.style.fill_opacity.ensure(1.0).add_keyframe(t_end_ms, 1.0, Easing::Linear);
            track.style.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 1.0, easing);
        }
    }
}

/// Wipes out vector targets by animating stroke progress and fill opacity down together.
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

            let start_stroke = track.style.stroke_progress.get(t_start_ms, 1.0);
            let start_fill = track.style.fill_opacity.get(t_start_ms, 1.0);

            if duration_ms > 0.0 {
                track.style.stroke_progress.ensure(1.0).add_keyframe(
                    t_start_ms,
                    start_stroke,
                    Easing::Linear,
                );
                track.style.fill_opacity.ensure(1.0).add_keyframe(
                    t_start_ms,
                    start_fill,
                    Easing::Linear,
                );
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                super::ensure_guard_keyframe(&mut track.style.stroke_progress, guard_time, 1.0);
                super::ensure_guard_keyframe(&mut track.style.fill_opacity, guard_time, 1.0);
            }

            track.style.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
            track.style.fill_opacity.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
        }
    }
}

/// Exits vector targets by hiding fill at the start, then erasing stroke progress over time.
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

            let start_stroke = track.style.stroke_progress.get(t_start_ms, 1.0);

            if duration_ms > 0.0 {
                track.style.stroke_progress.ensure(1.0).add_keyframe(
                    t_start_ms,
                    start_stroke,
                    Easing::Linear,
                );
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                super::ensure_guard_keyframe(&mut track.style.stroke_progress, guard_time, 1.0);
                super::ensure_guard_keyframe(&mut track.style.fill_opacity, guard_time, 1.0);
            }

            track
                .style
                .fill_opacity
                .ensure(1.0)
                .add_keyframe(t_start_ms, 0.0, Easing::Linear);
            track.style.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
        }
    }
}

/// Exits vector targets by erasing stroke progress over time while keeping fill until the end.
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

            let start_stroke = track.style.stroke_progress.get(t_start_ms, 1.0);
            let start_fill = track.style.fill_opacity.get(t_start_ms, 1.0);

            if duration_ms > 0.0 {
                track.style.stroke_progress.ensure(1.0).add_keyframe(
                    t_start_ms,
                    start_stroke,
                    Easing::Linear,
                );
                track.style.fill_opacity.ensure(1.0).add_keyframe(
                    t_start_ms,
                    start_fill,
                    Easing::Linear,
                );
                track.style.fill_opacity.ensure(1.0).add_keyframe(
                    t_end_ms.saturating_sub(1),
                    start_fill,
                    Easing::Linear,
                );
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                super::ensure_guard_keyframe(&mut track.style.stroke_progress, guard_time, 1.0);
                super::ensure_guard_keyframe(&mut track.style.fill_opacity, guard_time, 1.0);
            }

            track.style.stroke_progress.ensure(1.0).add_keyframe(t_end_ms, 0.0, easing);
            track.style.fill_opacity.ensure(1.0).add_keyframe(t_end_ms, 0.0, Easing::Linear);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Modifier, Property, Stmt, Time};
    use crate::diagnostics::DiagnosticCode;
    use crate::primitives::{BuildCtx, Primitive};
    use crate::timeline::ActorCategory;
    use crate::timeline::actions::process_action;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct TextExt;

    impl Primitive for TextExt {
        fn type_name(&self) -> &str {
            "TextExt"
        }

        fn display_name(&self) -> &str {
            "Text Extension"
        }

        fn category(&self) -> ActorCategory {
            ActorCategory::Text
        }

        fn icon_id(&self) -> &str {
            "text-ext"
        }

        fn kind_id(&self) -> ActorKindId {
            ActorKindId::Extension
        }

        fn capabilities(&self) -> animatix_syntax::schema::PrimitiveCapabilities {
            animatix_syntax::schema::PrimitiveCapabilities {
                text_paths: true,
                morphable_paths: true,
                vector_reveal_target: true,
                ..animatix_syntax::schema::PrimitiveCapabilities::default()
            }
        }

        fn build(
            &self,
            ctx: &mut BuildCtx,
            label: &str,
            _props: &[crate::ast::Property],
            _modifiers: &[crate::ast::Modifier],
            _children: &[crate::ast::InlineItem],
        ) -> Result<(), Vec<Diagnostic>> {
            let track = ctx
                .timeline
                .tracks
                .entry(label.to_string())
                .or_insert_with(|| crate::timeline::AnimationTrack::new(label.to_string()));
            track.kind = ActorKindId::Extension;
            track.rebuild_property_plan();
            Ok(())
        }
    }

    fn circle_decl(label: &str) -> Stmt {
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: label.to_string(),
            array_index: None,
            ty: "Ellipse".to_string(),
            props: vec![
                Property {
                    name: "size".to_string(),
                    value: crate::ast::Expr::Tuple(vec![
                        crate::ast::Expr::Num(80.0),
                        crate::ast::Expr::Num(80.0),
                    ]),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "at".to_string(),
                    value: crate::ast::Expr::Tuple(vec![
                        crate::ast::Expr::Num(320.0),
                        crate::ast::Expr::Num(240.0),
                    ]),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    }

    fn image_decl(label: &str) -> Stmt {
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: label.to_string(),
            array_index: None,
            ty: "Image".to_string(),
            props: vec![
                Property {
                    name: "url".to_string(),
                    value: Expr::Str("../../examples/checker.png".to_string()),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "at".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(320.0), Expr::Num(240.0)]),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(120.0)]),
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
                    value: crate::ast::Expr::Ident(format!("{duration_s}s")),
                }],
                byte_span: None,
                target_index: vec![],
            },
            None,
        )
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

        assert_eq!(track.style.stroke_progress.get(0, 1.0), 0.0);
        assert_eq!(track.style.stroke_progress.get(1000, 1.0), 1.0);
        assert_eq!(track.style.fill_opacity.get(500, 1.0), 0.0);
        assert_eq!(track.style.fill_opacity.get(1000, 1.0), 1.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn draw_in_lifts_hidden_by_default_opacity() {
        // Regression: a pre-keyframe declaration is seeded `opacity = 0`;
        // draw-in animates only stroke_progress/fill_opacity, which used to
        // leave the target fully transparent forever (the "Path renders
        // blank in image export" report).
        let ast = vec![
            circle_decl("shape"),
            Stmt::Keyframe {
                time: Time::Seconds(0.5),
                body: vec![action_stmt("draw-in", "shape", 0.7)],
                span: None,
            },
        ];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("shape").expect("shape track");

        assert_eq!(track.style.opacity.get(0, 1.0), 0.0);
        assert_eq!(track.style.opacity.get(400, 1.0), 0.0, "still hidden before the draw");
        assert!(track.style.opacity.get(900, 1.0) > 0.0, "opacity must rise during the draw");
        assert_eq!(track.style.opacity.get(1200, 1.0), 1.0, "fully visible after the draw");
        assert!(!track.hidden_by_default, "flag must be consumed by the lift");
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn draw_in_typewriter_lifts_hidden_by_default_opacity() {
        // Text draw-in animates char_progress only; the opacity hide must
        // still be lifted so the typewriter effect is visible.
        let ast = vec![
            text_decl("headline"),
            Stmt::Keyframe {
                time: Time::Seconds(0.5),
                body: vec![action_stmt("draw-in", "headline", 0.7)],
                span: None,
            },
        ];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("headline").expect("headline track");

        assert_eq!(track.style.opacity.get(400, 1.0), 0.0);
        assert_eq!(track.style.opacity.get(1200, 1.0), 1.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn reveal_in_lifts_hidden_by_default_opacity() {
        let ast = vec![
            circle_decl("shape"),
            Stmt::Keyframe {
                time: Time::Seconds(0.5),
                body: vec![action_stmt("reveal-in", "shape", 0.7)],
                span: None,
            },
        ];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("shape").expect("shape track");

        assert_eq!(track.style.opacity.get(400, 1.0), 0.0);
        assert_eq!(track.style.opacity.get(1200, 1.0), 1.0);
        assert!(!track.hidden_by_default);
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

        assert_eq!(track.style.fill_opacity.get(0, 1.0), 0.0);
        assert_eq!(track.style.stroke_progress.get(0, 1.0), 1.0);
        assert!(track.style.stroke_progress.get(500, 1.0) > 0.0);
        assert!(track.style.stroke_progress.get(500, 1.0) < 1.0);
        assert_eq!(track.style.stroke_progress.get(1000, 1.0), 0.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn reveal_out_preserves_prior_state_for_delayed_instant_change() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                circle_decl("shape"),
                Stmt::Action(
                    Action {
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
                        byte_span: None,
                        target_index: vec![],
                    },
                    None,
                ),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("shape").expect("shape track");

        assert_eq!(track.style.fill_opacity.get(249, 1.0), 1.0);
        assert_eq!(track.style.stroke_progress.get(249, 1.0), 1.0);
        assert_eq!(track.style.fill_opacity.get(250, 1.0), 0.0);
        assert_eq!(track.style.stroke_progress.get(250, 1.0), 0.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn reveal_out_on_text_proceeds_without_diagnostics() {
        // Text targets are now allowed through vector reveal actions.
        // They will animate fill_opacity even though stroke_progress has no visual effect.
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
            !report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::UnsupportedActionTarget),
            "reveal-out on text should not report unsupported target"
        );

        let track = report.output.tracks.get("headline").expect("headline track");
        // reveal-out hides fill at start (0.0) and keeps it hidden while stroke animates out
        assert_eq!(track.style.fill_opacity.get(0, 1.0), 0.0);
        assert_eq!(track.style.fill_opacity.get(1000, 1.0), 0.0);
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

        assert_eq!(track.style.stroke_progress.get(0, 1.0), 1.0);
        assert_eq!(track.style.fill_opacity.get(0, 1.0), 1.0);
        assert!(track.style.stroke_progress.get(500, 1.0) > 0.0);
        assert!(track.style.stroke_progress.get(500, 1.0) < 1.0);
        assert_eq!(track.style.fill_opacity.get(500, 1.0), 1.0);
        assert_eq!(track.style.stroke_progress.get(1000, 1.0), 0.0);
        assert_eq!(track.style.fill_opacity.get(1000, 1.0), 0.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn draw_out_on_text_proceeds_without_diagnostics() {
        // Text targets are now allowed through vector reveal actions.
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
            !report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::UnsupportedActionTarget),
            "draw-out on text should not report unsupported target"
        );

        let track = report.output.tracks.get("headline").expect("headline track");
        // fill_opacity should still be animated (fade-out behavior)
        assert_eq!(track.style.fill_opacity.get(0, 1.0), 1.0);
        assert_eq!(track.style.fill_opacity.get(1000, 1.0), 0.0);
    }

    #[test]
    fn draw_in_on_text_animates_char_progress() {
        // draw-in on Text should animate char_progress 0→1 instead of stroke_progress.
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                text_decl("headline"),
                action_stmt("draw-in", "headline", 2.0),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("headline").expect("headline track");

        // char_progress should be 0 at start, 1 at end
        assert_eq!(track.text.char_progress.get(0, 1.0), 0.0);
        assert_eq!(track.text.char_progress.get(2000, 1.0), 1.0);
        // Midpoint should be ~0.5
        let mid = track.text.char_progress.get(1000, 1.0);
        assert!(
            mid > 0.0 && mid < 1.0,
            "char_progress at midpoint should be between 0 and 1, got {}",
            mid
        );
        // No diagnostics should be emitted
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn draw_in_on_text_does_not_set_stroke_progress() {
        // draw-in on Text should NOT modify stroke_progress or fill_opacity.
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![text_decl("msg"), action_stmt("draw-in", "msg", 1.0)],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("msg").expect("msg track");

        // stroke_progress and fill_opacity should not have been set
        assert!(
            track.style.stroke_progress.is_none()
                || track
                    .style
                    .stroke_progress
                    .as_ref()
                    .map(|t| t.keyframes.is_empty())
                    .unwrap_or(true)
        );
        assert!(
            track.style.fill_opacity.is_none()
                || track
                    .style
                    .fill_opacity
                    .as_ref()
                    .map(|t| t.keyframes.is_empty())
                    .unwrap_or(true)
        );
        // Only char_progress should be tracked
        assert!(track.text.char_progress.is_some());
        assert!(!track.text.char_progress.as_ref().unwrap().keyframes.is_empty());
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn draw_in_on_regular_shape_still_animates_stroke_progress() {
        // Verify that draw-in on regular shapes still uses stroke_progress.
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![circle_decl("shape"), action_stmt("draw-in", "shape", 1.0)],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("shape").expect("shape track");

        assert_eq!(track.style.stroke_progress.get(0, 1.0), 0.0);
        assert_eq!(track.style.stroke_progress.get(1000, 1.0), 1.0);
        assert_eq!(track.style.fill_opacity.get(500, 1.0), 0.0);
        assert_eq!(track.style.fill_opacity.get(1000, 1.0), 1.0);
        // char_progress should NOT be set for non-text targets
        assert!(track.text.char_progress.is_none());
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn reveal_in_draws_stroke_then_pops_fill() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![circle_decl("shape"), action_stmt("reveal-in", "shape", 1.0)],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("shape").expect("shape track");

        assert_eq!(track.style.stroke_progress.get(0, 1.0), 0.0);
        assert_eq!(track.style.fill_opacity.get(0, 1.0), 0.0);
        assert!(track.style.stroke_progress.get(500, 1.0) > 0.0);
        assert!(track.style.stroke_progress.get(500, 1.0) < 1.0);
        assert_eq!(track.style.fill_opacity.get(500, 1.0), 0.0);
        assert_eq!(track.style.stroke_progress.get(1000, 1.0), 1.0);
        assert_eq!(track.style.fill_opacity.get(1000, 1.0), 1.0);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn draw_in_uses_text_capability_for_extension_primitives() {
        let (ast, errors) = animatix_syntax::parser::parse_source("p: TextExt");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");

        let mut registry = crate::primitives::PrimitiveRegistry::new();
        registry.register(Arc::new(TextExt)).expect("register TextExt");
        let report =
            Timeline::build_with_primitive_registry(&ast, &HashMap::new(), Arc::new(registry));
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        let mut timeline = report.output;
        let action = match action_stmt("draw-in", "p", 1.0) {
            Stmt::Action(action, _) => action,
            _ => unreachable!("action statement"),
        };
        let mut diagnostics = Vec::new();
        process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);
        assert!(diagnostics.is_empty(), "unexpected diagnostics: {:?}", diagnostics);
        let track = timeline.tracks.get("p").expect("text extension track");
        assert!(track.text.char_progress.is_some());
        assert!(
            track.style.stroke_progress.is_none()
                || track.style.stroke_progress.as_ref().is_some_and(|t| t.keyframes.is_empty()),
            "text capability should select the typewriter path"
        );
    }

    #[test]
    fn draw_in_is_entrance_category() {
        let sig = DrawIn.signature();
        assert_eq!(sig.category, "Entrance");
    }

    #[test]
    fn reveal_in_is_entrance_category() {
        let sig = RevealIn.signature();
        assert_eq!(sig.category, "Entrance");
    }
}
