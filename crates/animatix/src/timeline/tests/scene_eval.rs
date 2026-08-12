use super::*;

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
    assert!(
        cache.contains_key(&("box1".to_string(), DebugRenderOptions::default())),
        "cache should contain box1"
    );
    drop(cache);

    // Second evaluation at different time — should use cached encoding
    let _scene2 = timeline.evaluate_with_debug(1.0, dims, DebugRenderOptions::default(), &mut None);

    // Cache should still have entries
    let cache2 = timeline.static_subtree_cache.borrow();
    assert!(!cache2.is_empty(), "static subtree cache should still have entries");
}

#[test]
fn static_scene_cache_is_keyed_by_debug_options() {
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

    let _default =
        timeline.evaluate_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    let debug_opts = DebugRenderOptions {
        draw_bounds: true,
        ..DebugRenderOptions::default()
    };
    let _debug = timeline.evaluate_with_debug(0.0, dims, debug_opts, &mut None);

    let cache = timeline.static_subtree_cache.borrow();
    assert!(cache.contains_key(&("box1".to_string(), DebugRenderOptions::default())));
    assert!(cache.contains_key(&("box1".to_string(), debug_opts)));
    assert_eq!(cache.len(), 2, "debug options must not reuse the default scene");
}

#[test]
fn static_scene_cache_is_bypassed_for_hit_regions() {
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

    let _default =
        timeline.evaluate_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    let hit_opts = DebugRenderOptions {
        compute_hit_regions: true,
        ..DebugRenderOptions::default()
    };
    let _hit = timeline.evaluate_with_debug(0.0, dims, hit_opts, &mut None);

    assert!(
        !timeline.hit_regions().is_empty(),
        "hit-region evaluation must recompute regions instead of reusing a cached scene"
    );
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
                        target: vec![crate::ast::TargetSegment::Static("counter".to_string())],
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
fn runtime_empty_text_override_clears_stale_glyphs() {
    let ast = vec![Stmt::Keyframe {
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
                        value: Expr::Str("visible".to_string()),
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
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
            Stmt::Always {
                body: vec![Stmt::Assignment {
                    target: vec![crate::ast::TargetSegment::Static("counter".to_string())],
                    property: "text".to_string(),
                    value: Expr::Str("".to_string()),
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
    let timeline = report.output;
    let dimensions = SceneDimensions {
        width: 400,
        height: 200,
    };

    // The actor has build-time glyphs, but the runtime override is explicitly
    // empty. Evaluation must produce no text commands instead of silently
    // reusing the cached build-time content, so no precise bounds are recorded.
    let _scene = timeline.evaluate(0.0, dimensions);
    assert!(
        !timeline.precise_bounds_cache.borrow().contains_key("counter"),
        "empty runtime text override should clear visible content"
    );
}
