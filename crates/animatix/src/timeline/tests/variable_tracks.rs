use super::*;
use crate::ast::BinaryOp;

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
                    target: vec![crate::ast::TargetSegment::Static("tracker".to_string())],
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
    for program in &timeline.modifier_programs {
        timeline
            .apply_modifier_program(
                program,
                0,
                SceneDimensions::default(),
                &mut env,
                &mut overrides,
            )
            .expect("modifier IR execution should succeed");
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
fn always_object_field_writes_update_frame_environment() {
    let ast = vec![Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body: vec![
            Stmt::LetDecl {
                is_pub: false,
                name: "p".to_string(),
                value: Expr::Construct(
                    "Point".to_string(),
                    vec![
                        Property {
                            name: "x".to_string(),
                            value: Expr::Num(10.0),
                            value_span: None,
                            trailing_comment: None,
                        },
                        Property {
                            name: "y".to_string(),
                            value: Expr::Num(20.0),
                            value_span: None,
                            trailing_comment: None,
                        },
                    ],
                ),
                span: None,
            },
            Stmt::Always {
                body: vec![
                    Stmt::Assignment {
                        target: vec![crate::ast::TargetSegment::Static("p".to_string())],
                        property: "x".to_string(),
                        value: Expr::Num(30.0),
                        modifiers: vec![],
                        easing: None,
                        value_span: None,
                        span: None,
                    },
                    Stmt::LetDecl {
                        is_pub: false,
                        name: "q".to_string(),
                        value: Expr::Binary(
                            Box::new(Expr::Path(vec!["p".to_string(), "x".to_string()])),
                            BinaryOp::Add,
                            Box::new(Expr::Num(1.0)),
                        ),
                        span: None,
                    },
                ],
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

    let mut overrides = std::collections::HashMap::new();
    let mut env = timeline.build_frame_env(0, SceneDimensions::default(), &overrides);
    for program in &timeline.modifier_programs {
        timeline
            .apply_modifier_program(
                program,
                0,
                SceneDimensions::default(),
                &mut env,
                &mut overrides,
            )
            .expect("modifier IR execution should succeed");
    }

    match env.get("p") {
        Some(Value::Object(name, fields)) => {
            assert_eq!(name, "Point");
            assert_eq!(fields["x"], Value::Num(30.0));
            assert_eq!(fields["y"], Value::Num(20.0));
        },
        other => panic!("Expected Point object, got {:?}", other),
    }
    assert_eq!(env.get("q"), Some(Value::Num(31.0)));
}

#[test]
fn build_time_for_loop_let_shadowing_carries_algorithm_state() {
    let source = r#"
#0s
let arr = {2, 1}
for i in {0} {
  if arr[0] > arr[1] {
    let arr = list_swap(arr, 0, 1)
  }
  let done = arr[0] == 1
}
"#;
    let parsed = animatix_syntax::parser::parse_canonical(source);
    assert!(parsed.parse_errors.is_empty(), "parse errors: {:?}", parsed.parse_errors);
    let report = Timeline::build_with_diagnostics(
        parsed.statements.as_ref().expect("statements"),
        &std::collections::HashMap::new(),
    );
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let arr = timeline.variable_tracks.get("arr").expect("arr track");
    assert_eq!(
        arr.evaluate(0),
        Some(Value::List(vec![Value::Num(1.0), Value::Num(2.0)])),
        "the shadowing let should carry list_swap state into the same build pass"
    );
    let done = timeline.variable_tracks.get("done").expect("done track");
    assert_eq!(
        done.evaluate(0),
        Some(Value::Num(1.0)),
        "later statements in the same keyframe should read the shadowed value"
    );
}

#[test]
fn build_time_for_loop_step_sequences_swap_actions() {
    // A `[step: 300ms]` for loop advances the build-time clock per iteration,
    // so variable-index swap actions land on distinct keyframe times instead
    // of colliding at the loop start.
    let source = r#"
config { dynamic_layout: true }
row: Row, at: (640, 440), gap: 16 {
  for k in {0, 1, 2} {
    b[k]: Rect, size: (70, 80), color: blue
  }
}
#0s
let arr = {3, 2, 1}
for i in {0, 1} [step: 300ms] {
  if arr[i] > arr[i+1] {
    swap row.b[i], row.b[i+1] [300ms]
    let arr = list_swap(arr, i, i+1)
  }
}
"#;
    let parsed = animatix_syntax::parser::parse_canonical(source);
    assert!(parsed.parse_errors.is_empty(), "parse errors: {:?}", parsed.parse_errors);
    let report = Timeline::build_with_diagnostics(
        parsed.statements.as_ref().expect("statements"),
        &std::collections::HashMap::new(),
    );
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    // Two swaps: t=0..300 (b0<->b1) and t=300..600 (b1<->b2).
    let orders = timeline
        .child_orders
        .get("row")
        .expect("row child_orders track")
        .keyframes
        .keys()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(orders, vec![0, 300, 600], "swap keyframes must advance per step");

    // The algorithm state carried through the loop updates per iteration:
    // {3,2,1} -> swap(0,1) -> {2,3,1} at t=0, then swap(1,2) -> {2,1,3} at t=300.
    let arr = timeline.variable_tracks.get("arr").expect("arr track");
    assert_eq!(
        arr.evaluate(0),
        Some(Value::List(vec![Value::Num(2.0), Value::Num(3.0), Value::Num(1.0)])),
        "the first iteration should swap indices 0 and 1"
    );
    assert_eq!(
        arr.evaluate(600),
        Some(Value::List(vec![Value::Num(2.0), Value::Num(1.0), Value::Num(3.0)])),
        "the second iteration should swap indices 1 and 2"
    );
}

#[test]
fn build_time_indexed_assignment_resolves_against_loop_variables() {
    // `row.b[j].color = red` inside a for loop resolves `j` against the build
    // environment into the concrete `b__N` track.
    let source = r#"
config { dynamic_layout: true }
row: Row, at: (640, 440), gap: 16 {
  for k in {0, 1, 2} {
    b[k]: Rect, size: (70, 80), color: blue
  }
}
#0s
for j in {0, 2} {
  row.b[j].color = red [200ms]
}
"#;
    let parsed = animatix_syntax::parser::parse_canonical(source);
    assert!(parsed.parse_errors.is_empty(), "parse errors: {:?}", parsed.parse_errors);
    let report = Timeline::build_with_diagnostics(
        parsed.statements.as_ref().expect("statements"),
        &std::collections::HashMap::new(),
    );
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    for key in ["b__0", "b__2"] {
        let track = timeline.tracks.get(key).unwrap_or_else(|| panic!("{key} track"));
        assert_eq!(
            track.style.color.get(200, [1.0, 1.0, 1.0, 1.0]),
            [1.0, 0.0, 0.0, 1.0],
            "indexed assignment should write the resolved actor by the end of its duration"
        );
    }
}
#[test]
fn always_nested_object_field_writes_update_frame_environment() {
    let ast = vec![Stmt::Keyframe {
        time: crate::ast::Time::Seconds(0.0),
        body: vec![
            Stmt::LetDecl {
                is_pub: false,
                name: "p".to_string(),
                value: Expr::Construct(
                    "Point".to_string(),
                    vec![Property {
                        name: "a".to_string(),
                        value: Expr::Construct(
                            "Inner".to_string(),
                            vec![
                                Property {
                                    name: "b".to_string(),
                                    value: Expr::Num(10.0),
                                    value_span: None,
                                    trailing_comment: None,
                                },
                                Property {
                                    name: "c".to_string(),
                                    value: Expr::Num(20.0),
                                    value_span: None,
                                    trailing_comment: None,
                                },
                            ],
                        ),
                        value_span: None,
                        trailing_comment: None,
                    }],
                ),
                span: None,
            },
            Stmt::Always {
                body: vec![
                    Stmt::Assignment {
                        target: vec![
                            crate::ast::TargetSegment::Static("p".to_string()),
                            crate::ast::TargetSegment::Static("a".to_string()),
                        ],
                        property: "b".to_string(),
                        value: Expr::Num(30.0),
                        modifiers: vec![],
                        easing: None,
                        value_span: None,
                        span: None,
                    },
                    Stmt::LetDecl {
                        is_pub: false,
                        name: "q".to_string(),
                        value: Expr::Binary(
                            Box::new(Expr::Path(vec![
                                "p".to_string(),
                                "a".to_string(),
                                "b".to_string(),
                            ])),
                            BinaryOp::Add,
                            Box::new(Expr::Num(1.0)),
                        ),
                        span: None,
                    },
                ],
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

    let mut overrides = std::collections::HashMap::new();
    let mut env = timeline.build_frame_env(0, SceneDimensions::default(), &overrides);
    for program in &timeline.modifier_programs {
        timeline
            .apply_modifier_program(
                program,
                0,
                SceneDimensions::default(),
                &mut env,
                &mut overrides,
            )
            .expect("modifier IR execution should succeed");
    }

    assert_eq!(env.get("q"), Some(Value::Num(31.0)));
    match env.get("p") {
        Some(Value::Object(_, fields)) => match &fields["a"] {
            Value::Object(name, inner) => {
                assert_eq!(name, "Inner");
                assert_eq!(inner["b"], Value::Num(30.0));
                assert_eq!(inner["c"], Value::Num(20.0));
            },
            other => panic!("Expected Inner object, got {:?}", other),
        },
        other => panic!("Expected Point object, got {:?}", other),
    }
}

#[test]
fn pure_fn_calls_evaluate_in_expressions() {
    // `fn` declarations with `-> Type` are evaluated at build time when their
    // calls appear in `let` expressions; `return` unwinds the body.
    let source = r#"
fn dnf(arr: List<Num>) -> List<Num> {
  let arr = list_swap(arr, 0, 2)
  return arr
}
fn total(xs: List<Num>) -> Num {
  let a = xs[0]
  let b = xs[1]
  if a > b {
    return a + b
  } else {
    return a * b
  }
}
#0s
let sorted = dnf({3, 2, 1})
let summed = total(sorted)
"#;
    let parsed = animatix_syntax::parser::parse_canonical(source);
    assert!(parsed.parse_errors.is_empty(), "parse errors: {:?}", parsed.parse_errors);
    let report = Timeline::build_with_diagnostics(
        parsed.statements.as_ref().expect("statements"),
        &std::collections::HashMap::new(),
    );
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let sorted = timeline.variable_tracks.get("sorted").expect("sorted track");
    assert_eq!(
        sorted.evaluate(0),
        Some(Value::List(vec![Value::Num(1.0), Value::Num(2.0), Value::Num(3.0)])),
        "dnf should swap indices 0 and 2"
    );
    let summed = timeline.variable_tracks.get("summed").expect("summed track");
    assert_eq!(
        summed.evaluate(0),
        Some(Value::Num(2.0)),
        "total(1,2,3) should return 1*2 via the else branch"
    );
}

#[test]
fn timeline_fn_expands_with_block_scope_without_leaking_locals() {
    // A timeline function body's local `let` must not leak into the scene
    // after the call, and must not write scene variable tracks.
    let source = r#"
config { colorscheme: "editorial-dark" }
fn pulse(strength: Num = 1.0) {
  let local = strength * 2
  self.scale = local [100ms]
}
btn: Rect, size: (100, 50), color: blue
#0s
fade-in btn [300ms]
#1s
pulse btn [strength: 1.5]
"#;
    // Timeline-function calls are expanded by the module system, so load
    // through the module graph like the CLI does.
    let mut graph = animatix_syntax::module::ModuleGraph::new();
    let program = graph
        .load_program_with_source(std::path::Path::new("fn_scope.amx"), Some(source))
        .expect("program loads");
    let expanded = program.expand_components();
    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.is_empty(),
        "expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    assert!(
        !timeline.variable_tracks.contains_key("local"),
        "function-local let must not create a scene variable track"
    );
    // The scale keyframe written through the fn body targets the actor;
    // scale is stored in the transform track as [s,0,0,s,0,0].
    let track = timeline.tracks.get("btn").expect("btn track");
    assert_eq!(
        track.geometry.scale.get(1100, 1.0),
        3.0,
        "pulse body should write scale = strength * 2 = 3.0 at t=1.1s"
    );
}
