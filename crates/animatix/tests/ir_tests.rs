use std::collections::HashMap;
use std::fs;

use animatix::timeline::modifier_runtime::ir::{
    CompiledExpr, ModifierIrProgram, ModifierIrStmt, ModifierOverrides, compile_expr,
    execute_modifier_ir, lower_modifier_ir,
};
use animatix::timeline::{
    Environment, EvalError, SceneDimensions, Timeline, Value, evaluate_expr, load_standard_library,
};
use animatix_syntax::ast::{BinaryOp, Expr, LoopPattern, Property, Stmt, TargetSegment, Time};
use animatix_syntax::module::ModuleGraph;

fn evaluate_modifier_via_ir(value: CompiledExpr, env: &mut Environment) -> Value {
    let program = ModifierIrProgram::new(vec![ModifierIrStmt::Let {
        name: "__ir_test_result".to_string(),
        value,
    }]);
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, env, &mut overrides).expect("IR execution should succeed");
    env.get("__ir_test_result").expect("IR result should be stored")
}

fn num(value: f64) -> Expr {
    Expr::Num(value)
}

fn ident(name: &str) -> Expr {
    Expr::Ident(name.to_string())
}

fn bin(left: Expr, op: BinaryOp, right: Expr) -> Expr {
    Expr::Binary(Box::new(left), op, Box::new(right))
}

fn path(segments: &[&str]) -> Expr {
    Expr::Path(segments.iter().map(|segment| segment.to_string()).collect())
}

fn static_target(segments: &[&str]) -> Vec<TargetSegment> {
    segments
        .iter()
        .map(|segment| TargetSegment::Static(segment.to_string()))
        .collect()
}

fn indexed_target(base: &str, index: Expr) -> Vec<TargetSegment> {
    vec![TargetSegment::Indexed {
        base: base.to_string(),
        index: Box::new(index),
    }]
}

fn assignment(target: Vec<TargetSegment>, property: &str, value: Expr) -> Stmt {
    Stmt::Assignment {
        target,
        property: property.to_string(),
        value,
        modifiers: vec![],
        easing: None,
        value_span: None,
        span: None,
    }
}

fn let_decl(name: &str, value: Expr) -> Stmt {
    Stmt::LetDecl {
        is_pub: false,
        name: name.to_string(),
        value,
        span: None,
    }
}

fn always(body: Vec<Stmt>) -> Vec<Stmt> {
    vec![Stmt::Always { body, span: None }]
}

fn actor(label: &str, ty: &str, props: &[(&str, Expr)]) -> Stmt {
    Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: ty.to_string(),
        props: props
            .iter()
            .map(|(name, value)| Property {
                name: name.to_string(),
                value: value.clone(),
                value_span: None,
                trailing_comment: None,
            })
            .collect(),
        modifiers: vec![],
        children: vec![],
        span: None,
    }
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
    let program = always(vec![assignment(static_target(&["pulse"]), "opacity", num(1.0))]);

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    let rendered = format!("{ir}");

    assert_eq!(ir.statements.len(), 1);
    assert!(rendered.contains("assign pulse.opacity = const(Num(1))"));
}

#[test]
fn ir_lowering_bare_always_assignment_writes_frame_variable() {
    let value = bin(ident("t"), BinaryOp::Mul, num(2.0));
    let program = always(vec![assignment(vec![], "freq", value.clone())]);

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    assert_eq!(
        ir.statements,
        vec![ModifierIrStmt::Let {
            name: "freq".to_string(),
            value: compile_expr(&value).expect("expression should compile"),
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
    let program = always(vec![
        let_decl("x", bin(ident("t"), BinaryOp::Mul, num(2.0))),
        Stmt::Conditional {
            condition: bin(ident("x"), BinaryOp::Lt, num(1.0)),
            then_branch: vec![assignment(static_target(&["pulse"]), "opacity", num(1.0))],
            else_branch: Some(vec![assignment(static_target(&["pulse"]), "opacity", num(0.0))]),
            span: None,
        },
    ]);

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    let rendered = format!("{ir}");

    assert!(rendered.contains("let x = (load(t) Mul const(Num(2)))"));
    assert!(rendered.contains("if (load(x) Lt const(Num(1)))"));
    assert!(rendered.contains("assign pulse.opacity = const(Num(1))"));
    assert!(rendered.contains("assign pulse.opacity = const(Num(0))"));
}

#[test]
fn ir_lowering_ignores_comments_in_always_blocks() {
    let program = always(vec![Stmt::Comment("test".to_string(), None)]);

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
        Box::new(bin(Expr::call("sin".to_string(), vec![ident("t")]), BinaryOp::Gt, num(0.0))),
        Box::new(Expr::call(
            "format".to_string(),
            vec![Expr::Str("pulse={}".to_string()), ident("pulse")],
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
        Expr::call("signum".to_string(), vec![num(-2.5)]),
        Expr::call("fract".to_string(), vec![num(3.75)]),
        Expr::call("hypot".to_string(), vec![num(3.0), num(4.0)]),
        Expr::call("pow".to_string(), vec![num(2.0), num(10.0)]),
        Expr::call("rem".to_string(), vec![num(7.0), num(4.0)]),
        Expr::call("step".to_string(), vec![num(1.0), num(0.5)]),
        Expr::call("round".to_string(), vec![num(2.6)]),
        Expr::call("seeded_rand".to_string(), vec![num(42.0)]),
        Expr::call("factorial".to_string(), vec![num(5.0)]),
        Expr::call("sum".to_string(), vec![Expr::List(vec![num(1.0), num(2.0), num(3.5)])]),
        // >4 numeric items stay a Value::List (not a Vec4), so the sum arm
        // actually takes the list branch through the IR too.
        Expr::call("sum".to_string(), vec![Expr::List((0..8).map(|i| num(i as f64)).collect())]),
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
fn factorial_and_sum_reject_invalid_args() {
    // Both paths must surface the same errors for bad SCALAR arguments; an
    // incorrect lowering must not bypass the builtin's own type validation.
    let cases: Vec<Expr> = vec![
        Expr::call("factorial".to_string(), vec![num(-1.0)]),
        Expr::call("factorial".to_string(), vec![num(2.5)]),
        Expr::call("factorial".to_string(), vec![num(171.0)]),
    ];
    for expr in cases {
        let ast_result = evaluate_expr(&expr, &Environment::new());
        assert!(ast_result.is_err(), "AST eval should error for {expr:?}, got {:?}", ast_result);

        let compiled = compile_expr(&expr).expect("builtin should compile");
        let program = ModifierIrProgram::new(vec![ModifierIrStmt::Let {
            name: "__ir_test_result".to_string(),
            value: compiled,
        }]);
        let mut env = Environment::new();
        load_standard_library(&mut env);
        let mut overrides = ModifierOverrides::default();
        let ir_result = execute_modifier_ir(&program, &mut env, &mut overrides);
        assert!(ir_result.is_err(), "IR eval should error for {expr:?}, got {:?}", ir_result);
    }
}

/// Like `evaluate_modifier_via_ir` but returns the executor's Result so
/// error-path assertions can inspect it.
fn evaluate_modifier_via_ir_result(
    value: CompiledExpr,
    env: &mut Environment,
) -> Result<Value, EvalError> {
    let program = ModifierIrProgram::new(vec![ModifierIrStmt::Let {
        name: "__ir_test_result".to_string(),
        value,
    }]);
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, env, &mut overrides)?;
    Ok(env.get("__ir_test_result").expect("IR result should be stored"))
}

#[test]
fn sum_mixed_type_list_errors_on_both_paths() {
    // Formerly a documented divergence: the IR lowered brace lists via
    // MakeVec, coercing "x" -> 0.0 into a Vec2 and silently summing 1.
    // Lowering now distinguishes lists (MakeList, elements preserved) from
    // tuples (MakeVec, numeric), so both paths keep the Str and sum()
    // rejects it identically.
    let expr =
        Expr::call("sum".to_string(), vec![Expr::List(vec![num(1.0), Expr::Str("x".to_string())])]);
    assert!(evaluate_expr(&expr, &Environment::new()).is_err());

    let compiled = compile_expr(&expr).expect("builtin should compile");
    let mut env = Environment::new();
    load_standard_library(&mut env);
    let ir_err = evaluate_modifier_via_ir_result(compiled, &mut env);
    assert!(ir_err.is_err(), "IR must reject non-numeric elements now");
}

#[test]
fn brace_list_preserves_elements_through_ir() {
    // {1, 2, 3} lowered via MakeList must evaluate to Value::List([Num; 3])
    // — identical to the tree-walker — instead of the old MakeVec Vec3
    // coercion. Tuples stay on MakeVec.
    let list_expr = Expr::List(vec![num(1.0), num(2.0), num(3.0)]);
    let tuple_expr = Expr::Tuple(vec![num(1.0), num(2.0), num(3.0)]);

    // Tree-walker expectations.
    let walker_list = evaluate_expr(&list_expr, &Environment::new()).expect("list eval");
    assert!(
        matches!(walker_list, Value::List(_)),
        "walker keeps brace lists as List, got {walker_list:?}"
    );
    let walker_tuple = evaluate_expr(&tuple_expr, &Environment::new()).expect("tuple eval");
    assert!(
        matches!(walker_tuple, Value::Vec3(_)),
        "walker coerces 3-tuples to Vec3, got {walker_tuple:?}"
    );

    // IR parity for both.
    let compiled_list = compile_expr(&list_expr).expect("list compiles");
    let mut env = Environment::new();
    load_standard_library(&mut env);
    let ir_list = evaluate_modifier_via_ir(compiled_list, &mut env);
    assert_eq!(ir_list, walker_list, "IR list must match tree-walker");

    let compiled_tuple = compile_expr(&tuple_expr).expect("tuple compiles");
    let mut env2 = Environment::new();
    load_standard_library(&mut env2);
    let ir_tuple = evaluate_modifier_via_ir(compiled_tuple, &mut env2);
    assert_eq!(ir_tuple, walker_tuple, "IR tuple must match tree-walker");
}

#[test]
fn rgb_and_rgba_use_only_normalized_channels() {
    let mut env = Environment::new();
    load_standard_library(&mut env);
    for byte_channel in [0.99, 1.0, 1.01, 2.0, 255.0] {
        let level = byte_channel / 255.0;
        for (name, alpha, args) in [
            ("rgb", 1.0, vec![num(level); 3]),
            ("rgba", 0.5, vec![num(level), num(level), num(level), num(0.5)]),
        ] {
            let expr = Expr::call(name, args);
            let expected = Value::Color([level, level, level, alpha]);
            assert_eq!(evaluate_expr(&expr, &env).unwrap(), expected);
            assert_eq!(evaluate_modifier_via_ir(compile_expr(&expr).unwrap(), &mut env), expected);
        }
    }

    for (name, channel, args) in [
        ("rgb", "r", vec![num(1.01), num(1.0), num(1.0)]),
        ("rgb", "g", vec![num(0.0), num(-0.01), num(0.0)]),
        ("rgb", "b", vec![num(0.0), num(0.0), num(f64::NAN)]),
        ("rgba", "a", vec![num(0.0), num(0.0), num(0.0), num(128.0)]),
    ] {
        let error = evaluate_expr(&Expr::call(name, args), &env).unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&format!("{name} channel {channel} must be between 0 and 1"))
        );
    }
}

#[test]
fn ir_matches_ast_for_environment_functions() {
    let cases: Vec<Expr> = vec![
        Expr::call("rgb".to_string(), vec![num(1.0), num(0.0), num(0.5)]),
        Expr::call("rgba".to_string(), vec![num(1.0), num(0.5), num(0.0), num(0.25)]),
        Expr::call("vec2".to_string(), vec![num(3.0), num(4.0)]),
        Expr::call("hsv".to_string(), vec![num(120.0), num(1.0), num(1.0)]),
        Expr::call("seeded_rand".to_string(), vec![num(42.0)]),
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
            cue_group: None,
            time: Time::Seconds(0.0),
            body: always(vec![assignment(
                static_target(&["pulse"]),
                "opacity",
                bin(ident("t"), BinaryOp::Mul, num(0.5)),
            )]),
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
    let program = always(vec![assignment(
        indexed_target("bars", ident("i")),
        "scale",
        num(1.5),
    )]);

    let ir = lower_modifier_ir(&program).expect("IR lowering should succeed");
    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    env.set("i", Value::Num(2.0));
    execute_modifier_ir(&ir, &mut env, &mut overrides).expect("IR execution should succeed");
    assert_eq!(overrides["bars__2"]["scale"], Value::Num(1.5));
}

#[test]
fn modifier_ir_executes_let_and_if() {
    let program = always(vec![
        let_decl("x", bin(ident("t"), BinaryOp::Mul, num(2.0))),
        Stmt::Conditional {
            condition: bin(ident("x"), BinaryOp::Lt, num(1.0)),
            then_branch: vec![assignment(static_target(&["pulse"]), "opacity", num(1.0))],
            else_branch: Some(vec![assignment(static_target(&["pulse"]), "opacity", num(0.0))]),
            span: None,
        },
    ]);
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

fn assert_object_write(target: &[&str], property: &str, mut env: Environment, read_path: &[&str]) {
    let program = always(vec![
        assignment(static_target(target), property, num(30.0)),
        let_decl("q", bin(path(read_path), BinaryOp::Add, num(1.0))),
    ]);

    let ir = lower_modifier_ir(&program).expect("lowering should succeed");
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&ir, &mut env, &mut overrides).expect("IR execution should succeed");
    assert_eq!(env.get("q"), Some(Value::Num(31.0)));
    assert!(overrides.is_empty(), "object writes should not create actor overrides");
}

#[test]
fn ir_supports_runtime_object_field_writes() {
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

    assert_object_write(&["p"], "x", make_env(), &["p", "x"]);
}

#[test]
fn ir_supports_nested_object_field_writes() {
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

    assert_object_write(&["p", "a"], "b", make_env(), &["p", "a", "b"]);
}

#[test]
fn modifier_ir_executes_construct_expr() {
    let program = always(vec![assignment(
        static_target(&["pulse"]),
        "opacity",
        Expr::Construct("Point".to_string(), vec![]),
    )]);

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
    let (program, expanded) = load_fixture_program(REACTIVE_FIXTURE);
    let timeline = Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        animatix::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    )
    .output;
    let ir = lower_modifier_ir(&expanded.statements).expect("IR lowering should succeed");

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
        cue_group: None,
        time: Time::Seconds(0.0),
        body: vec![
            actor(
                "panel.badge",
                "Ellipse",
                &[
                    ("radius", num(12.0)),
                    ("color", ident("RED")),
                    ("at", Expr::Tuple(vec![num(400.0), num(300.0)])),
                ],
            ),
            actor(
                "echo",
                "Ellipse",
                &[
                    ("radius", num(10.0)),
                    ("color", ident("BLUE")),
                    ("at", Expr::Tuple(vec![num(500.0), num(300.0)])),
                ],
            ),
            Stmt::Always {
                body: vec![
                    assignment(
                        static_target(&["panel", "badge"]),
                        "radius",
                        bin(ident("t"), BinaryOp::Add, num(10.0)),
                    ),
                    assignment(
                        static_target(&["echo"]),
                        "radius",
                        path(&["panel", "badge", "radius"]),
                    ),
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
fn ir_nested_loops_restore_shadowed_bindings_even_after_errors() {
    let loop_stmt = |body| ModifierIrStmt::For {
        var: LoopPattern::Single("item".into()),
        index_var: Some("i".into()),
        iterable: CompiledExpr::Const(Value::List(vec![Value::Num(10.0), Value::Num(20.0)].into())),
        body,
    };
    let mut env = Environment::new();
    env.set("item", Value::Num(7.0));
    env.set("i", Value::Num(9.0));
    let inner = ModifierIrStmt::For {
        var: LoopPattern::Single("item".into()),
        index_var: Some("i".into()),
        iterable: CompiledExpr::Const(Value::List(vec![Value::Num(42.0)].into())),
        body: vec![],
    };
    let program = ModifierIrProgram::new(vec![loop_stmt(vec![
        inner,
        ModifierIrStmt::Let {
            name: "outer_index".into(),
            value: CompiledExpr::LoadEnv("i".into()),
        },
    ])]);
    execute_modifier_ir(&program, &mut env, &mut ModifierOverrides::default()).unwrap();
    assert_eq!(env.get("outer_index"), Some(Value::Num(1.0)));
    assert_eq!(env.get("item"), Some(Value::Num(7.0)));
    assert_eq!(env.get("i"), Some(Value::Num(9.0)));

    let failing = ModifierIrProgram::new(vec![loop_stmt(vec![ModifierIrStmt::Let {
        name: "unreachable_result".into(),
        value: CompiledExpr::LoadEnv("missing_loop_value".into()),
    }])]);
    assert!(execute_modifier_ir(&failing, &mut env, &mut ModifierOverrides::default()).is_err());
    assert_eq!(env.get("item"), Some(Value::Num(7.0)));
    assert_eq!(env.get("i"), Some(Value::Num(9.0)));
}

#[test]
fn ir_for_loop_binds_index_var() {
    // Test for-loops through the IR interpreter, including loop variable cleanup.
    let stmt = Stmt::ForLoop {
        var: LoopPattern::Single("v".to_string()),
        index_var: Some("i".to_string()),
        iterable: Expr::List(vec![num(10.0), num(20.0), num(30.0)]),
        modifiers: vec![],
        body: vec![let_decl(
            "z",
            bin(ident("v"), BinaryOp::Mul, bin(ident("i"), BinaryOp::Add, num(1.0))),
        )],
        span: None,
    };

    let program = always(vec![stmt]);
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
        target_key: "actor".to_string(),
        property: property.to_string(),
        value,
    };

    // `if true` must take the then-branch.
    let program = ModifierIrProgram::new(vec![ModifierIrStmt::If {
        condition: CompiledExpr::Const(Value::Bool(true)),
        then_branch: vec![assign("a", CompiledExpr::Const(Value::Num(1.0)))],
        else_branch: vec![assign("a", CompiledExpr::Const(Value::Num(0.0)))],
    }]);
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
    let program = ModifierIrProgram::new(vec![ModifierIrStmt::Let {
        name: "r".to_string(),
        value: select,
    }]);
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
    let program = ModifierIrProgram::new(vec![
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
    ]);
    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, &mut env, &mut overrides).expect("execute");
    assert_eq!(env.get("n"), Some(Value::Bool(false)));
    assert_eq!(env.get("and"), Some(Value::Bool(false)));
    assert_eq!(env.get("or"), Some(Value::Bool(true)));
}

#[test]
fn ir_for_loop_tuple_destructures_color() {
    let program = ModifierIrProgram::new(vec![ModifierIrStmt::For {
        var: LoopPattern::Tuple(["r", "g", "b", "a"].iter().map(|name| name.to_string()).collect()),
        index_var: None,
        iterable: CompiledExpr::Const(Value::Color([0.1, 0.2, 0.3, 1.0])),
        body: vec![ModifierIrStmt::Let {
            name: "__captured_color".to_string(),
            value: CompiledExpr::LoadEnv("r".to_string()),
        }],
    }]);

    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, &mut env, &mut overrides).expect("IR execution should succeed");
    assert_eq!(env.get("__captured_color"), Some(Value::Num(0.1)));
    assert_eq!(env.get("r"), None, "loop variables should be cleaned up");
}

#[test]
fn indexed_dotted_targets_keep_prefix_and_match_fast_and_generic_execution() {
    let statements = vec![Stmt::Assignment {
        target: vec![
            TargetSegment::Static("parent".to_string()),
            TargetSegment::Indexed {
                base: "cards".to_string(),
                index: Box::new(ident("i")),
            },
            TargetSegment::Static("title_text".to_string()),
        ],
        property: "opacity".to_string(),
        value: num(0.5),
        modifiers: vec![],
        easing: None,
        value_span: None,
        span: None,
    }];
    let fast = animatix::timeline::modifier_runtime::ir::lower_modifier_body(&statements)
        .expect("lower indexed assignment");
    let indexed = fast.statements.first().expect("one assignment");
    assert!(matches!(
        indexed,
        ModifierIrStmt::AssignIndexed {
            base,
            index_position: 1,
            ..
        } if base == "parent.cards.title_text"
    ));
    let mut written = std::collections::HashSet::new();
    fast.collect_written_targets(&mut written);
    assert!(written.contains("parent.cards.title_text"));
    assert!(written.contains("parent.cards"));
    let mut excluded = std::collections::HashSet::new();
    fast.collect_stats_excluded_targets(&mut excluded);
    assert!(excluded.contains("parent.cards.title_text"));
    assert!(excluded.contains("parent.cards"));

    let run = |program: &ModifierIrProgram| {
        let mut env = Environment::new();
        env.set("i", Value::Num(3.0));
        let mut overrides = ModifierOverrides::default();
        execute_modifier_ir(program, &mut env, &mut overrides).expect("execute assignment");
        overrides
    };
    let fast_overrides = run(&fast);
    let generic = ModifierIrProgram::without_fast(fast.statements.clone());
    let generic_overrides = run(&generic);
    assert_eq!(fast_overrides, generic_overrides);
    assert_eq!(
        fast_overrides
            .get("parent.cards__3.title_text")
            .and_then(|properties| properties.get("opacity")),
        Some(&Value::Num(0.5))
    );
}

fn load_fixture_program(
    source: &str,
) -> (animatix_syntax::module::LoadedProgram, animatix_syntax::module::ExpandedProgram) {
    let temp_dir = tempfile::tempdir().expect("create temp fixture directory");
    let temp_path = temp_dir.path().join("animatix_test_reactive.amx");
    fs::write(&temp_path, source).expect("write temp fixture should succeed");
    let mut graph = ModuleGraph::new();
    let program = graph.load_program(&temp_path).expect("program should load");
    let expanded = program.expand_components().expect("component expansion");
    (program, expanded)
}

#[test]
fn letchain_ir_matches_ast() {
    // Block-bodied closures: IR and tree-walker must agree on sequential
    // bindings with shadowing, and on a body that reads the outer plot arg.
    use animatix_syntax::ast::{BinaryOp as BO, Expr as E};

    // (v) => { let a = v * 2; let a = a + 1; a * 3 }
    //      => ((v*2) + 1) * 3 at v=4 → 27
    let expr = E::Closure(
        vec!["v".to_string()],
        Box::new(E::LetChain(
            vec![
                (
                    "a".to_string(),
                    E::Binary(Box::new(E::Ident("v".into())), BO::Mul, Box::new(E::Num(2.0))),
                ),
                (
                    "a".to_string(),
                    E::Binary(Box::new(E::Ident("a".into())), BO::Add, Box::new(E::Num(1.0))),
                ),
            ],
            Box::new(E::Binary(Box::new(E::Ident("a".into())), BO::Mul, Box::new(E::Num(3.0)))),
        )),
    );
    let call = E::call("apply".to_string(), vec![expr.clone(), E::Num(4.0)]);
    // No apply builtin exists — invoke the closure directly via the env: bind
    // it to a name and call it.
    let mut env = Environment::new();
    load_standard_library(&mut env);
    let closure_value = evaluate_expr(&expr, &env).expect("closure evaluates");
    env.set("my_fn", closure_value);
    let call_expr = E::call("my_fn".to_string(), vec![E::Num(4.0)]);

    // Tree-walker path.
    let ast_value = evaluate_expr(&call_expr, &env).expect("ast eval");
    assert_eq!(ast_value, Value::Num(27.0), "shadowing order: ((4*2)+1)*3");

    // IR path: compile the same call and execute.
    let compiled = compile_expr(&call_expr).expect("call compiles");
    let ir_value = evaluate_modifier_via_ir(compiled, &mut env);
    assert_eq!(ir_value, Value::Num(27.0), "IR must match the tree-walker");

    let _ = call; // call_expr is the invoked form; the raw closure+arg pair is unused
}

#[test]
fn noise_family_and_oklab_lerp_agree_between_ir_and_ast() {
    let color =
        |r: f64, g: f64, b: f64| Expr::call("rgb".to_string(), vec![num(r), num(g), num(b)]);
    let cases = vec![
        Expr::call("noise".to_string(), vec![num(1.5)]),
        Expr::call("noise2".to_string(), vec![num(1.5), num(-2.0)]),
        Expr::call("seeded_noise".to_string(), vec![num(7.0), num(3.25)]),
        Expr::call("seeded_noise2".to_string(), vec![num(7.0), num(3.25), num(9.0)]),
        Expr::call("fbm".to_string(), vec![num(1.5), num(4.0)]),
        Expr::call("seeded_fbm".to_string(), vec![num(7.0), num(1.5), num(3.0)]),
        Expr::call(
            "lerp_color_oklab".to_string(),
            vec![color(1.0, 0.0, 0.0), color(0.0, 1.0, 0.0), num(0.5)],
        ),
    ];

    for expr in &cases {
        let compiled = compile_expr(expr).expect("environment call should compile");
        let mut env = Environment::new();
        load_standard_library(&mut env);
        let ir_value = evaluate_modifier_via_ir(compiled, &mut env);
        let ast_value = evaluate_expr(expr, &env).expect("ast eval should work");
        assert_eq!(ir_value, ast_value, "IR and AST disagree for {expr:?}");
    }

    // Value-level assertions on the same surface an `always` block uses.
    let mut env = Environment::new();
    load_standard_library(&mut env);
    let eval = |expr: &Expr| evaluate_expr(expr, &env).expect("eval");

    // Noise is deterministic and bounded.
    let n = eval(&Expr::call("noise".to_string(), vec![num(1.5)]));
    let n_again = eval(&Expr::call("noise".to_string(), vec![num(1.5)]));
    assert_eq!(n, n_again, "noise must be pure");
    let Value::Num(n) = n else {
        panic!("noise must return Num, got {n:?}")
    };
    assert!((0.0..=1.0).contains(&n), "noise in [0,1], got {n}");

    // More octaves change the mix but stay in range.
    let f3 = eval(&Expr::call("fbm".to_string(), vec![num(1.5), num(3.0)]));
    let f6 = eval(&Expr::call("fbm".to_string(), vec![num(1.5), num(6.0)]));
    assert_ne!(f3, f6, "octave count must matter");
    for v in [f3, f6] {
        let Value::Num(v) = v else {
            panic!("fbm must return Num")
        };
        assert!((0.0..=1.0).contains(&v), "fbm in [0,1], got {v}");
    }

    // OKLab lerp: achromatic mixes stay achromatic and between; a saturated
    // midpoint stays in gamut (the raw sRGB lerp of red→green is muddy olive).
    let white = Expr::call("rgb".to_string(), vec![num(1.0); 3]);
    let black = Expr::call("rgb".to_string(), vec![num(0.0); 3]);
    let gray = eval(&Expr::call("lerp_color_oklab".to_string(), vec![white, black, num(0.5)]));
    let Value::Color(gray) = gray else {
        panic!("lerp_color_oklab must return Color, got {gray:?}")
    };
    assert!(
        (gray[0] - gray[1]).abs() < 1e-6 && (gray[1] - gray[2]).abs() < 1e-6,
        "achromatic midpoint must stay achromatic, got {gray:?}"
    );
    assert!(
        (0.3..0.7).contains(&gray[0]),
        "achromatic midpoint between black and white, got {}",
        gray[0]
    );

    let mid = eval(&Expr::call(
        "lerp_color_oklab".to_string(),
        vec![color(1.0, 0.0, 0.0), color(0.0, 1.0, 0.0), num(0.5)],
    ));
    let Value::Color(mid) = mid else {
        panic!("lerp_color_oklab must return Color, got {mid:?}")
    };
    assert!(
        mid.iter().all(|v| (0.0..=1.0).contains(v)),
        "midpoint stays in gamut, got {mid:?}"
    );
    // The whole point of OKLab: the perceptual midpoint of red→green is a
    // *bright* chartreuse, where the raw sRGB channel lerp dips to muddy
    // olive (r = g = 0.5, far darker in luminance terms).
    assert!(
        mid[0] > 0.65 && mid[1] > 0.5,
        "red→green OKLab midpoint must stay bright, got {mid:?}"
    );
    let srgb_mid = eval(&Expr::call(
        "lerp_color".to_string(),
        vec![color(1.0, 0.0, 0.0), color(0.0, 1.0, 0.0), num(0.5)],
    ));
    let Value::Color(srgb_mid) = srgb_mid else {
        panic!("lerp_color must return Color, got {srgb_mid:?}")
    };
    assert!(
        mid[0] > srgb_mid[0] && mid[1] > srgb_mid[1],
        "OKLab midpoint brighter than the sRGB one: oklab {mid:?} vs srgb {srgb_mid:?}"
    );

    // Endpoints survive the round trip exactly (up to encoding error).
    let back = eval(&Expr::call(
        "lerp_color_oklab".to_string(),
        vec![color(1.0, 0.0, 0.0), color(0.0, 1.0, 0.0), num(0.0)],
    ));
    let Value::Color(back) = back else {
        panic!("Color expected")
    };
    assert!((back[0] - 1.0).abs() < 1e-6 && back[1].abs() < 1e-6 && back[2].abs() < 1e-6);
}

#[test]
fn color_builtins_accept_typed_colors_and_reject_strings() {
    let mut env = Environment::new();
    load_standard_library(&mut env);
    let green = Expr::call("rgb".to_string(), vec![num(0.0), num(1.0), num(0.0)]);

    for name in ["lerp_color", "lerp_color_oklab"] {
        let lerp = |ink| {
            evaluate_expr(&Expr::call(name.to_string(), vec![ink, green.clone(), num(0.5)]), &env)
        };
        let from_hex = lerp(Expr::Color([255, 0, 0, 255])).unwrap();
        assert!(matches!(from_hex, Value::Color(_)));
        assert_eq!(from_hex, lerp(Expr::Ident("red".to_string())).unwrap());
        for text in ["#ff0000", "red"] {
            assert!(lerp(Expr::Str(text.to_string())).is_err(), "{name} accepted {text}");
        }
    }
}

#[test]
fn curve_at_and_curve_smooth_agree_between_ir_and_ast() {
    let curve_expr = Expr::List(vec![
        num(0.0),
        num(10.0),
        num(1.0),
        num(20.0),
        num(3.0),
        num(50.0),
    ]);

    let cases = vec![
        Expr::call("curve_at".to_string(), vec![curve_expr.clone(), num(0.5)]),
        Expr::call("curve_at".to_string(), vec![curve_expr.clone(), num(2.0)]),
        Expr::call("curve_at".to_string(), vec![curve_expr.clone(), num(-1.0)]),
        Expr::call("curve_smooth".to_string(), vec![curve_expr.clone(), num(1.5), num(0.6)]),
        Expr::call("curve_smooth".to_string(), vec![curve_expr, num(2.0), num(0.0)]),
    ];

    for expr in &cases {
        let compiled = compile_expr(expr).expect("curve builtin call should compile");
        let mut env = Environment::new();
        load_standard_library(&mut env);
        let ir_value = evaluate_modifier_via_ir(compiled, &mut env);
        let ast_value = evaluate_expr(expr, &env).expect("ast eval should work");
        assert_eq!(ir_value, ast_value, "IR and AST disagree for {expr:?}");
    }
}
