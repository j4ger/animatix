use super::super::test_support::circle_decl;
use super::*;

use crate::ast::{Expr, Property, Stmt, Time};
use crate::timeline::PlacementMode;

fn motion_action(verb: &str, modifier: &str, target: &str, value: Expr) -> Stmt {
    Stmt::Action(
        Action {
            verb: verb.to_string(),
            targets: vec![target.to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some(modifier.to_string()),
                    value,
                },
                Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                },
            ],
            byte_span: None,
            target_index: vec![],
            target_index_position: vec![],
        },
        None,
    )
}

fn shift_action(target: &str, by: Expr) -> Stmt {
    motion_action("shift", "by", target, by)
}

fn move_action(target: &str, to: Expr) -> Stmt {
    motion_action("move", "to", target, to)
}

fn rotate_action(target: &str, by: Expr) -> Stmt {
    motion_action("rotate", "by", target, by)
}

fn scale_action(target: &str, by: Expr) -> Stmt {
    motion_action("scale", "by", target, by)
}

#[test]
fn move_animates_motion_offset_to_target_value() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            circle_decl("badge"),
            move_action("badge", Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(-30.0)])),
        ],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let badge_id = report.output.world.get_id("badge").expect("badge id");
    let geometry = report.output.world.geometry(badge_id);

    assert_eq!(geometry.motion_offset.get(0, [0.0, 0.0]), [0.0, 0.0]);
    assert_eq!(geometry.motion_offset.get(1000, [0.0, 0.0]), [120.0, -30.0]);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn move_requires_to_vec2_modifier() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![circle_decl("badge"), move_action("badge", Expr::Num(10.0))],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidModifierValue)
    );
}

#[test]
fn rotate_animates_rotation_track_over_duration() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            circle_decl("badge"),
            rotate_action("badge", Expr::Num(std::f64::consts::FRAC_PI_2)),
        ],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let badge_id = report.output.world.get_id("badge").expect("badge id");
    let geometry = report.output.world.geometry(badge_id);

    assert!((geometry.rotation.get(0, 0.0) - 0.0).abs() < f32::EPSILON);
    assert!((geometry.rotation.get(1000, 0.0) - std::f32::consts::FRAC_PI_2).abs() < 0.0001);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn rotate_requires_numeric_by_modifier() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            circle_decl("badge"),
            rotate_action("badge", Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0)])),
        ],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidModifierValue)
    );
}

#[test]
fn scale_animates_scale_track_over_duration() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![circle_decl("badge"), scale_action("badge", Expr::Num(1.5))],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let badge_id = report.output.world.get_id("badge").expect("badge id");
    let geometry = report.output.world.geometry(badge_id);

    assert!((geometry.scale.get(0, 1.0) - 1.0).abs() < f32::EPSILON);
    assert!((geometry.scale.get(1000, 1.0) - 1.5).abs() < 0.0001);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn scale_requires_positive_numeric_by_modifier() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![circle_decl("badge"), scale_action("badge", Expr::Num(0.0))],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidModifierValue)
    );
}

#[test]
fn shift_animates_motion_offset_over_duration() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            circle_decl("badge"),
            shift_action("badge", Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(-24.0)])),
        ],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let badge_id = report.output.world.get_id("badge").expect("badge id");
    let geometry = report.output.world.geometry(badge_id);

    assert_eq!(geometry.motion_offset.get(0, [0.0, 0.0]), [0.0, 0.0]);
    assert_eq!(geometry.motion_offset.get(1000, [0.0, 0.0]), [40.0, -24.0]);
    assert!(report.diagnostics.is_empty());
}

#[test]
fn shift_requires_by_vec2_modifier() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![circle_decl("badge"), shift_action("badge", Expr::Num(10.0))],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidModifierValue)
    );
}

#[test]
fn shift_preserves_layout_managed_targets() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "row".to_string(),
                array_index: None,
                ty: "Row".to_string(),
                props: vec![Property {
                    name: "gap".to_string(),
                    value: Expr::Num(20.0),
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![crate::ast::InlineItem::Labeled {
                    label: "child".to_string(),
                    array_index: None,
                    ty: "Ellipse".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(40.0)]),
                        value_span: None,
                        trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                }],
                span: None,
            },
            shift_action("child", Expr::Tuple(vec![Expr::Num(25.0), Expr::Num(0.0)])),
        ],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let child_id = report.output.world.get_id("child").expect("child id");
    let geometry = report.output.world.geometry(child_id);

    assert_eq!(
        geometry.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(geometry.motion_offset.get(1000, [0.0, 0.0]), [25.0, 0.0]);
}

#[test]
fn test_motion_interruption_velocity_continuity() {
    let source = r#"
#0s
b: Rect, at: (0, 0), size: (20, 20), color: (1, 1, 1, 1)

#1s
move b [to: (200, 100), 1s, ease: spring(15, 20)]

#1.5s
move b [to: (400, 200), 1s, ease: spring(15, 20)]
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = Timeline::build_with_diagnostics(&ast.unwrap(), &Default::default());
    let non_lints: Vec<_> =
        crate::timeline::tests::without_content_lints(&report.diagnostics).collect();
    assert!(non_lints.is_empty(), "unexpected diagnostics: {non_lints:?}");

    let timeline = report.output;
    let track = timeline.world.tracks.get_id("b").expect("actor track b");
    let geometry = timeline.world.geometry(track);

    // Sample positions before and after the 1.5s interruption
    let pos_1490 = geometry.position.get(1490, [0.0, 0.0]);
    let pos_1500 = geometry.position.get(1500, [0.0, 0.0]);
    let pos_1510 = geometry.position.get(1510, [0.0, 0.0]);

    // Positional continuity: no teleportation at 1500ms
    let step_before = [pos_1500[0] - pos_1490[0], pos_1500[1] - pos_1490[1]];
    let step_after = [pos_1510[0] - pos_1500[0], pos_1510[1] - pos_1500[1]];

    // The velocity vector must not abruptly flip sign or jump discontinuously
    assert!(
        (step_before[0] - step_after[0]).abs() < 5.0,
        "X velocity discontinuity: {step_before:?} vs {step_after:?}"
    );
    assert!(
        (step_before[1] - step_after[1]).abs() < 5.0,
        "Y velocity discontinuity: {step_before:?} vs {step_after:?}"
    );
}
