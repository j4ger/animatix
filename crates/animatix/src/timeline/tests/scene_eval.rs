use super::*;

/// Content-lint warnings (`never-revealed`) fire on minimal fixtures whose
/// actors have no entrance actions — they are about demo content, not the
/// feature under test, so assertions exclude them.
fn without_content_lints(
    diagnostics: &[animatix_syntax::diagnostics::Diagnostic],
) -> impl Iterator<Item = &animatix_syntax::diagnostics::Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| !matches!(d.code, animatix_syntax::diagnostics::DiagnosticCode::NeverRevealed))
}

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

    let cache = timeline.eval_caches.static_subtree_cache.borrow();
    assert!(
        !cache.is_empty(),
        "static subtree cache should be populated after first evaluate"
    );
    assert!(
        cache.contains_key(&("box1".to_string(), dims, false, DebugRenderOptions::default())),
        "cache should contain box1"
    );
    drop(cache);

    // Second evaluation at different time — should use cached encoding
    let _scene2 = timeline.evaluate_with_debug(1.0, dims, DebugRenderOptions::default(), &mut None);

    // Cache should still have entries
    let cache2 = timeline.eval_caches.static_subtree_cache.borrow();
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

    let cache = timeline.eval_caches.static_subtree_cache.borrow();
    assert!(cache.contains_key(&("box1".to_string(), dims, false, DebugRenderOptions::default())));
    assert!(cache.contains_key(&("box1".to_string(), dims, false, debug_opts)));
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
fn static_scene_only_cache_does_not_collect_items() {
    let ast = vec![Stmt::Keyframe {
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
                    value: Expr::Ident("red".to_string()),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    let _scene = timeline.evaluate_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    let cache = timeline.eval_caches.static_subtree_cache.borrow();
    let scene_only_key = ("box1".to_string(), dims, false, DebugRenderOptions::default());
    let (_, _, items, _) =
        cache.get(&scene_only_key).expect("scene-only static subtree should be cached");
    assert!(items.is_empty(), "scene-only evaluation must not collect SceneItems");
    assert!(
        !cache.contains_key(&("box1".to_string(), dims, true, DebugRenderOptions::default())),
        "scene-only evaluation must not populate the program cache entry"
    );
}

#[test]
fn static_program_after_scene_only_still_collects_items() {
    let ast = vec![Stmt::Keyframe {
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
                    value: Expr::Ident("red".to_string()),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    let _scene = timeline.evaluate_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    let program =
        timeline.evaluate_program_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    assert!(
        !program.items.is_empty(),
        "program path must collect items after scene-only cache"
    );

    let cache = timeline.eval_caches.static_subtree_cache.borrow();
    let program_key = ("box1".to_string(), dims, true, DebugRenderOptions::default());
    let (_, _, items, _) = cache
        .get(&program_key)
        .expect("program static subtree should be cached separately");
    assert!(!items.is_empty());
}

#[test]
fn static_program_entries_are_stable_on_repeat_hits() {
    let ast = vec![Stmt::Keyframe {
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
                    value: Expr::Ident("red".to_string()),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    let first =
        timeline.evaluate_program_with_debug(0.0, dims, DebugRenderOptions::default(), &mut None);
    let second =
        timeline.evaluate_program_with_debug(1.0, dims, DebugRenderOptions::default(), &mut None);
    assert_eq!(first.items.len(), second.items.len());
    assert_eq!(first.items[0].transform, second.items[0].transform);
    assert_eq!(first.items[0].opacity, second.items[0].opacity);
}

#[test]
fn static_scene_cache_is_keyed_by_dimensions() {
    let ast = vec![Stmt::Keyframe {
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
                    value: Expr::Ident("red".to_string()),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }],
        span: None,
    }];

    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    let timeline = report.output;
    let dims_1 = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let dims_2 = SceneDimensions {
        width: 1280,
        height: 720,
    };

    let _first =
        timeline.evaluate_with_debug(0.0, dims_1, DebugRenderOptions::default(), &mut None);
    let _second =
        timeline.evaluate_with_debug(0.0, dims_2, DebugRenderOptions::default(), &mut None);

    let cache = timeline.eval_caches.static_subtree_cache.borrow();
    assert!(cache.contains_key(&(
        "box1".to_string(),
        dims_1,
        false,
        DebugRenderOptions::default()
    )));
    assert!(cache.contains_key(&(
        "box1".to_string(),
        dims_2,
        false,
        DebugRenderOptions::default()
    )));
    assert_eq!(cache.len(), 2, "dimensions must not reuse a static subtree scene");
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
        "static scene should not need frame env. modifiers={}, programs={}, proc_plots={}",
        timeline.modifiers.len(),
        timeline.modifier_programs.len(),
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

    // The compile cache is process-wide; start from a clean slate so the
    // entry count below only reflects this test's compilations (lib tests run
    // with --test-threads=1).
    crate::renderer::text::clear_text_compile_cache();

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

    // Both evaluated times should have memoized their distinct text contents
    let cache_len = crate::renderer::text::text_compile_cache_len();
    assert!(
        cache_len >= 2,
        "Text compile cache should have at least 2 entries for different times, got {}",
        cache_len
    );
}

#[test]
fn equation_frames_reuse_one_typst_compilation() {
    // The Equation path aggregates its Fragment children into one Typst
    // document on *every* frame. Nothing about that body changes while
    // scrubbing, so all frames must collapse onto one compilation instead of
    // re-running Typst per frame (see benches/equation_frame.rs).
    let (stmts, errors) = animatix_syntax::parser::parse_source(
        r#"
config { colorscheme: "editorial-dark" }

#0s
eq: Equation, font_size: 48, color: text.primary, at: (960, 540) {
  lhs: Fragment, content: "E"
  eq_sign: Fragment, content: " = "
  mass: Fragment, content: "m"
  c2: Fragment, content: "c^2"
}

// Without this the subtree is static and the static-subtree cache serves
// every frame, so the assertion below would pass without memoizing anything.
always {
  eq.at = (960 + 40 * sin(t), 540)
}
"#,
    );
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let timeline = Timeline::build(&stmts.expect("parsed AST"));

    crate::renderer::text::clear_text_compile_cache();

    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    for i in 0..8 {
        let _ = timeline.evaluate(i as f64 * 0.05, dims);
    }

    assert_eq!(
        crate::renderer::text::grouped_text_compile_cache_len(),
        1,
        "eight frames with identical fragment content must memoize to one compilation"
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
        timeline.precise_bounds_for("counter").is_none(),
        "empty runtime text override should clear visible content"
    );
}

#[test]
fn text_content_assignment_crossfades_paths_at_midpoint() {
    let source = r#"
        config { colorscheme: "editorial-dark" }

        label: Text, text: "Hello", font_size: 48

        #1s
        label.text = "World" [1s]
    "#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(
        without_content_lints(&report.diagnostics).next().is_none(),
        "Unexpected diagnostics: {:?}",
        report.diagnostics
    );

    let track = report.output.get_track("label").expect("label track should exist");
    let paths_mid = track.evaluate_text_paths(1500);

    // The default path-track morph would return one set of glyphs at the
    // midpoint. The content-assignment path stores a Fade morph so both
    // endpoint glyph sets are returned at partial opacity.
    assert!(
        paths_mid.len() > 5,
        "Expected source+target glyph sets at midpoint, got {} paths",
        paths_mid.len()
    );
    assert!(
        paths_mid.iter().all(|p| p.opacity > 0.0 && p.opacity < 1.0),
        "Expected cross-fade opacities strictly inside (0, 1)"
    );
}
