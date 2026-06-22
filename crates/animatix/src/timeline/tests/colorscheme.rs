use super::*;

#[test]
fn test_colorscheme_primitive_declaration() {
    let ast = vec![
        Stmt::LetDecl {
            is_pub: false,
            name: "test-scheme".to_string(),
            value: Expr::Construct(
                "Colorscheme".to_string(),
                vec![
                    Property {
                        name: "scene.background".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.1), Expr::Num(0.2), Expr::Num(0.3)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "text.primary".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.9), Expr::Num(0.95), Expr::Num(1.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
            ),
            span: None,
        },
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("test-scheme".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    assert_eq!(timeline.colorscheme.name, "test-scheme");
    assert_eq!(timeline.colorscheme.color("scene.background"), Some([0.1, 0.2, 0.3, 1.0]));
    assert_eq!(timeline.colorscheme.color("text.primary"), Some([0.9, 0.95, 1.0, 1.0]));
}

#[test]
fn test_colorscheme_let_declaration() {
    let ast = vec![
        Stmt::LetDecl {
            is_pub: false,
            name: "test-scheme-let".to_string(),
            value: Expr::Construct(
                "Colorscheme".to_string(),
                vec![
                    Property {
                        name: "scene.background".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.15), Expr::Num(0.25), Expr::Num(0.35)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "text.primary".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.85), Expr::Num(0.9), Expr::Num(0.95)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
            ),
            span: None,
        },
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("test-scheme-let".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    assert_eq!(timeline.colorscheme.name, "test-scheme-let");
    assert_eq!(timeline.colorscheme.color("scene.background"), Some([0.15, 0.25, 0.35, 1.0]));
    assert_eq!(timeline.colorscheme.color("text.primary"), Some([0.85, 0.9, 0.95, 1.0]));
}

#[test]
fn test_colorscheme_inheritance() {
    let ast = vec![
        Stmt::LetDecl {
            is_pub: false,
            name: "child".to_string(),
            value: Expr::Construct(
                "Colorscheme".to_string(),
                vec![
                    Property {
                        name: "extends".to_string(),
                        value: Expr::Str("default-dark".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "scene.background".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.5), Expr::Num(0.5), Expr::Num(0.5)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
            ),
            span: None,
        },
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("child".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    assert_eq!(timeline.colorscheme.name, "child");
    assert_eq!(timeline.colorscheme.color("scene.background"), Some([0.5, 0.5, 0.5, 1.0]));
    assert_eq!(timeline.colorscheme.color("text.primary"), Some([1.0, 1.0, 1.0, 1.0]));
}

#[test]
fn test_colorscheme_auto_cycle() {
    let ast = vec![
        Stmt::LetDecl {
            is_pub: false,
            name: "auto-test".to_string(),
            value: Expr::Construct(
                "Colorscheme".to_string(),
                vec![Property {
                    name: "auto".to_string(),
                    value: Expr::Tuple(vec![
                        Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(0.0), Expr::Num(0.0)]),
                        Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(1.0), Expr::Num(0.0)]),
                    ]),
                    value_span: None,
                    trailing_comment: None,
                }],
            ),
            span: None,
        },
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("auto-test".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            span: None,
        },
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "a".to_string(),
            array_index: None,
            ty: "Ellipse".to_string(),
            props: vec![Property {
                name: "color".to_string(),
                value: Expr::Ident("auto".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
        },
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "b".to_string(),
            array_index: None,
            ty: "Ellipse".to_string(),
            props: vec![Property {
                name: "color".to_string(),
                value: Expr::Ident("auto".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let mut timeline = report.output;

    let color_a = timeline.auto_color_for_label("a");
    let color_b = timeline.auto_color_for_label("b");

    assert_eq!(color_a, Some([1.0, 0.0, 0.0, 1.0]));
    assert_eq!(color_b, Some([0.0, 1.0, 0.0, 1.0]));
}
