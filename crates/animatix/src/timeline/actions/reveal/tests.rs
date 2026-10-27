//! Tests for reveal actions (draw/wipe/typewriter).

use super::{DrawIn, RevealIn};
use std::collections::HashMap;
use std::sync::Arc;

use animatix_core::caps::ActorCategory;

use crate::ast::{Action, Expr, Modifier, Property, Stmt, Time};
use crate::diagnostics::{Diagnostic, DiagnosticCode};
use crate::primitives::{BuildCtx, Primitive};
use crate::timeline::Timeline;
use crate::timeline::actions::process_action;
use crate::timeline::actions::registry::BuiltinAction;
use crate::timeline::property_track::TrackAccessor;

struct TextExt;

impl Primitive for TextExt {
    fn type_name(&self) -> &str {
        "TextExt"
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        _props: &[crate::ast::Property],
        _modifiers: &[crate::ast::Modifier],
        _children: &[crate::ast::InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        let track_id = ctx.timeline.world.spawn(label, "Rect");
        ctx.timeline.world.rebuild_property_plan(track_id);
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
    let shape_id = report.output.world.get_id("shape").expect("shape id");
    let style = report.output.world.style(shape_id);

    assert_eq!(style.stroke_progress.get(0, 1.0), 0.0);
    assert_eq!(style.stroke_progress.get(1000, 1.0), 1.0);
    assert_eq!(style.fill_opacity.get(500, 1.0), 0.0);
    assert_eq!(style.fill_opacity.get(1000, 1.0), 1.0);
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
    let shape_id = report.output.world.get_id("shape").expect("shape id");
    let style = report.output.world.style(shape_id);

    assert_eq!(style.opacity.get(0, 1.0), 0.0);
    assert_eq!(style.opacity.get(400, 1.0), 0.0, "still hidden before the draw");
    assert!(style.opacity.get(900, 1.0) > 0.0, "opacity must rise during the draw");
    assert_eq!(style.opacity.get(1200, 1.0), 1.0, "fully visible after the draw");
    assert!(
        !report.output.world.meta(shape_id).hidden_by_default,
        "flag must be consumed by the lift"
    );
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
    let headline_id = report.output.world.get_id("headline").expect("headline id");
    let style = report.output.world.style(headline_id);

    assert_eq!(style.opacity.get(400, 1.0), 0.0);
    assert_eq!(style.opacity.get(1200, 1.0), 1.0);
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
    let shape_id = report.output.world.get_id("shape").expect("shape id");
    let style = report.output.world.style(shape_id);

    assert_eq!(style.opacity.get(400, 1.0), 0.0);
    assert_eq!(style.opacity.get(1200, 1.0), 1.0);
    assert!(!report.output.world.meta(shape_id).hidden_by_default);
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
    let shape_id = report.output.world.get_id("shape").expect("shape id");
    let style = report.output.world.style(shape_id);

    assert_eq!(style.fill_opacity.get(0, 1.0), 0.0);
    assert_eq!(style.stroke_progress.get(0, 1.0), 1.0);
    assert!(style.stroke_progress.get(500, 1.0) > 0.0);
    assert!(style.stroke_progress.get(500, 1.0) < 1.0);
    assert_eq!(style.stroke_progress.get(1000, 1.0), 0.0);
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
    let shape_id = report.output.world.get_id("shape").expect("shape id");
    let style = report.output.world.style(shape_id);

    assert_eq!(style.fill_opacity.get(249, 1.0), 1.0);
    assert_eq!(style.stroke_progress.get(249, 1.0), 1.0);
    assert_eq!(style.fill_opacity.get(250, 1.0), 0.0);
    assert_eq!(style.stroke_progress.get(250, 1.0), 0.0);
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

    let headline_id = report.output.world.get_id("headline").expect("headline id");
    let style = report.output.world.style(headline_id);
    // reveal-out hides fill at start (0.0) and keeps it hidden while stroke animates out
    assert_eq!(style.fill_opacity.get(0, 1.0), 0.0);
    assert_eq!(style.fill_opacity.get(1000, 1.0), 0.0);
}

#[test]
fn draw_out_keeps_fill_until_the_end() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![circle_decl("shape"), action_stmt("draw-out", "shape", 1.0)],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let shape_id = report.output.world.get_id("shape").expect("shape id");
    let style = report.output.world.style(shape_id);

    assert_eq!(style.stroke_progress.get(0, 1.0), 1.0);
    assert_eq!(style.fill_opacity.get(0, 1.0), 1.0);
    assert!(style.stroke_progress.get(500, 1.0) > 0.0);
    assert!(style.stroke_progress.get(500, 1.0) < 1.0);
    assert_eq!(style.fill_opacity.get(500, 1.0), 1.0);
    assert_eq!(style.stroke_progress.get(1000, 1.0), 0.0);
    assert_eq!(style.fill_opacity.get(1000, 1.0), 0.0);
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

    let headline_id = report.output.world.get_id("headline").expect("headline id");
    let style = report.output.world.style(headline_id);
    // fill_opacity should still be animated (fade-out behavior)
    assert_eq!(style.fill_opacity.get(0, 1.0), 1.0);
    assert_eq!(style.fill_opacity.get(1000, 1.0), 0.0);
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
    let id = report.output.world.get_id("headline").expect("headline entity");
    let text = report.output.world.text_or_default(id);

    // char_progress should be 0 at start, 1 at end
    assert_eq!(text.char_progress.get(0, 1.0), 0.0);
    assert_eq!(text.char_progress.get(2000, 1.0), 1.0);
    // Midpoint should be ~0.5
    let mid = text.char_progress.get(1000, 1.0);
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
    let msg_id = report.output.world.get_id("msg").expect("msg id");
    let style = report.output.world.style(msg_id);

    // stroke_progress and fill_opacity should not have been set
    assert!(
        style.stroke_progress.is_none()
            || style.stroke_progress.as_ref().map(|t| t.keyframes.is_empty()).unwrap_or(true)
    );
    assert!(
        style.fill_opacity.is_none()
            || style.fill_opacity.as_ref().map(|t| t.keyframes.is_empty()).unwrap_or(true)
    );
    // Only char_progress should be tracked
    let text = report
        .output
        .world
        .text_or_default(report.output.world.get_id("msg").expect("msg entity"));
    assert!(text.char_progress.is_some());
    assert!(!text.char_progress.as_ref().unwrap().keyframes.is_empty());
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
    let shape_id = report.output.world.get_id("shape").expect("shape id");
    let style = report.output.world.style(shape_id);

    assert_eq!(style.stroke_progress.get(0, 1.0), 0.0);
    assert_eq!(style.stroke_progress.get(1000, 1.0), 1.0);
    assert_eq!(style.fill_opacity.get(500, 1.0), 0.0);
    assert_eq!(style.fill_opacity.get(1000, 1.0), 1.0);
    // char_progress should NOT be set for non-text targets
    let text = report
        .output
        .world
        .text_or_default(report.output.world.get_id("shape").expect("shape entity"));
    assert!(text.char_progress.is_none());
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
    let shape_id = report.output.world.get_id("shape").expect("shape id");
    let style = report.output.world.style(shape_id);

    assert_eq!(style.stroke_progress.get(0, 1.0), 0.0);
    assert_eq!(style.fill_opacity.get(0, 1.0), 0.0);
    assert!(style.stroke_progress.get(500, 1.0) > 0.0);
    assert!(style.stroke_progress.get(500, 1.0) < 1.0);
    assert_eq!(style.fill_opacity.get(500, 1.0), 0.0);
    assert_eq!(style.stroke_progress.get(1000, 1.0), 1.0);
    assert_eq!(style.fill_opacity.get(1000, 1.0), 1.0);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn draw_in_uses_text_capability_for_extension_primitives() {
    let (ast, errors) = animatix_syntax::parser::parse_source("p: TextExt");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut registry = crate::primitives::PrimitiveRegistry::new();
    let mut text_ext_info = animatix_std::PrimitiveInfo::extension("TextExt", ActorCategory::Text);
    text_ext_info.capabilities.text_paths = true;
    text_ext_info.text = Some(animatix_core::caps::TextKind::Text);
    registry.register(Arc::new(TextExt), text_ext_info).expect("register TextExt");
    let report = Timeline::build_with_primitive_registry(&ast, &HashMap::new(), Arc::new(registry));
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
    let id = timeline.world.get_id("p").expect("text extension entity");
    let style = timeline.world.style(id);
    let text = timeline.world.text_or_default(id);
    assert!(text.char_progress.is_some());
    assert!(
        style.stroke_progress.is_none()
            || style.stroke_progress.as_ref().is_some_and(|t| t.keyframes.is_empty()),
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
