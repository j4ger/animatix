use super::*;

use crate::ast::{Expr, Property, Stmt, Time};
use crate::timeline::PlacementMode;

fn circle_decl(label: &str) -> Stmt {
    Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
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
        span: None,
    }
}

fn shift_action(target: &str, by: Expr) -> Stmt {
    Stmt::Action(
        Action {
            verb: "shift".to_string(),
            targets: vec![target.to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("by".to_string()),
                    value: by,
                },
                Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                },
            ],
            byte_span: None,
            target_index: vec![],
        },
        None,
    )
}

fn move_action(target: &str, to: Expr) -> Stmt {
    Stmt::Action(
        Action {
            verb: "move".to_string(),
            targets: vec![target.to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("to".to_string()),
                    value: to,
                },
                Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                },
            ],
            byte_span: None,
            target_index: vec![],
        },
        None,
    )
}

fn rotate_action(target: &str, by: Expr) -> Stmt {
    Stmt::Action(
        Action {
            verb: "rotate".to_string(),
            targets: vec![target.to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("by".to_string()),
                    value: by,
                },
                Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                },
            ],
            byte_span: None,
            target_index: vec![],
        },
        None,
    )
}

fn scale_action(target: &str, by: Expr) -> Stmt {
    Stmt::Action(
        Action {
            verb: "scale".to_string(),
            targets: vec![target.to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("by".to_string()),
                    value: by,
                },
                Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                },
            ],
            byte_span: None,
            target_index: vec![],
        },
        None,
    )
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
    let track = report.output.world.tracks.get("badge").expect("badge track");

    assert_eq!(track.geometry.motion_offset.get(0, [0.0, 0.0]), [0.0, 0.0]);
    assert_eq!(track.geometry.motion_offset.get(1000, [0.0, 0.0]), [120.0, -30.0]);
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
    let track = report.output.world.tracks.get("badge").expect("badge track");

    assert!((track.geometry.rotation.get(0, 0.0) - 0.0).abs() < f32::EPSILON);
    assert!((track.geometry.rotation.get(1000, 0.0) - std::f32::consts::FRAC_PI_2).abs() < 0.0001);
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
    let track = report.output.world.tracks.get("badge").expect("badge track");

    assert!((track.geometry.scale.get(0, 1.0) - 1.0).abs() < f32::EPSILON);
    assert!((track.geometry.scale.get(1000, 1.0) - 1.5).abs() < 0.0001);
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
    let track = report.output.world.tracks.get("badge").expect("badge track");

    assert_eq!(track.geometry.motion_offset.get(0, [0.0, 0.0]), [0.0, 0.0]);
    assert_eq!(track.geometry.motion_offset.get(1000, [0.0, 0.0]), [40.0, -24.0]);
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
    let track = report.output.world.tracks.get("child").expect("child track");

    assert_eq!(
        track.geometry.placement_mode.get(0, PlacementMode::LayoutManaged),
        PlacementMode::LayoutManaged
    );
    assert_eq!(track.geometry.motion_offset.get(1000, [0.0, 0.0]), [25.0, 0.0]);
}
