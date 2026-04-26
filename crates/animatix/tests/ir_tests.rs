use animatix::ast::{BinaryOp, Expr, Stmt, Time};
use animatix::ir::{
    IrLowerError, ModifierExpr, compile_modifier_expr, evaluate_modifier_expr, lower_modifier_ir,
};
use animatix::module::ModuleGraph;
use animatix::timeline::{
    Environment, SceneDimensions, Timeline, Value, evaluate_expr, load_standard_library,
};
use animatix::vm::{VmCompileError, compile_modifier_bytecode};
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn ir_lowering_lowers_always_assignment_subset() {
    let program = vec![Stmt::Always {
        body: vec![Stmt::Assignment {
            target: vec!["pulse".to_string()],
            property: "opacity".to_string(),
            value: Expr::Num(1.0),
            modifiers: vec![],
        }],
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
                }],
                else_branch: Some(vec![Stmt::Assignment {
                    target: vec!["pulse".to_string()],
                    property: "opacity".to_string(),
                    value: Expr::Num(0.0),
                    modifiers: vec![],
                }]),
            },
        ],
    }];

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    let rendered = format!("{ir}");

    assert!(rendered.contains("let x = (load(t) Mul const(Num(2)))"));
    assert!(rendered.contains("if (load(x) Lt const(Num(1)))"));
    assert!(rendered.contains("assign pulse.opacity = const(Num(1))"));
    assert!(rendered.contains("assign pulse.opacity = const(Num(0))"));
}

#[test]
fn ir_lowering_rejects_unsupported_statement_forms() {
    let program = vec![Stmt::Always {
        body: vec![Stmt::ForLoop {
            var: "item".to_string(),
            iterable: Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0)]),
            body: vec![],
        }],
    }];

    let error = lower_modifier_ir(&program).expect_err("lowering should fail");
    assert_eq!(error, IrLowerError::UnsupportedStatement("for loop"));
}

#[test]
fn ir_lowering_rejects_unsupported_expression_forms() {
    let expr = Expr::Method(
        Box::new(Expr::Ident("graph".to_string())),
        "plot".to_string(),
        vec![],
    );

    let compiled = compile_modifier_expr(&expr);
    assert!(matches!(
        compiled,
        ModifierExpr::Unsupported(Expr::Method(_, _, _))
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
                }],
            }],
            span: None,
        },
        Stmt::Comment("ignored".to_string()),
    ];

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    assert_eq!(ir.statements.len(), 1);
    assert!(format!("{ir}").contains("assign pulse.opacity"));
}

#[test]
fn modifier_ir_matches_statement_modifier_execution() {
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
        }],
        else_branch: Some(vec![Stmt::Assignment {
            target: vec!["pulse".to_string()],
            property: "opacity".to_string(),
            value: Expr::Num(0.0),
            modifiers: vec![],
        }]),
    };

    let program = vec![Stmt::Always {
        body: vec![modifier.clone()],
    }];
    let ir = lower_modifier_ir(&program).expect("lowering should succeed");

    let mut stmt_overrides = std::collections::HashMap::new();
    let mut stmt_env = timeline.frame(500, SceneDimensions::default(), &stmt_overrides);
    timeline.apply_modifier_stmt_for_test(
        &modifier,
        500,
        SceneDimensions::default(),
        &mut stmt_env,
        &mut stmt_overrides,
    );

    let mut ir_overrides = std::collections::HashMap::new();
    let mut ir_env = timeline.frame(500, SceneDimensions::default(), &ir_overrides);
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
        }],
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
                }],
                else_branch: Some(vec![Stmt::Assignment {
                    target: vec!["pulse".to_string()],
                    property: "opacity".to_string(),
                    value: Expr::Num(0.0),
                    modifiers: vec![],
                }]),
            },
        ],
    }];
    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    let bytecode = compile_modifier_bytecode(&ir).expect("bytecode compilation should succeed");

    let mut timeline = Timeline::new();
    load_standard_library(&mut timeline.env);
    let mut overrides = HashMap::new();
    let mut env = timeline.frame(500, SceneDimensions::default(), &overrides);
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
            value: Expr::Method(
                Box::new(Expr::Ident("graph".to_string())),
                "plot".to_string(),
                vec![],
            ),
            modifiers: vec![],
        }],
    }];
    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    let error = compile_modifier_bytecode(&ir).expect_err("bytecode compilation should fail");
    assert_eq!(error, VmCompileError::UnsupportedExpr);
}

#[test]
fn vm_parity_reactive_runtime_matches_ir() {
    let expanded = load_example_program("examples/reactive.amx");
    let timeline = Timeline::build(&expanded);
    let ir = lower_modifier_ir(&expanded).expect("IR lowering should succeed");
    let bytecode = compile_modifier_bytecode(&ir).expect("bytecode compilation should succeed");

    for time_ms in [500_u64, 1500_u64] {
        let mut ir_overrides = HashMap::new();
        let mut ir_env = timeline.frame(time_ms, SceneDimensions::default(), &ir_overrides);
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
        let mut vm_env = timeline.frame(time_ms, SceneDimensions::default(), &vm_overrides);
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
                label: "panel.badge".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    animatix::ast::Property {
                        name: "radius".to_string(),
                        value: Expr::Num(12.0),
                    },
                    animatix::ast::Property {
                        name: "color".to_string(),
                        value: Expr::Ident("RED".to_string()),
                    },
                    animatix::ast::Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(400.0), Expr::Num(300.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
            },
            Stmt::ActorDecl {
                is_pub: false,
                label: "echo".to_string(),
                ty: "Circle".to_string(),
                props: vec![
                    animatix::ast::Property {
                        name: "radius".to_string(),
                        value: Expr::Num(10.0),
                    },
                    animatix::ast::Property {
                        name: "color".to_string(),
                        value: Expr::Ident("BLUE".to_string()),
                    },
                    animatix::ast::Property {
                        name: "at".to_string(),
                        value: Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(300.0)]),
                    },
                ],
                modifiers: vec![],
                children: vec![],
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
                    },
                ],
            },
        ],
        span: None,
    }];

    let timeline = Timeline::build(&ast);
    let ir = lower_modifier_ir(&ast).expect("IR lowering should succeed");
    let bytecode = compile_modifier_bytecode(&ir).expect("bytecode compilation should succeed");

    let mut ir_overrides = HashMap::new();
    let mut ir_env = timeline.frame(1000, SceneDimensions::default(), &ir_overrides);
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
    let mut vm_env = timeline.frame(1000, SceneDimensions::default(), &vm_overrides);
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

fn load_example_program(path: &str) -> Vec<Stmt> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate should live under workspace")
        .parent()
        .expect("workspace root should exist")
        .to_path_buf();
    let full_path = repo_root.join(path);
    ModuleGraph::new()
        .load_program(&full_path)
        .expect("program should load")
        .expand_components()
}
