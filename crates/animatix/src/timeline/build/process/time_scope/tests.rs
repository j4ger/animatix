use super::*;

fn assignment(duration: &str) -> Stmt {
    Stmt::Assignment {
        target: vec![crate::ast::TargetSegment::Static("dot".into())],
        property: "at".into(),
        value: Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0)]),
        modifiers: vec![Modifier {
            name: None,
            value: Expr::Ident(duration.into()),
        }],
        easing: None,
        value_span: None,
        span: None,
    }
}

fn build_source(source: &str) -> Timeline {
    let mut graph = animatix_syntax::module::ModuleGraph::new()
        .with_source_access(animatix_syntax::module::SourceAccess::SourcesOnly);
    let entry = std::path::Path::new("virtual:///time_scope_tests.amx");
    graph.upsert_source(entry.to_path_buf(), source.to_string());
    let mut program = graph.load_program(entry).expect("fixture should parse and load");
    let type_diagnostics = program.typecheck();
    assert!(type_diagnostics.is_empty(), "type diagnostics: {type_diagnostics:?}");
    let ast = program.expand_components(&mut Vec::new());
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "build diagnostics: {:?}", report.diagnostics);
    report.output
}

#[test]
fn local_time_rewrite_respects_let_initializer_scope_and_closure_shadowing() {
    let scope = TimeScope {
        origin_ms: 4000.0,
        rate: 2.0,
        end_ms: Some(6000.0),
    };
    let (let_u, shadowed) = retime_statement(
        &Stmt::LetDecl {
            is_pub: false,
            name: "u".into(),
            value: Expr::Ident("t".into()),
            span: None,
        },
        scope,
        250.0,
        false,
        &mut Vec::new(),
    );
    let Stmt::LetDecl { value, .. } = let_u else {
        panic!("expected let declaration")
    };
    assert_eq!(value, rewrite::local_time_expr(scope));
    assert!(!shadowed);

    let (let_t, shadowed) = retime_statement(
        &Stmt::LetDecl {
            is_pub: false,
            name: "t".into(),
            value: Expr::Ident("t".into()),
            span: None,
        },
        scope,
        0.0,
        false,
        &mut Vec::new(),
    );
    let Stmt::LetDecl { value, .. } = let_t else {
        panic!("expected let declaration")
    };
    assert_eq!(
        value,
        rewrite::local_time_expr(scope),
        "initializer reads the outer frame-time t"
    );
    assert!(shadowed);

    let shadowed_closure = Expr::Closure(vec!["t".into()], Box::new(Expr::Ident("t".into())));
    assert_eq!(
        rewrite::rewrite_expr_t(shadowed_closure.clone(), &rewrite::local_time_expr(scope), false),
        shadowed_closure
    );
    let free_closure = Expr::Closure(vec!["x".into()], Box::new(Expr::Ident("t".into())));
    assert!(rewrite::expr_has_free_t(&free_closure));
    assert_eq!(
        rewrite::rewrite_expr_t(free_closure, &rewrite::local_time_expr(scope), false),
        Expr::Closure(vec!["x".into()], Box::new(rewrite::local_time_expr(scope))),
    );
    assert!(!rewrite::expr_has_free_t(&shadowed_closure));
}

#[test]
fn local_plot_clock_holds_at_its_finite_end() {
    let scope = TimeScope {
        origin_ms: 1000.0,
        rate: 2.0,
        end_ms: Some(2000.0),
    };
    assert_eq!(
        rewrite::local_plot_time_expr(scope),
        Expr::Call(
            "clamp".into(),
            vec![
                rewrite::local_time_expr(scope),
                Expr::Num(0.0),
                Expr::Num(0.5)
            ],
        ),
    );
}

#[test]
fn local_always_rejects_function_t_shadowing_its_scene_clock_guard() {
    let timeline = Timeline::new();
    let body = vec![
        Stmt::LetDecl {
            is_pub: false,
            name: "t".into(),
            value: Expr::Num(0.25),
            span: None,
        },
        Stmt::Always {
            body: vec![assignment("0ms")],
            span: None,
        },
    ];
    let spec = TimeScopeSpec {
        time_scale: None,
        clip_duration: Some(Expr::Ident("500ms".into())),
        end_limit_duration_ms: None,
    };
    let mut diagnostics = Vec::new();
    assert!(resolve_scope(&timeline, Some(&spec), &body, 0.0, false, &mut diagnostics).is_none());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.is_error()));

    let loop_body = vec![Stmt::ForLoop {
        var: crate::ast::LoopPattern::Single("t".into()),
        index_var: None,
        iterable: Expr::List(vec![Expr::Num(0.0)]),
        body: vec![Stmt::Always {
            body: vec![assignment("0ms")],
            span: None,
        }],
        modifiers: Vec::new(),
        span: None,
    }];
    let mut diagnostics = Vec::new();
    assert!(
        resolve_scope(&timeline, Some(&spec), &loop_body, 0.0, false, &mut diagnostics).is_none()
    );
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.is_error()));
}

#[test]
fn function_local_loop_arguments_are_not_mistaken_for_globals_in_extent_inference() {
    let timeline = Timeline::new();
    let body = vec![
        Stmt::LetDecl {
            is_pub: false,
            name: "items".into(),
            value: Expr::List(vec![Expr::Num(1.0), Expr::Num(2.0)]),
            span: None,
        },
        Stmt::ForLoop {
            var: crate::ast::LoopPattern::Single("item".into()),
            index_var: None,
            iterable: Expr::Ident("items".into()),
            body: vec![assignment("100ms")],
            modifiers: Vec::new(),
            span: None,
        },
    ];
    let spec = TimeScopeSpec {
        time_scale: Some(Expr::Num(2.0)),
        clip_duration: None,
        end_limit_duration_ms: None,
    };
    let mut diagnostics = Vec::new();
    assert!(resolve_scope(&timeline, Some(&spec), &body, 0.0, false, &mut diagnostics).is_none());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.is_error()));
}

#[test]
fn empty_loop_and_overflowed_block_spans_are_handled_safely() {
    let timeline = Timeline::new();
    let empty_loop = Stmt::ForLoop {
        var: crate::ast::LoopPattern::Single("item".into()),
        index_var: None,
        iterable: Expr::List(Vec::new()),
        body: vec![assignment("100ms")],
        modifiers: vec![Modifier {
            name: Some("step".into()),
            value: Expr::Ident("250ms".into()),
        }],
        span: None,
    };
    assert_eq!(statement_extent_ms(&timeline, &empty_loop, true), Some(0.0));

    let body = vec![assignment("2ms")];
    let scale = TimeScopeSpec {
        time_scale: Some(Expr::Num(1.0e308)),
        clip_duration: None,
        end_limit_duration_ms: None,
    };
    assert_eq!(scoped_block_span_ms(&timeline, Some(&scale), &body, true), None);
    assert_eq!(
        sequence_block_span_ms(&timeline, None, &[assignment("1e308ms"), assignment("1e308ms")]),
        None,
    );
}

#[test]
fn parsed_local_sequence_scales_assignment_delay_and_cursor() {
    let timeline = build_source(
        r#"
config { colorscheme: "editorial-dark", resolution: (800, 450), duration: 3 }
first: Ellipse, at: (200, 200), size: (24, 24), opacity: 1
second: Ellipse, at: (400, 200), size: (24, 24), opacity: 1

fn move_pair(first, second) {
  #0ms
  sequence {
    first.opacity = 0.5 [100ms, delay: 50ms]
    second.opacity = 0.25 [200ms]
  }
}

#1s
move_pair(first, second) [time_scale: 2.0]
"#,
    );

    assert!(timeline.has_keyframe_at("first", "opacity", 1100));
    assert!(timeline.has_keyframe_at("first", "opacity", 1300));
    assert!(timeline.has_keyframe_at("second", "opacity", 1300));
    assert!(timeline.has_keyframe_at("second", "opacity", 1700));
}

#[test]
fn parsed_local_for_step_and_stagger_scale_to_scene_keyframe_times() {
    let timeline = build_source(
        r#"
config { colorscheme: "editorial-dark", resolution: (800, 450), duration: 3 }
dot: Ellipse, at: (200, 200), size: (24, 24), opacity: 1
first: Ellipse, at: (400, 200), size: (24, 24), opacity: 1
second: Ellipse, at: (600, 200), size: (24, 24), opacity: 1

fn animate_batch(dot, first, second) {
  #0ms
  for i in {0, 1, 2} [step: 100ms] {
    dot.opacity = i * 0.2 [20ms]
  }
  stagger [each: 100ms] {
    first.opacity = 0.5 [20ms]
    second.opacity = 0.25 [20ms]
  }
}

#1s
animate_batch(dot, first, second) [time_scale: 2.0]
"#,
    );

    for time_ms in [1000, 1040, 1200, 1240, 1400, 1440] {
        assert!(
            timeline.has_keyframe_at("dot", "opacity", time_ms),
            "dot keyframe at {time_ms}ms"
        );
    }
    for time_ms in [1000, 1040] {
        assert!(
            timeline.has_keyframe_at("first", "opacity", time_ms),
            "first keyframe at {time_ms}ms"
        );
    }
    for time_ms in [1200, 1240] {
        assert!(
            timeline.has_keyframe_at("second", "opacity", time_ms),
            "second keyframe at {time_ms}ms"
        );
    }
}

#[test]
fn always_only_clip_end_extends_inferred_timeline_duration() {
    let timeline = build_source(
        r#"
config { colorscheme: "editorial-dark", resolution: (800, 450) }
dot: Ellipse, at: (200, 200), size: (24, 24), opacity: 0

fn pulse(dot) {
  always { dot.opacity = clamp(t, 0, 1) }
}

#1s
pulse(dot) [clip_duration: 900ms]
"#,
    );

    assert_eq!(timeline.declared_duration_seconds(), None);
    assert!((timeline.duration_seconds() - 1.9).abs() < 1e-6);
    assert!((timeline.playback_duration_seconds() - 1.9).abs() < 1e-6);
}

#[test]
fn scoped_block_extent_uses_parallel_or_sequence_cursor_as_authored() {
    let timeline = Timeline::new();
    let body = vec![assignment("500ms"), assignment("500ms")];
    let scaled = TimeScopeSpec {
        time_scale: Some(Expr::Num(2.0)),
        clip_duration: None,
        end_limit_duration_ms: None,
    };
    assert_eq!(scoped_block_span_ms(&timeline, Some(&scaled), &body, false), Some(1000.0));
    assert_eq!(scoped_block_span_ms(&timeline, Some(&scaled), &body, true), Some(2000.0));
    let block = Stmt::Block {
        body: body.clone(),
        instance_scope: None,
        time_scope: Some(scaled),
        span: None,
    };
    assert_eq!(statement_extent_ms(&timeline, &block, false), Some(1000.0));
    assert_eq!(statement_extent_ms(&timeline, &block, true), Some(2000.0));

    let clipped = TimeScopeSpec {
        time_scale: None,
        clip_duration: Some(Expr::Ident("500ms".into())),
        end_limit_duration_ms: None,
    };
    let mut diagnostics = Vec::new();
    let parallel =
        resolve_scope(&timeline, Some(&clipped), &body, 1000.0, false, &mut diagnostics).unwrap();
    let sequential =
        resolve_scope(&timeline, Some(&clipped), &body, 1000.0, true, &mut diagnostics).unwrap();
    assert_eq!(parallel.rate, 1.0);
    assert_eq!(sequential.rate, 0.5);
    assert!(diagnostics.is_empty());
}

#[test]
fn nested_always_inherits_outer_finite_end_even_at_unit_rate() {
    let timeline = Timeline::new();
    let parent = TimeScope {
        origin_ms: 1000.0,
        rate: 1.0,
        end_ms: Some(2000.0),
    };
    let inner_body = vec![Stmt::Always {
        body: vec![assignment("0ms")],
        span: None,
    }];
    let block = Stmt::Block {
        body: inner_body,
        instance_scope: None,
        time_scope: None,
        span: None,
    };
    let (rewritten, _) = retime_statement(&block, parent, 500.0, false, &mut Vec::new());
    let Stmt::Block {
        body,
        time_scope: Some(spec),
        ..
    } = rewritten
    else {
        panic!("nested block should inherit a local time scope")
    };
    assert_eq!(spec.end_limit_duration_ms, Some(500.0));
    let inner =
        resolve_scope(&timeline, Some(&spec), &body, 1500.0, false, &mut Vec::new()).unwrap();
    assert_eq!(inner.end_ms, Some(2000.0));
    let (always, _) = retime_statement(&body[0], inner, 0.0, false, &mut Vec::new());
    assert!(
        matches!(always, Stmt::Always { body, .. } if matches!(body.as_slice(), [Stmt::Conditional { .. }]))
    );
}

#[test]
fn overflowing_scope_extent_is_rejected() {
    let timeline = Timeline::new();
    let body = vec![Stmt::Keyframe {
        time: Time::Seconds(2.0),
        body: vec![assignment("0ms")],
        span: None,
    }];
    let spec = TimeScopeSpec {
        time_scale: Some(Expr::Num(1.0e308)),
        clip_duration: None,
        end_limit_duration_ms: None,
    };
    let mut diagnostics = Vec::new();
    assert!(resolve_scope(&timeline, Some(&spec), &body, 0.0, false, &mut diagnostics).is_none());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.is_error()));
}
