use super::*;

#[test]
fn always_overrides_keyframes_warning() {
    // Keyframe at 0s with an Assignment for box1.opacity = 1.0 creates
    // a keyframe in the opacity track.  Then the always block also writes
    // to box1.opacity, which should trigger the warning.
    let ast = vec![
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "box1".to_string(),
                    array_index: None,
                    ty: "Rect".to_string(),
                    props: vec![Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
                        value_span: None,
                        trailing_comment: None,
                    }],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
                Stmt::Assignment {
                    target: vec!["box1".to_string()],
                    property: "opacity".to_string(),
                    value: Expr::Num(1.0),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                },
            ],
            span: None,
        },
        Stmt::Always {
            body: vec![Stmt::Assignment {
                target: vec!["box1".to_string()],
                property: "opacity".to_string(),
                value: Expr::Num(0.5),
                modifiers: vec![],
                easing: None,
                value_span: None,
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let has_warning = report
        .diagnostics
        .iter()
        .any(|d| d.code == animatix_syntax::diagnostics::DiagnosticCode::AlwaysOverridesKeyframes);
    assert!(
        has_warning,
        "Expected AlwaysOverridesKeyframes warning when both keyframes and always block target the same property"
    );
}

#[test]
fn always_overrides_keyframes_no_warning_without_track() {
    // No keyframe at all, just an always block.  The target actor doesn't
    // exist in tracks, so no warning should be emitted.
    let ast = vec![Stmt::Always {
        body: vec![Stmt::Assignment {
            target: vec!["box1".to_string()],
            property: "opacity".to_string(),
            value: Expr::Num(0.5),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let has_warning = report
        .diagnostics
        .iter()
        .any(|d| d.code == animatix_syntax::diagnostics::DiagnosticCode::AlwaysOverridesKeyframes);
    assert!(
        !has_warning,
        "Should NOT emit AlwaysOverridesKeyframes warning when actor doesn't exist in tracks"
    );
}

#[test]
fn always_overrides_keyframes_no_warning_without_conflict() {
    // ActorDecl creates a track but the always block writes to a property
    // that has no keyframes (e.g., rotation is not set by insert_end_keyframes).
    // No warning should be emitted.
    let ast = vec![
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "box1".to_string(),
            array_index: None,
            ty: "Rect".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
            span: None,
        },
        Stmt::Always {
            body: vec![Stmt::Assignment {
                target: vec!["box1".to_string()],
                property: "rotation".to_string(),
                value: Expr::Num(0.5),
                modifiers: vec![],
                easing: None,
                value_span: None,
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let has_warning = report
        .diagnostics
        .iter()
        .any(|d| d.code == animatix_syntax::diagnostics::DiagnosticCode::AlwaysOverridesKeyframes);
    assert!(
        !has_warning,
        "Should NOT emit AlwaysOverridesKeyframes warning when the always property has no keyframes"
    );
}

#[test]
fn absolute_position_on_layout_managed_child_warning() {
    // A child of a Row with explicit `at` should emit a warning.
    let ast = vec![Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "row1".to_string(),
            array_index: None,
            ty: "Row".to_string(),
            props: vec![Property {
                name: "size".to_string(),
                value: Expr::Tuple(vec![Expr::Num(400.0), Expr::Num(100.0)]),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![crate::ast::InlineItem::Labeled {
                label: "child1".to_string(),
                array_index: None,
                ty: "Rect".to_string(),
                props: vec![
                    Property {
                        name: "size".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(50.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let has_warning = report.diagnostics.iter().any(|d| {
        d.code == animatix_syntax::diagnostics::DiagnosticCode::AbsolutePositionOnLayoutManagedChild
    });
    assert!(
        has_warning,
        "Expected AbsolutePositionOnLayoutManagedChild warning when a Row child has 'at'"
    );
}

#[test]
fn absolute_position_on_layout_managed_child_no_warning_without_at() {
    // A child of a Row WITHOUT `at` should NOT emit the warning.
    let ast = vec![Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "row1".to_string(),
            array_index: None,
            ty: "Row".to_string(),
            props: vec![Property {
                name: "size".to_string(),
                value: Expr::Tuple(vec![Expr::Num(400.0), Expr::Num(100.0)]),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![crate::ast::InlineItem::Labeled {
                label: "child1".to_string(),
                array_index: None,
                ty: "Rect".to_string(),
                props: vec![Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(50.0)]),
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let has_warning = report.diagnostics.iter().any(|d| {
        d.code == animatix_syntax::diagnostics::DiagnosticCode::AbsolutePositionOnLayoutManagedChild
    });
    assert!(
        !has_warning,
        "Should NOT emit AbsolutePositionOnLayoutManagedChild warning when child has no 'at'"
    );
}
