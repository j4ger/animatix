use super::*;
use crate::ast::{Expr, InlineItem, Modifier, Property, Stmt, Time};
use crate::timeline::Timeline;

/// Helper: create a minimal Fragment track inside an Equation track.
fn make_equation_with_fragment() -> Vec<Stmt> {
    vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "eq".to_string(),
            array_index: None,
            ty: "Equation".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![InlineItem::Labeled {
                label: "f1".to_string(),
                array_index: None,
                ty: "Fragment".to_string(),
                props: vec![Property {
                    name: "text".to_string(),
                    value: Expr::Str("x^2".to_string()),
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        }],
        span: None,
    }]
}

/// Helper: create an Equation track with multiple Fragment children.
fn make_equation_with_fragments(labels: &[&str]) -> Vec<Stmt> {
    let children: Vec<InlineItem> = labels
        .iter()
        .map(|label| InlineItem::Labeled {
            label: label.to_string(),
            array_index: None,
            ty: "Fragment".to_string(),
            props: vec![Property {
                name: "text".to_string(),
                value: Expr::Str("x".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
        })
        .collect();

    vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "eq".to_string(),
            array_index: None,
            ty: "Equation".to_string(),
            props: vec![],
            modifiers: vec![],
            children,
            span: None,
        }],
        span: None,
    }]
}

/// Helper: create two Equations, each with its own Fragments.
fn make_two_equations() -> Vec<Stmt> {
    vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "eqA".to_string(),
                array_index: None,
                ty: "Equation".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![
                    InlineItem::Labeled {
                        label: "fA1".to_string(),
                        array_index: None,
                        ty: "Fragment".to_string(),
                        props: vec![Property {
                            name: "text".to_string(),
                            value: Expr::Str("a1".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        }],
                        modifiers: vec![],
                        children: vec![],
                    },
                    InlineItem::Labeled {
                        label: "fA2".to_string(),
                        array_index: None,
                        ty: "Fragment".to_string(),
                        props: vec![Property {
                            name: "text".to_string(),
                            value: Expr::Str("a2".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        }],
                        modifiers: vec![],
                        children: vec![],
                    },
                ],
                span: None,
            },
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "eqB".to_string(),
                array_index: None,
                ty: "Equation".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![
                    InlineItem::Labeled {
                        label: "fB1".to_string(),
                        array_index: None,
                        ty: "Fragment".to_string(),
                        props: vec![Property {
                            name: "text".to_string(),
                            value: Expr::Str("b1".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        }],
                        modifiers: vec![],
                        children: vec![],
                    },
                    InlineItem::Labeled {
                        label: "fB2".to_string(),
                        array_index: None,
                        ty: "Fragment".to_string(),
                        props: vec![Property {
                            name: "text".to_string(),
                            value: Expr::Str("b2".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        }],
                        modifiers: vec![],
                        children: vec![],
                    },
                ],
                span: None,
            },
        ],
        span: None,
    }]
}

/// Helper: count opacity keyframes for a track by label.
fn opacity_keyframe_count(
    report: &crate::diagnostics::BuildReport<Timeline>,
    label: &str,
) -> usize {
    report
        .output
        .world
        .get_id(label)
        .map(|id| report.output.world.highlight_or_default(id).highlight_opacity.as_ref())
        .and_then(|v| v)
        .map(|t| t.keyframes.len())
        .unwrap_or(0)
}

#[test]
fn highlight_adds_opacity_keyframes() {
    let mut ast = make_equation_with_fragment();
    // Append a highlight action at t=1s
    if let Stmt::Keyframe { body, .. } = &mut ast[0] {
        body.push(Stmt::Action(
            Action {
                verb: "highlight".to_string(),
                targets: vec!["f1".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("800ms".to_string()),
                }],
                byte_span: None,
                target_index: vec![],
            },
            None,
        ));
    }

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let f1_id = report
        .output
        .world
        .get_id("f1")
        .expect("`f1` should have a track — the Equation's child was never registered");
    // highlight_opacity should have keyframes
    assert!(
        report
            .output
            .world
            .highlight_or_default(f1_id)
            .highlight_opacity
            .as_ref()
            .map(|t| !t.keyframes.is_empty())
            .unwrap_or(false),
        "highlight_opacity should have keyframes"
    );
    // No panics or critical errors
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.severity != crate::diagnostics::DiagnosticSeverity::Error),
        "unexpected errors: {:?}",
        report.diagnostics
    );
}

#[test]
fn unhighlight_adds_opacity_keyframes() {
    let mut ast = make_equation_with_fragment();
    if let Stmt::Keyframe { body, .. } = &mut ast[0] {
        body.push(Stmt::Action(
            Action {
                verb: "unhighlight".to_string(),
                targets: vec!["f1".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("600ms".to_string()),
                }],
                byte_span: None,
                target_index: vec![],
            },
            None,
        ));
    }

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    if let Some(f1_id) = report.output.world.get_id("f1") {
        assert!(
            report
                .output
                .world
                .highlight_or_default(f1_id)
                .highlight_opacity
                .as_ref()
                .map(|t| !t.keyframes.is_empty())
                .unwrap_or(false),
            "highlight_opacity should have keyframes for unhighlight"
        );
    }
    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.severity != crate::diagnostics::DiagnosticSeverity::Error),
        "unexpected errors: {:?}",
        report.diagnostics
    );
}

#[test]
fn exclusive_highlight_unhighlights_siblings() {
    // Equation with f1, f2, f3 — highlight f2, check siblings get unhighlighted
    let mut ast = make_equation_with_fragments(&["f1", "f2", "f3"]);
    if let Stmt::Keyframe { body, .. } = &mut ast[0] {
        body.push(Stmt::Action(
            Action {
                verb: "highlight".to_string(),
                targets: vec!["f2".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("800ms".to_string()),
                }],
                byte_span: None,
                target_index: vec![],
            },
            None,
        ));
    }

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    // f2 should have 2 keyframes (0 → 1)
    assert_eq!(
        opacity_keyframe_count(&report, "f2"),
        2,
        "f2 should have 2 opacity keyframes (highlight in)"
    );

    // f1 should have 2 keyframes (current opacity at t_start → 0)
    assert_eq!(
        opacity_keyframe_count(&report, "f1"),
        2,
        "f1 should have 2 opacity keyframes (unhighlight sibling)"
    );

    // f3 should have 2 keyframes (current opacity at t_start → 0)
    assert_eq!(
        opacity_keyframe_count(&report, "f3"),
        2,
        "f3 should have 2 opacity keyframes (unhighlight sibling)"
    );

    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.severity != crate::diagnostics::DiagnosticSeverity::Error),
        "unexpected errors: {:?}",
        report.diagnostics
    );
}

#[test]
fn multi_target_highlight_exclusive() {
    // Equation with f1, f2, f3 — highlight f1, f2 together
    // Both should be highlighted, f3 should be unhighlighted
    let mut ast = make_equation_with_fragments(&["f1", "f2", "f3"]);
    if let Stmt::Keyframe { body, .. } = &mut ast[0] {
        body.push(Stmt::Action(
            Action {
                verb: "highlight".to_string(),
                targets: vec!["f1".to_string(), "f2".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("800ms".to_string()),
                }],
                byte_span: None,
                target_index: vec![],
            },
            None,
        ));
    }

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    // f1 and f2 should be highlighted (2 keyframes each: 0 → 1)
    assert_eq!(
        opacity_keyframe_count(&report, "f1"),
        2,
        "f1 should have 2 opacity keyframes (highlight in)"
    );
    assert_eq!(
        opacity_keyframe_count(&report, "f2"),
        2,
        "f2 should have 2 opacity keyframes (highlight in)"
    );

    // f3 should be unhighlighted (2 keyframes: current → 0)
    assert_eq!(
        opacity_keyframe_count(&report, "f3"),
        2,
        "f3 should have 2 opacity keyframes (unhighlight sibling)"
    );

    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.severity != crate::diagnostics::DiagnosticSeverity::Error),
        "unexpected errors: {:?}",
        report.diagnostics
    );
}

#[test]
fn different_equations_independent() {
    // Two Equations, each with Fragments. Highlight in eqA should NOT
    // affect Fragments in eqB.
    let mut ast = make_two_equations();
    if let Stmt::Keyframe { body, .. } = &mut ast[0] {
        body.push(Stmt::Action(
            Action {
                verb: "highlight".to_string(),
                targets: vec!["fA1".to_string()],
                args: vec![],
                modifiers: vec![Modifier {
                    name: None,
                    value: Expr::Ident("800ms".to_string()),
                }],
                byte_span: None,
                target_index: vec![],
            },
            None,
        ));
    }

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    // fA1 should be highlighted
    assert_eq!(
        opacity_keyframe_count(&report, "fA1"),
        2,
        "fA1 should have 2 opacity keyframes (highlight in)"
    );

    // fA2 should be unhighlighted (sibling in same Equation)
    assert_eq!(
        opacity_keyframe_count(&report, "fA2"),
        2,
        "fA2 should have 2 opacity keyframes (unhighlight sibling)"
    );

    // fB1 and fB2 should be UNTOUCHED (different Equation)
    assert_eq!(
        opacity_keyframe_count(&report, "fB1"),
        0,
        "fB1 should have NO opacity keyframes"
    );
    assert_eq!(
        opacity_keyframe_count(&report, "fB2"),
        0,
        "fB2 should have NO opacity keyframes"
    );

    assert!(
        report
            .diagnostics
            .iter()
            .all(|d| d.severity != crate::diagnostics::DiagnosticSeverity::Error),
        "unexpected errors: {:?}",
        report.diagnostics
    );
}

#[test]
fn highlight_with_color_and_blend_no_unsupported_modifier_warning() {
    // `color` and `blend` are action-effect modifiers declared in highlight's signature;
    // parse_timing_modifiers must not emit UnsupportedModifierKey for them.
    let mut ast = make_equation_with_fragment();
    if let Stmt::Keyframe { body, .. } = &mut ast[0] {
        body.push(Stmt::Action(
            Action {
                verb: "highlight".to_string(),
                targets: vec!["f1".to_string()],
                args: vec![],
                modifiers: vec![
                    Modifier {
                        name: None,
                        value: Expr::Ident("800ms".to_string()),
                    },
                    Modifier {
                        name: Some("color".to_string()),
                        value: Expr::Ident("RED".to_string()),
                    },
                    Modifier {
                        name: Some("blend".to_string()),
                        value: Expr::Str("difference".to_string()),
                    },
                ],
                byte_span: None,
                target_index: vec![],
            },
            None,
        ));
    }

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    let unsupported_key_warnings = report
        .diagnostics
        .iter()
        .filter(|d| {
            d.code == animatix_syntax::diagnostics::DiagnosticCode::UnsupportedModifierKey
                && d.severity == crate::diagnostics::DiagnosticSeverity::Warning
        })
        .count();
    assert_eq!(
        unsupported_key_warnings, 0,
        "highlight with color/blend should not produce UnsupportedModifierKey warnings; got: {:?}",
        report.diagnostics
    );
}
