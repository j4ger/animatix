use super::*;
use crate::ast::{BinaryOp, Property};

#[test]
fn static_scene_cache_populated_after_first_evaluate() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            span: None,
        },
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "box1".to_string(),
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
                        name: "color".to_string(),
                        value: Expr::Ident("accent.primary".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    // First evaluation — should populate static subtree cache
    let _scene1 = timeline.evaluate_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);

    let cache = timeline.static_subtree_cache.borrow();
    assert!(
        !cache.is_empty(),
        "static subtree cache should be populated after first evaluate"
    );
    assert!(cache.contains_key("box1"), "cache should contain box1");
    drop(cache);

    // Second evaluation at different time — should use cached encoding
    let _scene2 = timeline.evaluate_with_debug(1.0, dims, DebugRenderOptions::default(), &mut None);

    // Cache should still have entries
    let cache2 = timeline.static_subtree_cache.borrow();
    assert!(!cache2.is_empty(), "static subtree cache should still have entries");
}

#[test]
fn static_scene_skips_frame_env() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            span: None,
        },
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "box1".to_string(),
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
                        name: "color".to_string(),
                        value: Expr::Ident("accent.primary".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    // Static scene with no modifiers or procedural plots should not need frame env
    let msg = format!(
        "static scene should not need frame env. modifiers={}, programs={}, bytecode={}, proc_plots={}",
        timeline.modifiers.len(),
        timeline.modifier_programs.len(),
        timeline.modifier_bytecode_programs.len(),
        timeline.has_procedural_plots()
    );
    assert!(!timeline.needs_frame_env(), "{}", msg);
}

#[test]
fn test_for_iter_values_supports_tuple_literals() {
    let env = Environment::new();
    let values =
        for_iter_values(&Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)]), &env);

    assert_eq!(values, vec![Value::Num(1.0), Value::Num(2.0), Value::Num(3.0)]);
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
            easing: None,
            value_span: None,
            span: None,
        }],
        else_branch: Some(vec![Stmt::Assignment {
            target: vec!["pulse".to_string()],
            property: "opacity".to_string(),
            value: Expr::Num(0.0),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }]),
        span: None,
    };

    let mut first_overrides = std::collections::HashMap::new();
    let mut first_env =
        timeline.build_frame_env_internal(500, SceneDimensions::default(), &first_overrides);
    timeline.apply_modifier_stmt(&modifier, &mut first_env, &mut first_overrides);

    let mut second_overrides = std::collections::HashMap::new();
    let mut second_env =
        timeline.build_frame_env_internal(1500, SceneDimensions::default(), &second_overrides);
    timeline.apply_modifier_stmt(&modifier, &mut second_env, &mut second_overrides);

    let mut repeat_overrides = std::collections::HashMap::new();
    let mut repeat_env =
        timeline.build_frame_env_internal(500, SceneDimensions::default(), &repeat_overrides);
    timeline.apply_modifier_stmt(&modifier, &mut repeat_env, &mut repeat_overrides);

    assert_eq!(first_overrides["pulse"]["opacity"], Value::Num(1.0));
    assert_eq!(second_overrides["pulse"]["opacity"], Value::Num(0.0));
    assert_eq!(first_overrides, repeat_overrides);
}

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

#[test]
fn test_runtime_text_recompilation() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            span: None,
        },
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "counter".to_string(),
                    array_index: None,
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
                        easing: None,
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
    let _scene_0s = timeline.evaluate(
        0.0,
        SceneDimensions {
            width: 400,
            height: 200,
        },
    );
    let _scene_1_5s = timeline.evaluate(
        1.5,
        SceneDimensions {
            width: 400,
            height: 200,
        },
    );

    // The text compiler should have cached entries for both times
    let cache_len = timeline.text_compiler.borrow().cache_len();
    assert!(
        cache_len >= 2,
        "TextCompiler should have at least 2 cache entries for different times, got {}",
        cache_len
    );
}

#[test]
fn test_keyframe_scoped_variables_create_tracks() {
    let ast = vec![
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::LetDecl {
                is_pub: false,
                name: "freq".to_string(),
                value: Expr::Num(1.0),
                span: None,
            }],
            span: None,
        },
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(3.0),
            body: vec![Stmt::LetDecl {
                is_pub: false,
                name: "freq".to_string(),
                value: Expr::Num(1.7),
                span: None,
            }],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    assert!(
        timeline.variable_tracks.contains_key("freq"),
        "Expected variable track for 'freq'"
    );
    let freq_track = timeline.variable_tracks.get("freq").unwrap();
    assert_eq!(freq_track.keyframes.len(), 2, "Expected 2 keyframes for freq");

    // At t=0s, freq should be 1.0
    let val_0s = freq_track.evaluate(0);
    assert!(val_0s.is_some(), "Expected freq value at t=0s");
    assert_eq!(val_0s.unwrap(), Value::Num(1.0));

    // At t=1.5s, freq should still be 1.0 (piecewise-constant)
    let val_1_5s = freq_track.evaluate(1500);
    assert!(val_1_5s.is_some(), "Expected freq value at t=1.5s");
    assert_eq!(val_1_5s.unwrap(), Value::Num(1.0));

    // At t=3s, freq should be 1.7
    let val_3s = freq_track.evaluate(3000);
    assert!(val_3s.is_some(), "Expected freq value at t=3s");
    assert_eq!(val_3s.unwrap(), Value::Num(1.7));

    // At t=5s, freq should still be 1.7
    let val_5s = freq_track.evaluate(5000);
    assert!(val_5s.is_some(), "Expected freq value at t=5s");
    assert_eq!(val_5s.unwrap(), Value::Num(1.7));
}

#[test]
fn test_animated_scene_has_keyframes() {
    let ast = vec![
        Stmt::Config {
            settings: vec![Property {
                name: "colorscheme".to_string(),
                value: Expr::Str("editorial-dark".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            span: None,
        },
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "box0".to_string(),
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
                        name: "color".to_string(),
                        value: Expr::Ident("accent.primary".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        },
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(5.0),
            body: vec![
                Stmt::Assignment {
                    target: vec!["box0".to_string()],
                    property: "position".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(200.0)]),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                },
                Stmt::Assignment {
                    target: vec!["box0".to_string()],
                    property: "color".to_string(),
                    value: Expr::Ident("accent.success".to_string()),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                },
                Stmt::Assignment {
                    target: vec!["box0".to_string()],
                    property: "opacity".to_string(),
                    value: Expr::Num(0.5),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                },
            ],
            span: None,
        },
    ];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let track = timeline.get_track("box0").expect("box0 should exist");
    assert!(track.has_any_keyframes(), "box0 should have animated keyframes");
    assert!(
        track.position.as_ref().map(|t| t.keyframes.len()).unwrap_or(0) >= 2,
        "position should have at least 2 keyframes"
    );
    assert!(
        track.color.as_ref().map(|t| t.keyframes.len()).unwrap_or(0) >= 2,
        "color should have at least 2 keyframes"
    );
    assert!(
        track.opacity.as_ref().map(|t| t.keyframes.len()).unwrap_or(0) >= 2,
        "opacity should have at least 2 keyframes"
    );
}

#[test]
fn test_keyframe_scoped_variables_injected_into_frame_env() {
    let ast = vec![Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "tracker".to_string(),
                array_index: None,
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(10.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
            Stmt::LetDecl {
                is_pub: false,
                name: "freq".to_string(),
                value: Expr::Num(2.0),
                span: None,
            },
            Stmt::Always {
                body: vec![Stmt::Assignment {
                    target: vec!["tracker".to_string()],
                    property: "at".to_string(),
                    value: Expr::Tuple(vec![
                        Expr::Binary(
                            Box::new(Expr::Num(640.0)),
                            BinaryOp::Add,
                            Box::new(Expr::Binary(
                                Box::new(Expr::Num(100.0)),
                                BinaryOp::Mul,
                                Box::new(Expr::Call(
                                    "cos".to_string(),
                                    vec![Expr::Binary(
                                        Box::new(Expr::Ident("freq".to_string())),
                                        BinaryOp::Mul,
                                        Box::new(Expr::Ident("t".to_string())),
                                    )],
                                )),
                            )),
                        ),
                        Expr::Binary(
                            Box::new(Expr::Num(360.0)),
                            BinaryOp::Add,
                            Box::new(Expr::Binary(
                                Box::new(Expr::Num(100.0)),
                                BinaryOp::Mul,
                                Box::new(Expr::Call(
                                    "sin".to_string(),
                                    vec![Expr::Binary(
                                        Box::new(Expr::Ident("freq".to_string())),
                                        BinaryOp::Mul,
                                        Box::new(Expr::Ident("t".to_string())),
                                    )],
                                )),
                            )),
                        ),
                    ]),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                }],
                span: None,
            },
        ],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    // Evaluate at t=0s — with freq=2.0, cos(2*0)=1, sin(2*0)=0
    let mut overrides = std::collections::HashMap::new();
    let mut env = timeline.build_frame_env_internal(
        0,
        SceneDimensions {
            width: 1280,
            height: 720,
        },
        &overrides,
    );
    for modifier in &timeline.modifiers {
        timeline.apply_modifier_stmt(modifier, &mut env, &mut overrides);
    }

    let tracker_at = overrides.get("tracker").and_then(|m| m.get("at"));
    assert!(
        tracker_at.is_some(),
        "Expected tracker.at override from always block using freq variable"
    );
    if let Some(Value::Vec2([x, y])) = tracker_at {
        assert!((x - 740.0).abs() < 0.1, "Expected x≈740, got {}", x);
        assert!((y - 360.0).abs() < 0.1, "Expected y≈360, got {}", y);
    } else {
        panic!("Expected Vec2 override for tracker.at, got {:?}", tracker_at);
    }
}

#[test]
fn test_reactive_binding_desugars_to_modifier() {
    let ast = vec![Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "orbiter".to_string(),
                array_index: None,
                ty: "Ellipse".to_string(),
                props: vec![
                    Property {
                        name: "radius".to_string(),
                        value: Expr::Num(10.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                    Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(0.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
            Stmt::ReactiveBinding {
                target: vec!["orbiter".to_string()],
                property: "at".to_string(),
                value: Expr::Tuple(vec![
                    Expr::Binary(
                        Box::new(Expr::Num(640.0)),
                        BinaryOp::Add,
                        Box::new(Expr::Binary(
                            Box::new(Expr::Num(100.0)),
                            BinaryOp::Mul,
                            Box::new(Expr::Call(
                                "cos".to_string(),
                                vec![Expr::Ident("t".to_string())],
                            )),
                        )),
                    ),
                    Expr::Binary(
                        Box::new(Expr::Num(360.0)),
                        BinaryOp::Add,
                        Box::new(Expr::Binary(
                            Box::new(Expr::Num(100.0)),
                            BinaryOp::Mul,
                            Box::new(Expr::Call(
                                "sin".to_string(),
                                vec![Expr::Ident("t".to_string())],
                            )),
                        )),
                    ),
                ]),
                value_span: None,
                span: None,
            },
        ],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    // The reactive binding should have been desugared to a modifier
    assert!(
        !timeline.modifiers.is_empty(),
        "Expected modifiers from reactive binding desugaring"
    );

    // Evaluate at t=0s — orbiter should be at (740, 360)
    let mut overrides = std::collections::HashMap::new();
    let mut env = timeline.build_frame_env_internal(
        0,
        SceneDimensions {
            width: 1280,
            height: 720,
        },
        &overrides,
    );
    for modifier in &timeline.modifiers {
        timeline.apply_modifier_stmt(modifier, &mut env, &mut overrides);
    }

    let orbiter_at = overrides.get("orbiter").and_then(|m| m.get("at"));
    assert!(orbiter_at.is_some(), "Expected orbiter.at override from reactive binding");
    if let Some(Value::Vec2([x, y])) = orbiter_at {
        assert!((x - 740.0).abs() < 0.1, "Expected x≈740, got {}", x);
        assert!((y - 360.0).abs() < 0.1, "Expected y≈360, got {}", y);
    } else {
        panic!("Expected Vec2 override for orbiter.at, got {:?}", orbiter_at);
    }
}

#[test]
fn test_hierarchical_assignment_target() {
    let source = r#"
        g: Graph {
            circ: Ellipse {
                at: (0, 0),
                radius: 10,
            }
        }

        #+1s
        g.circ.opacity = 0.5
    "#;

    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    assert!(
        report.diagnostics.is_empty(),
        "Expected no build diagnostics, got: {:?}",
        report.diagnostics
    );

    let timeline = report.output;

    // At t=0s, circ.opacity should be 0.0 (pre-keyframe default is hidden)
    let circ_track = timeline.tracks.get("circ").expect("circ track should exist");
    let opacity_at_0 = circ_track.opacity.as_ref().unwrap().evaluate(0);
    assert!(
        (opacity_at_0 - 0.0).abs() < 0.01,
        "Expected circ.opacity=0.0 at t=0 (pre-keyframe default), got {:?}",
        opacity_at_0
    );

    // At t=1s, circ.opacity should be 0.5
    let opacity_at_1s = circ_track.opacity.as_ref().unwrap().evaluate(1000);
    assert!(
        (opacity_at_1s - 0.5).abs() < 0.01,
        "Expected circ.opacity=0.5 at t=1s, got {:?}",
        opacity_at_1s
    );
}

#[test]
fn graph_axes_invisible_before_fadein() {
    // Graph declared before any keyframe → default_opacity = 0.0
    // fade-in at #0.5s should animate opacity 0→1
    let source = "g1: Graph, x_domain: (-4, 4), y_domain: (-2, 18), size: (380, 280), at: (280, 200)\n\n#0.5s\nfade-in g1 [400ms]";
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report =
        crate::timeline::Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;

    let track = timeline.tracks.get("g1").expect("g1 track should exist");
    let opacity_at_0 = track.opacity.as_ref().map(|t| t.evaluate(0));
    let opacity_at_500 = track.opacity.as_ref().map(|t| t.evaluate(500));
    let opacity_at_900 = track.opacity.as_ref().map(|t| t.evaluate(900));
    let opacity_at_1000 = track.opacity.as_ref().map(|t| t.evaluate(1000));

    assert_eq!(opacity_at_0, Some(0.0), "opacity should be 0 at t=0");
    assert_eq!(opacity_at_500, Some(0.0), "opacity should be 0 at t=500ms (fade-in start)");
    assert_eq!(opacity_at_900, Some(1.0), "opacity should be 1 at t=900ms (fade-in end)");
    assert_eq!(opacity_at_1000, Some(1.0), "opacity should stay 1 after fade-in");
}

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

#[test]
fn equation_container_builds_with_fragment_children() {
    let source = r#"
        eq: Equation {
            f1: Fragment, content: "x^2"
            f2: Fragment, content: "+ y"
        }
    "#;

    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    // Equation container track should exist
    let eq_track = report.output.tracks.get("eq").expect("eq track should exist");

    // Equation should have Fragment children registered
    assert!(
        eq_track.children.contains(&"f1".to_string()),
        "Equation track should contain child 'f1', got: {:?}",
        eq_track.children
    );
    assert!(
        eq_track.children.contains(&"f2".to_string()),
        "Equation track should contain child 'f2', got: {:?}",
        eq_track.children
    );

    // Fragment f1 track should exist with content stored
    let f1_track = report.output.tracks.get("f1").expect("f1 track should exist");
    let f1_content = f1_track
        .text_content
        .as_ref()
        .expect("f1 should have text_content")
        .evaluate(0);
    assert_eq!(f1_content, "x^2", "Expected f1 content 'x^2', got {:?}", f1_content);

    // Fragment f2 track should exist with content stored
    let f2_track = report.output.tracks.get("f2").expect("f2 track should exist");
    let f2_content = f2_track
        .text_content
        .as_ref()
        .expect("f2 should have text_content")
        .evaluate(0);
    assert_eq!(f2_content, "+ y", "Expected f2 content '+ y', got {:?}", f2_content);
}

#[test]
fn equation_fragment_dot_path_assignment() {
    let source = r#"
        eq: Equation {
            f1: Fragment, content: "x^2"
            f2: Fragment, content: "+ y"
        }

        #+1s
        eq.f1.highlight_opacity = 1.0 [800ms]
        eq.f2.content = "+ z"
    "#;

    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

    // Filter out non-error diagnostics (e.g. deprecation warnings)
    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "Expected no build errors, got: {:?}",
        errors
    );

    let timeline = report.output;

    // Fragment f1 should have highlight_opacity animated
    let f1_track = timeline.tracks.get("f1").expect("f1 track should exist");
    let highlight_opacity_at_1s = f1_track
        .highlight_opacity
        .as_ref()
        .expect("f1 should have highlight_opacity track")
        .evaluate(1000);
    assert!(
        (highlight_opacity_at_1s - 0.0).abs() < 0.01,
        "Expected highlight_opacity=0.0 at t=1s (animation start), got {:?}",
        highlight_opacity_at_1s
    );

    let highlight_opacity_at_end = f1_track
        .highlight_opacity
        .as_ref()
        .expect("f1 should have highlight_opacity track")
        .evaluate(1800);
    assert!(
        (highlight_opacity_at_end - 1.0).abs() < 0.01,
        "Expected highlight_opacity=1.0 at t=1.8s (animation end), got {:?}",
        highlight_opacity_at_end
    );

    // Fragment f2 should have updated content
    let f2_track = timeline.tracks.get("f2").expect("f2 track should exist");
    let f2_content_at_1s = f2_track
        .text_content
        .as_ref()
        .expect("f2 should have text_content")
        .evaluate(1000);
    assert_eq!(
        f2_content_at_1s, "+ z",
        "Expected f2 content '+ z' at t=1s, got {:?}",
        f2_content_at_1s
    );
}

#[test]
fn test_container_metadata_gap_helpers() {
    // gap_uniform creates a uniform [f32; 2] from a scalar
    let g = gap_uniform(10.0);
    assert_eq!(g, [10.0, 10.0]);

    // padding_uniform creates a uniform [f32; 4] from a scalar
    let p = padding_uniform(8.0);
    assert_eq!(p, [8.0, 8.0, 8.0, 8.0]);
}

#[test]
fn test_stack_align_start_and_end() {
    use crate::timeline::layout::ChildExtent;
    use crate::timeline::PlacementMode;

    let children = vec![
        ChildExtent {
            label: "a".to_string(),
            half_size: [50.0, 30.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
        ChildExtent {
            label: "b".to_string(),
            half_size: [40.0, 20.0],
            placement_mode: PlacementMode::LayoutManaged,
        },
    ];

    // "center" alignment — all children at origin
    let positions_center = crate::timeline::layout::compute_stack_layout(&children, "center");
    assert_eq!(positions_center[0], [0.0, 0.0]);
    assert_eq!(positions_center[1], [0.0, 0.0]);

    // "start" alignment — shift toward negative
    let positions_start = crate::timeline::layout::compute_stack_layout(&children, "start");
    assert_eq!(positions_start[0], [-50.0, -30.0]);
    assert_eq!(positions_start[1], [-40.0, -20.0]);

    // "end" alignment — shift toward positive
    let positions_end = crate::timeline::layout::compute_stack_layout(&children, "end");
    assert_eq!(positions_end[0], [50.0, 30.0]);
    assert_eq!(positions_end[1], [40.0, 20.0]);
}

#[test]
fn test_baseline_alignment_via_layout_engine() {
    // Integration test: LayoutEngine::compute_positions_with_baselines
    use crate::timeline::layout::ChildExtent;
    use crate::timeline::{ContainerMetadata, LayoutEngine, LayoutType, PlacementMode};

    let children = vec![
        ChildExtent { label: "a".to_string(), half_size: [50.0, 30.0], placement_mode: PlacementMode::LayoutManaged },
        ChildExtent { label: "b".to_string(), half_size: [40.0, 20.0], placement_mode: PlacementMode::LayoutManaged },
    ];

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [0.0, 0.0],
        padding: [0.0, 0.0, 0.0, 0.0],
        align: "center".to_string(),
        vertical_align: "baseline".to_string(),
        cols: None,
        child_order: vec!["a".to_string(), "b".to_string()],
    };

    // Baseline alignment
    let child_baselines = vec![-8.0, -4.0];
    let positions = LayoutEngine::compute_positions_with_baselines(
        &metadata, &children, &child_baselines,
    );

    assert_eq!(positions.len(), 2);
    // Baselines should differ from center-aligned positions
    // The child with baseline=-8 (larger offset from center) should adjust more
    assert!(
        (positions[0][1]).abs() > 0.01 || (positions[1][1]).abs() > 0.01,
        "Baseline alignment should produce non-zero Y adjustments"
    );

    // With empty baselines, should behave like center
    let positions_no_baselines = LayoutEngine::compute_positions_with_baselines(
        &metadata, &children, &[],
    );
    assert!((positions_no_baselines[0][1]).abs() < 0.01);
    assert!((positions_no_baselines[1][1]).abs() < 0.01);

    // Center vertical_align should not adjust Y
    let metadata_center = ContainerMetadata {
        vertical_align: "center".to_string(),
        ..metadata.clone()
    };
    let positions_center = LayoutEngine::compute_positions_with_baselines(
        &metadata_center, &children, &child_baselines,
    );
    assert!((positions_center[0][1]).abs() < 0.01);
    assert!((positions_center[1][1]).abs() < 0.01);
}

// ── Phase 7: Percentage & intrinsic sizing tests ──

#[test]
fn test_percentage_child_sizing_row() {
    use crate::timeline::layout::ChildExtent;
    // Row with two children: a at 50% width, b fills remainder
    let children = vec![
        ChildExtent { label: "a".into(), half_size: [50.0, 25.0], placement_mode: PlacementMode::LayoutManaged },
        ChildExtent { label: "b".into(), half_size: [50.0, 25.0], placement_mode: PlacementMode::LayoutManaged },
    ];
    let specs = vec![
        Some(crate::timeline::taffy_layout::ChildSizeSpec::from_parts(
            crate::timeline::taffy_layout::SizeSpec::Percent(0.5),
            crate::timeline::taffy_layout::SizeSpec::Fixed(50.0),
        )),
        Some(crate::timeline::taffy_layout::ChildSizeSpec::from_parts(
            crate::timeline::taffy_layout::SizeSpec::Fill,
            crate::timeline::taffy_layout::SizeSpec::Fixed(50.0),
        )),
    ];
    let constraints = vec![
        crate::timeline::taffy_layout::SizeConstraints::default(),
        crate::timeline::taffy_layout::SizeConstraints::default(),
    ];

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [0.0, 0.0],
        padding: [0.0, 0.0, 0.0, 0.0],
        align: "start".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec!["a".into(), "b".into()],
    };

    let positions = crate::timeline::LayoutEngine::compute_positions_with_specs(
        &metadata, &children, &[], &specs, &constraints, [400.0, 100.0],
    );

    assert_eq!(positions.len(), 2);
    // a (50%) should be at the start (left half of container), b (fill) should take remaining
    // In center-relative coords: a starts at left edge (x=-200), center is at x=-100
    assert!((positions[0][0] - (-100.0)).abs() < 5.0,
        "Child a (50%) expected x~-100 (left of center), got {}", positions[0][0]);
    assert!((positions[1][0] - (-0.0)).abs() < 5.0 || positions[1][0] > positions[0][0],
        "Child b (fill) should be after child a");
}

#[test]
fn test_min_max_constraints() {
    use crate::timeline::layout::ChildExtent;
    // Child with min_width: 100, max_width: 200
    let children = vec![
        ChildExtent { label: "a".into(), half_size: [150.0, 25.0], placement_mode: PlacementMode::LayoutManaged },
        ChildExtent { label: "b".into(), half_size: [150.0, 25.0], placement_mode: PlacementMode::LayoutManaged },
    ];
    let specs = vec![None, None];
    let constraints = vec![
        crate::timeline::taffy_layout::SizeConstraints {
            min_width: Some(100.0),
            max_width: Some(200.0),
            min_height: None,
            max_height: None,
        },
        crate::timeline::taffy_layout::SizeConstraints::default(),
    ];

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [10.0, 0.0],
        padding: [0.0, 0.0, 0.0, 0.0],
        align: "start".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec!["a".into(), "b".into()],
    };

    let positions = crate::timeline::LayoutEngine::compute_positions_with_specs(
        &metadata, &children, &[], &specs, &constraints, [500.0, 100.0],
    );

    assert_eq!(positions.len(), 2);
    // Child a has max_width: 200, so its actual width should be clamped at 200
    // Child b is 300 (150*2) which is within [0, inf)
    // Container width should be roughly 200 + 10 + 300 = 510
    assert!(positions[1][0] - positions[0][0] > 200.0,
        "Child a and b should be spaced apart");
}

#[test]
fn test_parse_size_spec_from_property() {
    use crate::timeline::taffy_layout::{parse_size_spec, SizeSpec};
    use crate::ast::Expr;

    // size: (50%, 40)
    let spec = parse_size_spec(&Expr::Tuple(vec![
        Expr::Str("50%".into()),
        Expr::Num(40.0),
    ]));
    assert_eq!(spec.width, SizeSpec::Percent(0.5));
    assert_eq!(spec.height, SizeSpec::Fixed(40.0));

    // size: fill
    let spec = parse_size_spec(&Expr::Ident("fill".into()));
    assert_eq!(spec.width, SizeSpec::Fill);

    // size: auto
    let spec = parse_size_spec(&Expr::Ident("auto".into()));
    assert_eq!(spec.width, SizeSpec::Auto);
}

// ────────────────────────────────────────────────────────
// 4.7: keyframe_times_s tests
// ────────────────────────────────────────────────────────

fn keyframe_times_s_timeline() -> Timeline {
    let mut timeline = Timeline::new();
    // Remove the default background_color keyframe at 0 so it doesn't pollute results
    timeline.background_color.keyframes_mut().clear();
    timeline
}

#[test]
fn test_keyframe_times_s_collects_all_fields() {
    let mut timeline = keyframe_times_s_timeline();
    let mut track = AnimationTrack::new("test".to_string());

    // Add keyframes to various fields
    track.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);
    track.position.ensure([0.0, 0.0]).add_keyframe(2000, [100.0, 0.0], Easing::Linear);
    track.transform.ensure([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
        .add_keyframe(3000, [2.0, 0.0, 0.0, 2.0, 0.0, 0.0], Easing::Linear);

    timeline.tracks.insert("test".to_string(), track);
    let times = timeline.keyframe_times_s();
    // Times in seconds: 1.0 (opacity), 2.0 (position), 3.0 (transform)
    assert_eq!(times.len(), 3, "Got: {:?}", times);
    assert!(times.contains(&1.0));
    assert!(times.contains(&2.0));
    assert!(times.contains(&3.0));
}

#[test]
fn test_keyframe_times_s_includes_highlight_fields() {
    let mut timeline = keyframe_times_s_timeline();
    // Highlight fields apply to Equation/Fragment actors
    let mut track = AnimationTrack::new("test".to_string());
    track.kind = ActorKindId::Equation;

    track.highlight_color.ensure([0.3, 0.5, 1.0, 1.0])
        .add_keyframe(500, [1.0, 0.0, 0.0, 1.0], Easing::Linear);
    track.highlight_opacity.ensure(0.0).add_keyframe(2500, 0.8, Easing::Linear);

    timeline.tracks.insert("test".to_string(), track);
    let times = timeline.keyframe_times_s();
    assert!(times.contains(&0.5), "Got: {:?}", times);
    assert!(times.contains(&2.5), "Got: {:?}", times);
}

#[test]
fn test_keyframe_times_s_returns_unique_times() {
    let mut timeline = keyframe_times_s_timeline();
    let mut track_a = AnimationTrack::new("a".to_string());
    track_a.opacity.ensure(1.0).add_keyframe(1000, 0.5, Easing::Linear);

    let mut track_b = AnimationTrack::new("b".to_string());
    track_b.opacity.ensure(1.0).add_keyframe(1000, 0.0, Easing::Linear);

    timeline.tracks.insert("a".to_string(), track_a);
    timeline.tracks.insert("b".to_string(), track_b);
    let times = timeline.keyframe_times_s();
    // Both tracks have the same keyframe time (1000ms = 1.0s)
    assert_eq!(times.len(), 1, "Should have unique times, got: {:?}", times);
    assert!((times[0] - 1.0).abs() < 0.001);
}

#[test]
fn test_keyframe_times_s_returns_seconds_not_milliseconds() {
    let mut timeline = keyframe_times_s_timeline();
    let mut track = AnimationTrack::new("test".to_string());
    track.opacity.ensure(1.0).add_keyframe(5000, 0.5, Easing::Linear);
    timeline.tracks.insert("test".to_string(), track);
    let times = timeline.keyframe_times_s();
    assert!(!times.contains(&5000.0), "Should be in seconds, not milliseconds");
    assert!(times.contains(&5.0), "5000ms should be 5.0s, got: {:?}", times);
}

#[test]
fn test_keyframe_times_s_includes_background_color() {
    let mut timeline = keyframe_times_s_timeline();
    timeline.background_color.add_keyframe(3000, [1.0, 0.0, 0.0, 1.0], Easing::Linear);
    let times = timeline.keyframe_times_s();
    assert!(times.contains(&3.0), "Got: {:?}", times);
}

#[test]
fn test_keyframe_times_s_includes_filter_fields() {
    let mut timeline = keyframe_times_s_timeline();
    // Filter fields apply to Filter actor kind
    let mut track = AnimationTrack::new("test".to_string());
    track.kind = ActorKindId::Filter;
    track.filter_brightness.ensure(1.0).add_keyframe(500, 2.0, Easing::Linear);
    track.filter_contrast.ensure(1.0).add_keyframe(1200, 1.5, Easing::Linear);
    track.filter_saturate.ensure(1.0).add_keyframe(800, 0.0, Easing::Linear);
    timeline.tracks.insert("test".to_string(), track);
    let times = timeline.keyframe_times_s();
    assert!(times.contains(&0.5));
    assert!(times.contains(&0.8));
    assert!(times.contains(&1.2));
}

#[test]
fn test_keyframe_times_s_includes_plot_param_tracks() {
    let mut timeline = keyframe_times_s_timeline();
    let mut track = AnimationTrack::new("test".to_string());
    track.plot_param_tracks.entry("freq".to_string())
        .or_insert_with(|| PropertyTrack::new(1.0))
        .add_keyframe(2000, 2.0, Easing::Linear);
    timeline.tracks.insert("test".to_string(), track);
    let times = timeline.keyframe_times_s();
    assert!(times.contains(&2.0));
}

#[test]
fn test_keyframe_times_s_empty_when_no_keyframes() {
    let timeline = keyframe_times_s_timeline();
    let times = timeline.keyframe_times_s();
    assert!(times.is_empty());
}

fn test_fixed_size_layout_still_works() {
    use crate::timeline::layout::ChildExtent;
    // Backward compatibility: fixed-size layout should work unchanged
    let children = vec![
        ChildExtent { label: "a".into(), half_size: [50.0, 25.0], placement_mode: PlacementMode::LayoutManaged },
        ChildExtent { label: "b".into(), half_size: [50.0, 25.0], placement_mode: PlacementMode::LayoutManaged },
    ];

    let metadata = ContainerMetadata {
        layout_type: LayoutType::Row,
        gap: [10.0, 10.0],
        padding: [5.0, 5.0, 5.0, 5.0],
        align: "center".to_string(),
        vertical_align: "center".to_string(),
        cols: None,
        child_order: vec!["a".into(), "b".into()],
    };

    // Legacy path (no specs/constraints)
    let positions = crate::timeline::LayoutEngine::compute_positions(&metadata, &children);

    assert_eq!(positions.len(), 2);
    // Two 100-wide children with 10 gap + 10 padding → total width = 100+10+100+10 = 220
    // a at -110 (left of center), b at 0 (center), actually let's just verify they're reasonable
    assert!(positions[0][0] < 0.0, "First child should be left of center");
    assert!(positions[1][0] > 0.0, "Second child should be right of center");

    // With specs/constraints (empty), should produce same result
    let positions_with_specs = crate::timeline::LayoutEngine::compute_positions_with_specs(
        &metadata, &children, &[], &[], &[], [0.0, 0.0],
    );
    assert_eq!(positions.len(), positions_with_specs.len());
    for i in 0..positions.len() {
        assert!(
            (positions[i][0] - positions_with_specs[i][0]).abs() < 0.01 &&
            (positions[i][1] - positions_with_specs[i][1]).abs() < 0.01,
            "Position mismatch at index {}: {:?} vs {:?}",
            i, positions[i], positions_with_specs[i]
        );
    }
}
