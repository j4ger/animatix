use super::*;
use crate::ast::{BinaryOp, Property};

#[test]
fn test_for_iter_values_supports_tuple_literals() {
    let env = Environment::new();
    let values = for_iter_values(
        &Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)]),
        &env,
    );

    assert_eq!(
        values,
        vec![Value::Num(1.0), Value::Num(2.0), Value::Num(3.0)]
    );
}

#[test]
fn test_apply_modifier_stmt_supports_conditionals_statelessly() {
    let mut timeline = Timeline::new();
    load_standard_library(&mut timeline.env);

    let modifier = Stmt::Conditional {
        condition: Expr::Binary(
            Box::new(Expr::Ident("t".to_string())),
            BinaryOp::Lt,
            Box::new(Expr::Num(1.0)),
        ),
        then_branch: vec![Stmt::Assignment {
            target: vec!["pulse".to_string()],
            property: "opacity".to_string(),
            value: Expr::Num(1.0),
            modifiers: vec![],
            value_span: None,
            span: None,
        }],
        else_branch: Some(vec![Stmt::Assignment {
            target: vec!["pulse".to_string()],
            property: "opacity".to_string(),
            value: Expr::Num(0.0),
            modifiers: vec![],
            value_span: None,
            span: None,
        }]),
        span: None,
    };

    let mut first_overrides = std::collections::HashMap::new();
    let mut first_env =
        timeline.frame_eval_env(500, SceneDimensions::default(), &first_overrides);
    timeline.apply_modifier_stmt(
        &modifier,
        500,
        SceneDimensions::default(),
        &mut first_env,
        &mut first_overrides,
    );

    let mut second_overrides = std::collections::HashMap::new();
    let mut second_env =
        timeline.frame_eval_env(1500, SceneDimensions::default(), &second_overrides);
    timeline.apply_modifier_stmt(
        &modifier,
        1500,
        SceneDimensions::default(),
        &mut second_env,
        &mut second_overrides,
    );

    let mut repeat_overrides = std::collections::HashMap::new();
    let mut repeat_env =
        timeline.frame_eval_env(500, SceneDimensions::default(), &repeat_overrides);
    timeline.apply_modifier_stmt(
        &modifier,
        500,
        SceneDimensions::default(),
        &mut repeat_env,
        &mut repeat_overrides,
    );

    assert_eq!(first_overrides["pulse"]["opacity"], Value::Num(1.0));
    assert_eq!(second_overrides["pulse"]["opacity"], Value::Num(0.0));
    assert_eq!(first_overrides, repeat_overrides);
}

#[test]
fn test_colorscheme_primitive_declaration() {
    let ast = vec![
        Stmt::LetDecl { is_pub: false,
            name: "test-scheme".to_string(),
            value: Expr::Construct(
                "Colorscheme".to_string(),
                vec![
                    Property {
                        name: "scene.background".to_string(),
                        value: Expr::Tuple(vec![
                            Expr::Num(0.1),
                            Expr::Num(0.2),
                            Expr::Num(0.3),
                        ]),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "text.primary".to_string(),
                        value: Expr::Tuple(vec![
                            Expr::Num(0.9),
                            Expr::Num(0.95),
                            Expr::Num(1.0),
                        ]),
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
    assert_eq!(
        timeline.colorscheme.color("scene.background"),
        Some([0.1, 0.2, 0.3, 1.0])
    );
    assert_eq!(
        timeline.colorscheme.color("text.primary"),
        Some([0.9, 0.95, 1.0, 1.0])
    );
}

#[test]
fn test_colorscheme_let_declaration() {
    let ast = vec![
        Stmt::LetDecl { is_pub: false,
            name: "test-scheme-let".to_string(),
            value: Expr::Construct(
                "Colorscheme".to_string(),
                vec![
                    Property {
                        name: "scene.background".to_string(),
                        value: Expr::Tuple(vec![
                            Expr::Num(0.15),
                            Expr::Num(0.25),
                            Expr::Num(0.35),
                        ]),
                        value_span: None,
                    trailing_comment: None,
                    },
                    Property {
                        name: "text.primary".to_string(),
                        value: Expr::Tuple(vec![
                            Expr::Num(0.85),
                            Expr::Num(0.9),
                            Expr::Num(0.95),
                        ]),
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
    assert_eq!(
        timeline.colorscheme.color("scene.background"),
        Some([0.15, 0.25, 0.35, 1.0])
    );
    assert_eq!(
        timeline.colorscheme.color("text.primary"),
        Some([0.85, 0.9, 0.95, 1.0])
    );
}

#[test]
fn test_colorscheme_inheritance() {
    let ast = vec![
        Stmt::LetDecl { is_pub: false,
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
                        value: Expr::Tuple(vec![
                            Expr::Num(0.5),
                            Expr::Num(0.5),
                            Expr::Num(0.5),
                        ]),
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
    assert_eq!(
        timeline.colorscheme.color("scene.background"),
        Some([0.5, 0.5, 0.5, 1.0])
    );
    assert_eq!(
        timeline.colorscheme.color("text.primary"),
        Some([1.0, 1.0, 1.0, 1.0])
    );
}

#[test]
fn test_colorscheme_auto_cycle() {
    let ast = vec![
        Stmt::LetDecl { is_pub: false,
            name: "auto-test".to_string(),
            value: Expr::Construct(
                "Colorscheme".to_string(),
                vec![
                    Property {
                        name: "auto".to_string(),
                        value: Expr::Tuple(vec![
                            Expr::Tuple(vec![
                                Expr::Num(1.0),
                                Expr::Num(0.0),
                                Expr::Num(0.0),
                            ]),
                            Expr::Tuple(vec![
                                Expr::Num(0.0),
                                Expr::Num(1.0),
                                Expr::Num(0.0),
                            ]),
                        ]),
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
                value: Expr::Str("auto-test".to_string()),
                value_span: None,
            trailing_comment: None,
            }],
            span: None,
        },
        Stmt::ActorDecl {
            is_pub: false,
            label: "a".to_string(),
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
            label: "b".to_string(),
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

#[test]
fn test_runtime_text_recompilation() {
    let ast = vec![
        Stmt::Config {
            settings: vec![
                Property {
                    name: "colorscheme".to_string(),
                    value: Expr::Str("editorial-dark".to_string()),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            span: None,
        },
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "counter".to_string(),
                    ty: "Text".to_string(),
                    props: vec![
                        Property {
                            name: "text".to_string(),
                            value: Expr::Str("0.00".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "font_size".to_string(),
                            value: Expr::Num(48.0),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "font_family".to_string(),
                            value: Expr::Str("Open Sans".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "color".to_string(),
                            value: Expr::Tuple(vec![
                                Expr::Num(1.0),
                                Expr::Num(1.0),
                                Expr::Num(1.0),
                                Expr::Num(1.0),
                            ]),
                            value_span: None,
                            trailing_comment: None,
                        },
                    ],
                    modifiers: vec![],
                    children: vec![],
                    span: None,
                },
                Stmt::Always {
                    body: vec![Stmt::Assignment {
                        target: vec!["counter".to_string()],
                        property: "text".to_string(),
                        value: Expr::Call(
                            "format".to_string(),
                            vec![Expr::Str("t={}".to_string()), Expr::Ident("t".to_string())],
                        ),
                        modifiers: vec![],
                        value_span: None,
                        span: None,
                    }],
                    span: None,
                },
            ],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    // Evaluate at t=0s and t=1.5s
    let _scene_0s = timeline.evaluate(0.0, SceneDimensions { width: 400, height: 200 });
    let _scene_1_5s = timeline.evaluate(1.5, SceneDimensions { width: 400, height: 200 });

    // The text compiler should have cached entries for both times
    let cache_len = timeline.text_compiler.borrow().cache_len();
    assert!(
        cache_len >= 2,
        "TextCompiler should have at least 2 cache entries for different times, got {}",
        cache_len
    );
}
