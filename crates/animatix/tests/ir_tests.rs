use std::collections::HashMap;
use std::fs;

use animatix::ir::{
    CompiledExpr, ModifierIrProgram, ModifierIrStmt, ModifierOverrides, compile_expr,
    execute_modifier_ir, lower_modifier_ir,
};
use animatix::timeline::{
    Environment, SceneDimensions, Timeline, Value, evaluate_expr, load_standard_library,
};
use animatix_syntax::ast::{BinaryOp, Expr, LoopPattern, Stmt, Time};
use animatix_syntax::module::ModuleGraph;

fn evaluate_modifier_via_ir(value: CompiledExpr, env: &mut Environment) -> Value {
    let program = ModifierIrProgram {
        statements: vec![ModifierIrStmt::Let {
            name: "__ir_test_result".to_string(),
            value,
        }],
    };
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, env, &mut overrides).expect("IR execution should succeed");
    env.get("__ir_test_result").expect("IR result should be stored")
}

const REACTIVE_FIXTURE: &str = r#"// Reactive: always, time-driven behavior, if/else.

config { colorscheme: "editorial-dark" }

#0s
center: Ellipse, size: (16, 16), color: text.primary, at: (640, 390)
orbiter: Ellipse, size: (64, 64), color: accent.primary, at: (820, 390)
pulse: Rect, size: (120, 120), color: (0.88, 0.42, 0.84, 1.0), at: (280, 390)
echo: Ellipse, size: (40, 40), color: accent.warning, at: pulse.at

always {
  orbiter.at = (640 + 180 * cos(t), 390 + 120 * sin(t * 2))
  pulse.size = if (t % 1.0) < 0.5 { (120, 120) } else { (180, 180) }
  echo.size = (pulse.size.x / 3, pulse.size.x / 3)
  echo.at = orbiter.at
}

// Explicit repeated declarations (for loops are top-level only)
#0s
dots: Row, anchor: scene.bottom, offset: (0, -120), gap: 30, align: "center" {
  d0: Ellipse, size: (24, 24), color: accent.primary
  d1: Ellipse, size: (32, 32), color: accent.success
  d2: Ellipse, size: (40, 40), color: accent.warning
  d3: Ellipse, size: (48, 48), color: accent.danger
  d4: Ellipse, size: (56, 56), color: accent.primary
}
"#;

#[test]
fn ir_lowering_lowers_always_assignment_subset() {
    let program = vec![Stmt::Always {
        body: vec![Stmt::Assignment {
            target: vec![animatix_syntax::ast::TargetSegment::Static(
                "pulse".to_string(),
            )],
            property: "opacity".to_string(),
            value: Expr::Num(1.0),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }],
        span: None,
    }];

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    let rendered = format!("{ir}");

    assert_eq!(ir.statements.len(), 1);
    assert!(rendered.contains("assign pulse.opacity = const(Num(1))"));
}

#[test]
fn ir_lowering_bare_always_assignment_writes_frame_variable() {
    let program = vec![Stmt::Always {
        body: vec![Stmt::Assignment {
            target: vec![],
            property: "freq".to_string(),
            value: Expr::Binary(
                Box::new(Expr::Ident("t".to_string())),
                BinaryOp::Mul,
                Box::new(Expr::Num(2.0)),
            ),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }],
        span: None,
    }];

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    assert_eq!(
        ir.statements,
        vec![ModifierIrStmt::Let {
            name: "freq".to_string(),
            value: compile_expr(&Expr::Binary(
                Box::new(Expr::Ident("t".to_string())),
                BinaryOp::Mul,
                Box::new(Expr::Num(2.0)),
            ))
            .expect("expression should compile"),
        }]
    );

    let mut env = Environment::new();
    env.set("t", Value::Num(0.5));
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&ir, &mut env, &mut overrides).expect("IR execution should succeed");
    assert_eq!(env.get("freq"), Some(Value::Num(1.0)));
    assert!(overrides.is_empty(), "bare variable writes are not actor overrides");
}

#[test]
fn ir_lowering_supports_conditionals_and_lets() {
    let program = vec![Stmt::Always {
        body: vec![
            Stmt::LetDecl {
                is_pub: false,
                name: "x".to_string(),
                value: Expr::Binary(
                    Box::new(Expr::Ident("t".to_string())),
                    BinaryOp::Mul,
                    Box::new(Expr::Num(2.0)),
                ),
                span: None,
            },
            Stmt::Conditional {
                condition: Expr::Binary(
                    Box::new(Expr::Ident("x".to_string())),
                    BinaryOp::Lt,
                    Box::new(Expr::Num(1.0)),
                ),
                then_branch: vec![Stmt::Assignment {
                    target: vec![animatix_syntax::ast::TargetSegment::Static(
                        "pulse".to_string(),
                    )],
                    property: "opacity".to_string(),
                    value: Expr::Num(1.0),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                }],
                else_branch: Some(vec![Stmt::Assignment {
                    target: vec![animatix_syntax::ast::TargetSegment::Static(
                        "pulse".to_string(),
                    )],
                    property: "opacity".to_string(),
                    value: Expr::Num(0.0),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                }]),
                span: None,
            },
        ],
        span: None,
    }];

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    let rendered = format!("{ir}");

    assert!(rendered.contains("let x = (load(t) Mul const(Num(2)))"));
    assert!(rendered.contains("if (load(x) Lt const(Num(1)))"));
    assert!(rendered.contains("assign pulse.opacity = const(Num(1))"));
    assert!(rendered.contains("assign pulse.opacity = const(Num(0))"));
}

#[test]
fn ir_lowering_ignores_comments_in_always_blocks() {
    let program = vec![Stmt::Always {
        body: vec![Stmt::Comment("test".to_string(), None)],
        span: None,
    }];

    let result = lower_modifier_ir(&program).expect("lowering should succeed");
    assert!(
        result.statements.is_empty(),
        "comments should be filtered out, leaving empty program"
    );
}

#[test]
fn ir_lowering_supports_construct_expression_forms() {
    let expr = Expr::Construct("Point".to_string(), vec![]);

    let compiled = compile_expr(&expr).expect("Construct should lower successfully");
    assert!(matches!(compiled, CompiledExpr::Construct(..)));
}

#[test]
fn ir_matches_evaluate_expr_for_supported_subset() {
    let expr = Expr::Conditional(
        Box::new(Expr::Binary(
            Box::new(Expr::Call("sin".to_string(), vec![Expr::Ident("t".to_string())])),
            BinaryOp::Gt,
            Box::new(Expr::Num(0.0)),
        )),
        Box::new(Expr::Call(
            "format".to_string(),
            vec![
                Expr::Str("pulse={}".to_string()),
                Expr::Ident("pulse".to_string()),
            ],
        )),
        Box::new(Expr::Str("pulse=off".to_string())),
    );

    let compiled = compile_expr(&expr).expect("expression should compile");
    let mut env = Environment::new();
    load_standard_library(&mut env);
    env.set("t", Value::Num(std::f64::consts::FRAC_PI_2));
    env.set("pulse", Value::Num(3.0));

    let compiled_value = evaluate_modifier_via_ir(compiled, &mut env);
    let ast_value = evaluate_expr(&expr, &env).expect("ast eval should work");

    assert_eq!(compiled_value, ast_value);
}

#[test]
fn ir_matches_ast_for_extended_builtins() {
    let cases: Vec<Expr> = vec![
        Expr::Call("signum".to_string(), vec![Expr::Num(-2.5)]),
        Expr::Call("fract".to_string(), vec![Expr::Num(3.75)]),
        Expr::Call("hypot".to_string(), vec![Expr::Num(3.0), Expr::Num(4.0)]),
        Expr::Call("pow".to_string(), vec![Expr::Num(2.0), Expr::Num(10.0)]),
        Expr::Call("rem".to_string(), vec![Expr::Num(7.0), Expr::Num(4.0)]),
        Expr::Call("step".to_string(), vec![Expr::Num(1.0), Expr::Num(0.5)]),
        Expr::Call("round".to_string(), vec![Expr::Num(2.6)]),
    ];

    for expr in cases {
        let compiled = compile_expr(&expr).expect("builtin should compile");
        let mut env = Environment::new();
        load_standard_library(&mut env);
        let ir_value = evaluate_modifier_via_ir(compiled, &mut env);
        let ast_value = evaluate_expr(&expr, &env).expect("ast eval should work");
        assert_eq!(ir_value, ast_value, "IR and AST disagree for {expr:?}");
    }
}

#[test]
fn ir_matches_ast_for_environment_functions() {
    let cases: Vec<Expr> = vec![
        Expr::Call("rgb".to_string(), vec![Expr::Num(255.0), Expr::Num(0.0), Expr::Num(128.0)]),
        Expr::Call(
            "rgba".to_string(),
            vec![
                Expr::Num(1.0),
                Expr::Num(0.5),
                Expr::Num(0.0),
                Expr::Num(0.25),
            ],
        ),
        Expr::Call("vec2".to_string(), vec![Expr::Num(3.0), Expr::Num(4.0)]),
        Expr::Call("hsv".to_string(), vec![Expr::Num(120.0), Expr::Num(1.0), Expr::Num(1.0)]),
        Expr::Call("seeded_rand".to_string(), vec![Expr::Num(42.0)]),
    ];

    for expr in cases {
        let compiled = compile_expr(&expr).expect("environment call should compile");
        let mut env = Environment::new();
        load_standard_library(&mut env);
        let ir_value = evaluate_modifier_via_ir(compiled, &mut env);
        let ast_value = evaluate_expr(&expr, &env).expect("ast eval should work");
        assert_eq!(ir_value, ast_value, "IR and AST disagree for {expr:?}");
    }
}

#[test]
fn modifier_ir_can_lower_post_expansion_program() {
    let program = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::Always {
                body: vec![Stmt::Assignment {
                    target: vec![animatix_syntax::ast::TargetSegment::Static(
                        "pulse".to_string(),
                    )],
                    property: "opacity".to_string(),
                    value: Expr::Binary(
                        Box::new(Expr::Ident("t".to_string())),
                        BinaryOp::Mul,
                        Box::new(Expr::Num(0.5)),
                    ),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                }],
                span: None,
            }],
            span: None,
        },
        Stmt::Comment("ignored".to_string(), None),
    ];

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    assert_eq!(ir.statements.len(), 1);
    assert!(format!("{ir}").contains("assign pulse.opacity"));
}

#[test]
fn ir_indexed_override_executes() {
    let program = vec![Stmt::Always {
        body: vec![Stmt::Assignment {
            target: vec![animatix_syntax::ast::TargetSegment::Indexed {
                base: "bars".to_string(),
                index: Box::new(Expr::Ident("i".to_string())),
            }],
            property: "scale".to_string(),
            value: Expr::Num(1.5),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }],
        span: None,
    }];

    let ir = lower_modifier_ir(&program).expect("IR lowering should succeed");
    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    env.set("i", Value::Num(2.0));
    execute_modifier_ir(&ir, &mut env, &mut overrides).expect("IR execution should succeed");
    assert_eq!(overrides["bars__2"]["scale"], Value::Num(1.5));
}

#[test]
fn modifier_ir_executes_let_and_if() {
    let program = vec![Stmt::Always {
        body: vec![
            Stmt::LetDecl {
                is_pub: false,
                name: "x".to_string(),
                value: Expr::Binary(
                    Box::new(Expr::Ident("t".to_string())),
                    BinaryOp::Mul,
                    Box::new(Expr::Num(2.0)),
                ),
                span: None,
            },
            Stmt::Conditional {
                condition: Expr::Binary(
                    Box::new(Expr::Ident("x".to_string())),
                    BinaryOp::Lt,
                    Box::new(Expr::Num(1.0)),
                ),
                then_branch: vec![Stmt::Assignment {
                    target: vec![animatix_syntax::ast::TargetSegment::Static(
                        "pulse".to_string(),
                    )],
                    property: "opacity".to_string(),
                    value: Expr::Num(1.0),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                }],
                else_branch: Some(vec![Stmt::Assignment {
                    target: vec![animatix_syntax::ast::TargetSegment::Static(
                        "pulse".to_string(),
                    )],
                    property: "opacity".to_string(),
                    value: Expr::Num(0.0),
                    modifiers: vec![],
                    easing: None,
                    value_span: None,
                    span: None,
                }]),
                span: None,
            },
        ],
        span: None,
    }];
    let ir = lower_modifier_ir(&program).expect("lowering should succeed");

    let mut timeline = Timeline::new();
    load_standard_library(timeline.env_mut());
    let mut overrides = HashMap::new();
    let mut env = timeline.build_frame_env(500, SceneDimensions::default(), &overrides);
    timeline
        .apply_modifier_program(&ir, 500, SceneDimensions::default(), &mut env, &mut overrides)
        .expect("IR execution should succeed");

    assert_eq!(overrides["pulse"]["opacity"], Value::Num(0.0));
}

#[test]
fn ir_supports_runtime_object_field_writes() {
    let program = vec![Stmt::Always {
        body: vec![
            Stmt::Assignment {
                target: vec![animatix_syntax::ast::TargetSegment::Static("p".to_string())],
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
    }];

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");

    let make_env = || {
        let mut env = Environment::new();
        env.set(
            "p",
            Value::Object(
                "Point".to_string(),
                HashMap::from([
                    ("x".to_string(), Value::Num(10.0)),
                    ("y".to_string(), Value::Num(20.0)),
                ]),
            ),
        );
        env
    };

    let mut env = make_env();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&ir, &mut env, &mut overrides).expect("IR execution should succeed");
    assert_eq!(env.get("q"), Some(Value::Num(31.0)));
    assert!(overrides.is_empty(), "object writes should not create actor overrides");
}

#[test]
fn ir_supports_nested_object_field_writes() {
    let program = vec![Stmt::Always {
        body: vec![
            Stmt::Assignment {
                target: vec![
                    animatix_syntax::ast::TargetSegment::Static("p".to_string()),
                    animatix_syntax::ast::TargetSegment::Static("a".to_string()),
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
                    Box::new(Expr::Path(vec!["p".to_string(), "a".to_string(), "b".to_string()])),
                    BinaryOp::Add,
                    Box::new(Expr::Num(1.0)),
                ),
                span: None,
            },
        ],
        span: None,
    }];

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");

    let make_env = || {
        let mut env = Environment::new();
        env.set(
            "p",
            Value::Object(
                "Point".to_string(),
                HashMap::from([(
                    "a".to_string(),
                    Value::Object(
                        "Inner".to_string(),
                        HashMap::from([
                            ("b".to_string(), Value::Num(10.0)),
                            ("c".to_string(), Value::Num(20.0)),
                        ]),
                    ),
                )]),
            ),
        );
        env
    };

    let mut env = make_env();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&ir, &mut env, &mut overrides).expect("IR execution should succeed");
    assert_eq!(env.get("q"), Some(Value::Num(31.0)));
    assert!(overrides.is_empty());
}

#[test]
fn modifier_ir_executes_construct_expr() {
    let program = vec![Stmt::Always {
        body: vec![Stmt::Assignment {
            target: vec![animatix_syntax::ast::TargetSegment::Static(
                "pulse".to_string(),
            )],
            property: "opacity".to_string(),
            value: Expr::Construct("Point".to_string(), vec![]),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }],
        span: None,
    }];

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&ir, &mut env, &mut overrides).expect("IR execution should succeed");
    match overrides["pulse"].get("opacity") {
        Some(Value::Object(name, _)) => assert_eq!(name, "Point"),
        other => panic!("expected Point object override, got {:?}", other),
    }
}

#[test]
fn ir_reactive_runtime_executes() {
    let expanded = load_fixture_program(REACTIVE_FIXTURE);
    let timeline = Timeline::build(&expanded);
    let ir = lower_modifier_ir(&expanded).expect("IR lowering should succeed");

    for time_ms in [500_u64, 1500_u64] {
        let mut overrides = HashMap::new();
        let mut env = timeline.build_frame_env(time_ms, SceneDimensions::default(), &overrides);
        timeline
            .apply_modifier_program(
                &ir,
                time_ms,
                SceneDimensions::default(),
                &mut env,
                &mut overrides,
            )
            .expect("IR execution should succeed");

        assert!(
            !overrides.is_empty(),
            "reactive fixture should produce overrides at time {time_ms}"
        );
    }
}

#[test]
fn ir_nested_modifier_targets_execute() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "panel.badge".to_string(),
                array_index: None,
                ty: "Ellipse".to_string(),
                props: vec![
                    animatix_syntax::ast::Property {
                        name: "radius".to_string(),
                        value: Expr::Num(12.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                    animatix_syntax::ast::Property {
                        name: "color".to_string(),
                        value: Expr::Ident("RED".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                    animatix_syntax::ast::Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(400.0), Expr::Num(300.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "echo".to_string(),
                array_index: None,
                ty: "Ellipse".to_string(),
                props: vec![
                    animatix_syntax::ast::Property {
                        name: "radius".to_string(),
                        value: Expr::Num(10.0),
                        value_span: None,
                        trailing_comment: None,
                    },
                    animatix_syntax::ast::Property {
                        name: "color".to_string(),
                        value: Expr::Ident("BLUE".to_string()),
                        value_span: None,
                        trailing_comment: None,
                    },
                    animatix_syntax::ast::Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(300.0)]),
                        value_span: None,
                        trailing_comment: None,
                    },
                ],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
            Stmt::Always {
                body: vec![
                    Stmt::Assignment {
                        target: vec![
                            animatix_syntax::ast::TargetSegment::Static("panel".to_string()),
                            animatix_syntax::ast::TargetSegment::Static("badge".to_string()),
                        ],
                        property: "radius".to_string(),
                        value: Expr::Binary(
                            Box::new(Expr::Ident("t".to_string())),
                            BinaryOp::Add,
                            Box::new(Expr::Num(10.0)),
                        ),
                        modifiers: vec![],
                        easing: None,
                        value_span: None,
                        span: None,
                    },
                    Stmt::Assignment {
                        target: vec![animatix_syntax::ast::TargetSegment::Static(
                            "echo".to_string(),
                        )],
                        property: "radius".to_string(),
                        value: Expr::Path(vec![
                            "panel".to_string(),
                            "badge".to_string(),
                            "radius".to_string(),
                        ]),
                        modifiers: vec![],
                        easing: None,
                        value_span: None,
                        span: None,
                    },
                ],
                span: None,
            },
        ],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let ir = lower_modifier_ir(&ast).expect("IR lowering should succeed");

    let mut overrides = HashMap::new();
    let mut env = timeline.build_frame_env(1000, SceneDimensions::default(), &overrides);
    timeline
        .apply_modifier_program(&ir, 1000, SceneDimensions::default(), &mut env, &mut overrides)
        .expect("IR execution should succeed");

    assert!(
        overrides.contains_key("echo"),
        "nested modifier targets should produce an echo override"
    );
}

#[test]
fn ir_for_loop_binds_index_var() {
    // Test for-loops through the IR interpreter, including loop variable cleanup.
    let stmt = Stmt::ForLoop {
        var: LoopPattern::Single("v".to_string()),
        index_var: Some("i".to_string()),
        iterable: Expr::List(vec![Expr::Num(10.0), Expr::Num(20.0), Expr::Num(30.0)]),
        body: vec![Stmt::LetDecl {
            name: "z".to_string(),
            value: Expr::Binary(
                Box::new(Expr::Ident("v".to_string())),
                BinaryOp::Mul,
                Box::new(Expr::Binary(
                    Box::new(Expr::Ident("i".to_string())),
                    BinaryOp::Add,
                    Box::new(Expr::Num(1.0)),
                )),
            ),
            is_pub: false,
            span: None,
        }],
        span: None,
    };

    let program = vec![Stmt::Always {
        body: vec![stmt],
        span: None,
    }];

    let ir = lower_modifier_ir(&program).expect("IR lowering should succeed");

    // Execute via the IR interpreter
    let mut env = Environment::new();
    let mut overrides: ModifierOverrides = HashMap::new();
    execute_modifier_ir(&ir, &mut env, &mut overrides).expect("IR execution should succeed");

    // After the loop, all loop variables should be cleaned up
    assert!(
        env.get("v").is_none(),
        "Loop variable 'v' should be cleaned up after loop exit (IR path)"
    );
    assert!(
        env.get("i").is_none(),
        "Index variable 'i' should be cleaned up after loop exit (IR path)"
    );
    // The let-decl 'z' should survive
    assert!(env.get("z").is_some(), "Let-decl 'z' should survive after loop exit (IR path)");
    // Last iteration: v=30, i=2, z = 30 * (2+1) = 90
    if let Some(Value::Num(n)) = env.get("z") {
        assert!(
            (n - 90.0).abs() < 0.01,
            "Expected z=90 (last iter: v=30, i=2), got {} (IR path)",
            n
        );
    } else {
        panic!("Expected Num value for z (IR path)");
    }
}

#[test]
fn ir_truthiness_handles_bool_literals() {
    let assign = |property: &str, value: CompiledExpr| ModifierIrStmt::Assign {
        target: vec!["actor".to_string()],
        property: property.to_string(),
        value,
    };

    // `if true` must take the then-branch.
    let program = ModifierIrProgram {
        statements: vec![ModifierIrStmt::If {
            condition: CompiledExpr::Const(Value::Bool(true)),
            then_branch: vec![assign("a", CompiledExpr::Const(Value::Num(1.0)))],
            else_branch: vec![assign("a", CompiledExpr::Const(Value::Num(0.0)))],
        }],
    };
    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, &mut env, &mut overrides).expect("execute");
    assert_eq!(overrides["actor"]["a"], Value::Num(1.0));

    // Ternary `Select` with a Bool condition.
    let select = CompiledExpr::Select(
        Box::new(CompiledExpr::Const(Value::Bool(true))),
        Box::new(CompiledExpr::Const(Value::Num(7.0))),
        Box::new(CompiledExpr::Const(Value::Num(9.0))),
    );
    let program = ModifierIrProgram {
        statements: vec![ModifierIrStmt::Let {
            name: "r".to_string(),
            value: select,
        }],
    };
    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, &mut env, &mut overrides).expect("execute");
    assert_eq!(env.get("r"), Some(Value::Num(7.0)));

    // `!true`, `true && false`, and `true || false` with Bool operands.
    let not_true = CompiledExpr::Unary(
        animatix_syntax::ast::UnaryOp::Not,
        Box::new(CompiledExpr::Const(Value::Bool(true))),
    );
    let and_expr = CompiledExpr::Binary(
        Box::new(CompiledExpr::Const(Value::Bool(true))),
        BinaryOp::And,
        Box::new(CompiledExpr::Const(Value::Bool(false))),
    );
    let or_expr = CompiledExpr::Binary(
        Box::new(CompiledExpr::Const(Value::Bool(true))),
        BinaryOp::Or,
        Box::new(CompiledExpr::Const(Value::Bool(false))),
    );
    let program = ModifierIrProgram {
        statements: vec![
            ModifierIrStmt::Let {
                name: "n".to_string(),
                value: not_true,
            },
            ModifierIrStmt::Let {
                name: "and".to_string(),
                value: and_expr,
            },
            ModifierIrStmt::Let {
                name: "or".to_string(),
                value: or_expr,
            },
        ],
    };
    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, &mut env, &mut overrides).expect("execute");
    assert_eq!(env.get("n"), Some(Value::Num(0.0)));
    assert_eq!(env.get("and"), Some(Value::Num(0.0)));
    assert_eq!(env.get("or"), Some(Value::Num(1.0)));
}

#[test]
fn ir_for_loop_tuple_destructures_color() {
    let program = ModifierIrProgram {
        statements: vec![ModifierIrStmt::For {
            var: LoopPattern::Tuple(vec![
                "r".to_string(),
                "g".to_string(),
                "b".to_string(),
                "a".to_string(),
            ]),
            index_var: None,
            iterable: CompiledExpr::Const(Value::Color([0.1, 0.2, 0.3, 1.0])),
            body: vec![ModifierIrStmt::Let {
                name: "__captured_color".to_string(),
                value: CompiledExpr::LoadEnv("r".to_string()),
            }],
        }],
    };

    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, &mut env, &mut overrides).expect("IR execution should succeed");
    assert_eq!(env.get("__captured_color"), Some(Value::Num(0.1)));
    assert_eq!(env.get("r"), None, "loop variables should be cleaned up");
}

fn load_fixture_program(source: &str) -> Vec<Stmt> {
    let temp_path = std::env::temp_dir().join("animatix_test_reactive.amx");
    fs::write(&temp_path, source).expect("write temp fixture should succeed");
    ModuleGraph::new()
        .load_program(&temp_path)
        .expect("program should load")
        .expand_components()
}
