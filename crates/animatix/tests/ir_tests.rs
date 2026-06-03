use animatix_syntax::ast::{BinaryOp, Expr, Stmt, Time};
use animatix::ir::{
    ModifierExpr, compile_modifier_expr, evaluate_modifier_expr, lower_modifier_ir,
};
use animatix_syntax::module::ModuleGraph;
use animatix::timeline::{
    Environment, SceneDimensions, Timeline, Value, evaluate_expr, load_standard_library,
};
use animatix::vm::{VmCompileError, compile_modifier_bytecode};
use std::collections::HashMap;
use std::fs;


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
            target: vec!["pulse".to_string()],
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
fn ir_lowering_supports_conditionals_and_lets() {
    let program = vec![Stmt::Always {
        body: vec![
            Stmt::LetDecl { is_pub: false,
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
    assert!(result.statements.is_empty(), "comments should be filtered out, leaving empty program");
}

#[test]
fn ir_lowering_rejects_unsupported_expression_forms() {
    let expr = Expr::Construct(
        "Point".to_string(),
        vec![],
    );

    let compiled = compile_modifier_expr(&expr);
    assert!(matches!(
        compiled,
        ModifierExpr::Unsupported(Expr::Construct(_, _))
    ));
}

#[test]
fn compiled_ir_matches_evaluate_expr_for_supported_subset() {
    let expr = Expr::Conditional(
        Box::new(Expr::Binary(
            Box::new(Expr::Call(
                "sin".to_string(),
                vec![Expr::Ident("t".to_string())],
            )),
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

    let compiled = compile_modifier_expr(&expr);
    let mut env = Environment::new();
    load_standard_library(&mut env);
    env.set("t", Value::Num(std::f64::consts::FRAC_PI_2));
    env.set("pulse", Value::Num(3.0));

    let compiled_value =
        evaluate_modifier_expr(&compiled, &env).expect("compiled eval should work");
    let ast_value = evaluate_expr(&expr, &env).expect("ast eval should work");

    assert_eq!(compiled_value, ast_value);
}

#[test]
fn modifier_ir_can_lower_post_expansion_program() {
    let program = vec![
        Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::Always {
                body: vec![Stmt::Assignment {
                    target: vec!["pulse".to_string()],
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
fn modifier_ir_matches_statement_modifier_execution() {
    let mut timeline = Timeline::new();
    load_standard_library(timeline.env_mut());

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

    let program = vec![Stmt::Always {
        body: vec![modifier.clone()],
        span: None,
    }];
    let ir = lower_modifier_ir(&program).expect("lowering should succeed");

    let mut stmt_overrides = std::collections::HashMap::new();
    let mut stmt_env = timeline.build_frame_env(500, SceneDimensions::default(), &stmt_overrides);
    timeline.apply_modifier_stmt(&modifier, &mut stmt_env, &mut stmt_overrides);

    let mut ir_overrides = std::collections::HashMap::new();
    let mut ir_env = timeline.build_frame_env(500, SceneDimensions::default(), &ir_overrides);
    timeline
        .apply_modifier_ir_program(
            &ir,
            500,
            SceneDimensions::default(),
            &mut ir_env,
            &mut ir_overrides,
        )
        .expect("IR execution should succeed");

    assert_eq!(stmt_overrides, ir_overrides);
}

#[test]
fn modifier_bytecode_compiles_assignment_subset() {
    let program = vec![Stmt::Always {
        body: vec![Stmt::Assignment {
            target: vec!["pulse".to_string()],
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
    let bytecode = compile_modifier_bytecode(&ir).expect("bytecode compilation should succeed");
    let rendered = format!("{bytecode}");

    assert!(rendered.contains("LoadConst Num(1"));
    assert!(rendered.contains("WriteOverride pulse opacity"));
    assert!(rendered.contains("Halt"));
}

#[test]
fn modifier_bytecode_executes_let_and_if() {
    let program = vec![Stmt::Always {
        body: vec![
            Stmt::LetDecl { is_pub: false,
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
            },
        ],
        span: None,
    }];
    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    let bytecode = compile_modifier_bytecode(&ir).expect("bytecode compilation should succeed");

    let mut timeline = Timeline::new();
    load_standard_library(timeline.env_mut());
    let mut overrides = HashMap::new();
    let mut env = timeline.build_frame_env(500, SceneDimensions::default(), &overrides);
    timeline
        .apply_modifier_bytecode_program(
            &bytecode,
            500,
            SceneDimensions::default(),
            &mut env,
            &mut overrides,
        )
        .expect("bytecode execution should succeed");

    assert_eq!(overrides["pulse"]["opacity"], Value::Num(0.0));
}

#[test]
fn modifier_bytecode_rejects_unsupported_ir_expr() {
    let program = vec![Stmt::Always {
        body: vec![Stmt::Assignment {
            target: vec!["pulse".to_string()],
            property: "opacity".to_string(),
            value: Expr::Construct(
                "Point".to_string(),
                vec![],
            ),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }],
        span: None,
    }];

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    let error = compile_modifier_bytecode(&ir).expect_err("bytecode compilation should fail");
    assert_eq!(error, VmCompileError::UnsupportedExpr);
}

#[test]
fn vm_parity_reactive_runtime_matches_ir() {
    let expanded = load_fixture_program(REACTIVE_FIXTURE);
    let timeline = Timeline::build(&expanded);
    let ir = lower_modifier_ir(&expanded).expect("IR lowering should succeed");
    let bytecode = compile_modifier_bytecode(&ir).expect("bytecode compilation should succeed");

    for time_ms in [500_u64, 1500_u64] {
        let mut ir_overrides = HashMap::new();
        let mut ir_env = timeline.build_frame_env(time_ms, SceneDimensions::default(), &ir_overrides);
        timeline
            .apply_modifier_ir_program(
                &ir,
                time_ms,
                SceneDimensions::default(),
                &mut ir_env,
                &mut ir_overrides,
            )
            .expect("IR execution should succeed");

        let mut vm_overrides = HashMap::new();
        let mut vm_env = timeline.build_frame_env(time_ms, SceneDimensions::default(), &vm_overrides);
        timeline
            .apply_modifier_bytecode_program(
                &bytecode,
                time_ms,
                SceneDimensions::default(),
                &mut vm_env,
                &mut vm_overrides,
            )
            .expect("VM execution should succeed");

        assert_eq!(ir_overrides, vm_overrides);
    }
}

#[test]
fn vm_parity_nested_modifier_targets_match_ir() {
    let ast = vec![Stmt::Keyframe {
        time: Time::Seconds(0.0),
        body: vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "panel.badge".to_string(),
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
                        target: vec!["panel".to_string(), "badge".to_string()],
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
                        target: vec!["echo".to_string()],
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
    let bytecode = compile_modifier_bytecode(&ir).expect("bytecode compilation should succeed");

    let mut ir_overrides = HashMap::new();
    let mut ir_env = timeline.build_frame_env(1000, SceneDimensions::default(), &ir_overrides);
    timeline
        .apply_modifier_ir_program(
            &ir,
            1000,
            SceneDimensions::default(),
            &mut ir_env,
            &mut ir_overrides,
        )
        .expect("IR execution should succeed");

    let mut vm_overrides = HashMap::new();
    let mut vm_env = timeline.build_frame_env(1000, SceneDimensions::default(), &vm_overrides);
    timeline
        .apply_modifier_bytecode_program(
            &bytecode,
            1000,
            SceneDimensions::default(),
            &mut vm_env,
            &mut vm_overrides,
        )
        .expect("VM execution should succeed");

    assert_eq!(ir_overrides, vm_overrides);
}

fn load_fixture_program(source: &str) -> Vec<Stmt> {
    let temp_path = std::env::temp_dir().join("animatix_test_reactive.amx");
    fs::write(&temp_path, source).expect("write temp fixture should succeed");
    ModuleGraph::new()
        .load_program(&temp_path)
        .expect("program should load")
        .expand_components()
}
