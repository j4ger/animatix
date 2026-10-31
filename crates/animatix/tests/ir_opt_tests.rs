//! Tests for IR optimization passes (constant folding, dead branch elimination,
//! constant propagation, and match scrutinee single-evaluation CSE).

use std::collections::HashSet;

use animatix::timeline::modifier_runtime::ir::{
    BuiltinFn, CompiledExpr, ModifierIrProgram, ModifierIrStmt, ModifierOverrides, compile_expr,
    execute_modifier_ir, lower_modifier_body, lower_modifier_ir, optimize_expr, optimize_program,
};
use animatix::timeline::{Environment, Value};
use animatix_syntax::ast::{BinaryOp, Expr, MatchPattern, Stmt, TargetSegment, UnaryOp};

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

fn eval_expr_in_empty_env(expr: CompiledExpr) -> Value {
    let program = ModifierIrProgram::without_fast(vec![ModifierIrStmt::Let {
        name: "__res".to_string(),
        value: expr,
    }]);
    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, &mut env, &mut overrides).expect("execution should succeed");
    env.get("__res").expect("result should be bound")
}

#[test]
fn test_constant_unary_folding_parity() {
    // Negation of constant number
    let neg = CompiledExpr::Unary(UnaryOp::Neg, Box::new(CompiledExpr::Const(Value::Num(42.0))));
    assert_eq!(eval_expr_in_empty_env(neg), Value::Num(-42.0));

    // Logical NOT of truthy / falsy
    let not_true =
        CompiledExpr::Unary(UnaryOp::Not, Box::new(CompiledExpr::Const(Value::Bool(true))));
    assert_eq!(eval_expr_in_empty_env(not_true), Value::Num(0.0));

    let not_zero =
        CompiledExpr::Unary(UnaryOp::Not, Box::new(CompiledExpr::Const(Value::Num(0.0))));
    assert_eq!(eval_expr_in_empty_env(not_zero), Value::Num(1.0));
}

#[test]
fn test_constant_binary_arithmetic_parity() {
    let expr = CompiledExpr::Binary(
        Box::new(CompiledExpr::Const(Value::Num(10.0))),
        BinaryOp::Add,
        Box::new(CompiledExpr::Binary(
            Box::new(CompiledExpr::Const(Value::Num(3.0))),
            BinaryOp::Mul,
            Box::new(CompiledExpr::Const(Value::Num(4.0))),
        )),
    );
    assert_eq!(eval_expr_in_empty_env(expr), Value::Num(22.0));
}

#[test]
fn test_constant_binary_safe_div_zero() {
    let div_zero = CompiledExpr::Binary(
        Box::new(CompiledExpr::Const(Value::Num(10.0))),
        BinaryOp::Div,
        Box::new(CompiledExpr::Const(Value::Num(0.0))),
    );
    assert_eq!(eval_expr_in_empty_env(div_zero), Value::Num(0.0));
}

#[test]
fn test_constant_select_folding() {
    let select_true = CompiledExpr::Select(
        Box::new(CompiledExpr::Const(Value::Bool(true))),
        Box::new(CompiledExpr::Const(Value::Num(100.0))),
        Box::new(CompiledExpr::Const(Value::Num(200.0))),
    );
    assert_eq!(eval_expr_in_empty_env(select_true), Value::Num(100.0));

    let select_false = CompiledExpr::Select(
        Box::new(CompiledExpr::Const(Value::Num(0.0))),
        Box::new(CompiledExpr::Const(Value::Num(100.0))),
        Box::new(CompiledExpr::Const(Value::Num(200.0))),
    );
    assert_eq!(eval_expr_in_empty_env(select_false), Value::Num(200.0));
}

#[test]
fn test_constant_if_branch_execution() {
    let program = ModifierIrProgram::without_fast(vec![ModifierIrStmt::If {
        condition: CompiledExpr::Const(Value::Num(1.0)),
        then_branch: vec![ModifierIrStmt::Let {
            name: "branch".to_string(),
            value: CompiledExpr::Const(Value::Str("then".to_string())),
        }],
        else_branch: vec![ModifierIrStmt::Let {
            name: "branch".to_string(),
            value: CompiledExpr::Const(Value::Str("else".to_string())),
        }],
    }]);
    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, &mut env, &mut overrides).expect("execution succeeds");
    assert_eq!(env.get("branch"), Some(Value::Str("then".to_string())));
}

#[test]
fn test_optimize_expr_folds_constant_arithmetic() {
    // 4.0 / 2.0 -> Const(Num(2.0))
    let div_expr = CompiledExpr::Binary(
        Box::new(CompiledExpr::Const(Value::Num(4.0))),
        BinaryOp::Div,
        Box::new(CompiledExpr::Const(Value::Num(2.0))),
    );
    let (opt_div, stats_div) = optimize_expr(div_expr, &HashSet::new());
    assert_eq!(opt_div, CompiledExpr::Const(Value::Num(2.0)));
    assert_eq!(stats_div.folded, 1);
    assert_eq!(stats_div.nodes_before, 3);
    assert_eq!(stats_div.nodes_after, 1);

    // 2.0 * 3.141592653589793 -> Const(Num(6.283185307179586))
    let pi = 3.141592653589793_f64;
    let mul_expr = CompiledExpr::Binary(
        Box::new(CompiledExpr::Const(Value::Num(2.0))),
        BinaryOp::Mul,
        Box::new(CompiledExpr::Const(Value::Num(pi))),
    );
    let (opt_mul, stats_mul) = optimize_expr(mul_expr, &HashSet::new());
    assert_eq!(opt_mul, CompiledExpr::Const(Value::Num(2.0 * pi)));
    assert_eq!(stats_mul.folded, 1);

    // Nested constant folding: (2.0 + 3.0) * (10.0 - 4.0) -> Const(Num(30.0))
    let nested = CompiledExpr::Binary(
        Box::new(CompiledExpr::Binary(
            Box::new(CompiledExpr::Const(Value::Num(2.0))),
            BinaryOp::Add,
            Box::new(CompiledExpr::Const(Value::Num(3.0))),
        )),
        BinaryOp::Mul,
        Box::new(CompiledExpr::Binary(
            Box::new(CompiledExpr::Const(Value::Num(10.0))),
            BinaryOp::Sub,
            Box::new(CompiledExpr::Const(Value::Num(4.0))),
        )),
    );
    let (opt_nested, stats_nested) = optimize_expr(nested, &HashSet::new());
    assert_eq!(opt_nested, CompiledExpr::Const(Value::Num(30.0)));
    assert_eq!(stats_nested.folded, 3);
    assert_eq!(stats_nested.nodes_before, 7);
    assert_eq!(stats_nested.nodes_after, 1);
}

#[test]
fn test_optimize_expr_unary_and_builtin_folding() {
    // Unary Neg: - -42.0 -> Const(Num(42.0))
    let double_neg = CompiledExpr::Unary(
        UnaryOp::Neg,
        Box::new(CompiledExpr::Unary(
            UnaryOp::Neg,
            Box::new(CompiledExpr::Const(Value::Num(42.0))),
        )),
    );
    let (opt_neg, stats_neg) = optimize_expr(double_neg, &HashSet::new());
    assert_eq!(opt_neg, CompiledExpr::Const(Value::Num(42.0)));
    assert_eq!(stats_neg.folded, 2);

    // Unary Not: !0.0 -> Const(Num(1.0)), !1.0 -> Const(Num(0.0))
    let not_zero =
        CompiledExpr::Unary(UnaryOp::Not, Box::new(CompiledExpr::Const(Value::Num(0.0))));
    let (opt_not_zero, stats_not_zero) = optimize_expr(not_zero, &HashSet::new());
    assert_eq!(opt_not_zero, CompiledExpr::Const(Value::Num(1.0)));
    assert_eq!(stats_not_zero.folded, 1);

    // Builtin functions: sqrt(16.0) -> Const(Num(4.0)), abs(-5.0) -> Const(Num(5.0))
    let sqrt_expr =
        CompiledExpr::CallBuiltin(BuiltinFn::Sqrt, vec![CompiledExpr::Const(Value::Num(16.0))]);
    let (opt_sqrt, stats_sqrt) = optimize_expr(sqrt_expr, &HashSet::new());
    assert_eq!(opt_sqrt, CompiledExpr::Const(Value::Num(4.0)));
    assert_eq!(stats_sqrt.folded, 1);

    let abs_expr =
        CompiledExpr::CallBuiltin(BuiltinFn::Abs, vec![CompiledExpr::Const(Value::Num(-5.0))]);
    let (opt_abs, stats_abs) = optimize_expr(abs_expr, &HashSet::new());
    assert_eq!(opt_abs, CompiledExpr::Const(Value::Num(5.0)));
    assert_eq!(stats_abs.folded, 1);

    // Builtin functions: floor, ceil, round
    let floor_expr =
        CompiledExpr::CallBuiltin(BuiltinFn::Floor, vec![CompiledExpr::Const(Value::Num(3.7))]);
    assert_eq!(
        optimize_expr(floor_expr, &HashSet::new()).0,
        CompiledExpr::Const(Value::Num(3.0))
    );

    let ceil_expr =
        CompiledExpr::CallBuiltin(BuiltinFn::Ceil, vec![CompiledExpr::Const(Value::Num(3.2))]);
    assert_eq!(
        optimize_expr(ceil_expr, &HashSet::new()).0,
        CompiledExpr::Const(Value::Num(4.0))
    );

    let round_expr =
        CompiledExpr::CallBuiltin(BuiltinFn::Round, vec![CompiledExpr::Const(Value::Num(3.5))]);
    assert_eq!(
        optimize_expr(round_expr, &HashSet::new()).0,
        CompiledExpr::Const(Value::Num(4.0))
    );
}

#[test]
fn test_optimize_expr_shadowed_builtin_and_seeded_rand() {
    // When "sqrt" is in shadow_names, CallBuiltin(Sqrt, ...) must NOT be folded
    let mut shadows = HashSet::new();
    shadows.insert("sqrt".to_string());
    let sqrt_expr =
        CompiledExpr::CallBuiltin(BuiltinFn::Sqrt, vec![CompiledExpr::Const(Value::Num(16.0))]);
    let (opt_sqrt, stats_sqrt) = optimize_expr(sqrt_expr.clone(), &shadows);
    assert_eq!(opt_sqrt, sqrt_expr);
    assert_eq!(stats_sqrt.folded, 0);

    // SeededRand is non-constant / excluded from folding
    let rand_expr = CompiledExpr::CallBuiltin(
        BuiltinFn::SeededRand,
        vec![CompiledExpr::Const(Value::Num(42.0))],
    );
    let (opt_rand, stats_rand) = optimize_expr(rand_expr.clone(), &HashSet::new());
    assert_eq!(opt_rand, rand_expr);
    assert_eq!(stats_rand.folded, 0);

    // Invalid op on constants preserves Binary so runtime errors remain observable
    let invalid_sub = CompiledExpr::Binary(
        Box::new(CompiledExpr::Const(Value::Str("hello".to_string()))),
        BinaryOp::Sub,
        Box::new(CompiledExpr::Const(Value::Num(1.0))),
    );
    let (opt_inv, stats_inv) = optimize_expr(invalid_sub.clone(), &HashSet::new());
    assert_eq!(opt_inv, invalid_sub);
    assert_eq!(stats_inv.folded, 0);
}

#[test]
fn test_optimize_expr_eliminates_dead_select_branches() {
    // Select with truthy condition keeps then_branch, eliminates else_branch
    let select_true = CompiledExpr::Select(
        Box::new(CompiledExpr::Const(Value::Bool(true))),
        Box::new(CompiledExpr::Const(Value::Num(100.0))),
        Box::new(CompiledExpr::Const(Value::Num(200.0))),
    );
    let (opt_true, stats_true) = optimize_expr(select_true, &HashSet::new());
    assert_eq!(opt_true, CompiledExpr::Const(Value::Num(100.0)));
    assert_eq!(stats_true.dead_branches, 1);

    // Select with falsy condition keeps else_branch, eliminates then_branch
    let select_false = CompiledExpr::Select(
        Box::new(CompiledExpr::Const(Value::Num(0.0))),
        Box::new(CompiledExpr::Const(Value::Num(100.0))),
        Box::new(CompiledExpr::Const(Value::Num(200.0))),
    );
    let (opt_false, stats_false) = optimize_expr(select_false, &HashSet::new());
    assert_eq!(opt_false, CompiledExpr::Const(Value::Num(200.0)));
    assert_eq!(stats_false.dead_branches, 1);

    // Select with folded condition: (5.0 > 3.0) ? 10.0 : 20.0
    let select_computed = CompiledExpr::Select(
        Box::new(CompiledExpr::Binary(
            Box::new(CompiledExpr::Const(Value::Num(5.0))),
            BinaryOp::Gt,
            Box::new(CompiledExpr::Const(Value::Num(3.0))),
        )),
        Box::new(CompiledExpr::Const(Value::Num(10.0))),
        Box::new(CompiledExpr::Const(Value::Num(20.0))),
    );
    let (opt_comp, stats_comp) = optimize_expr(select_computed, &HashSet::new());
    assert_eq!(opt_comp, CompiledExpr::Const(Value::Num(10.0)));
    assert_eq!(stats_comp.folded, 1);
    assert_eq!(stats_comp.dead_branches, 1);
}

#[test]
fn test_optimize_let_chain_constant_propagation_and_pruning() {
    // let a = 2.0; let b = a * 3.0; b + 4.0 -> Const(Num(10.0))
    let chain = CompiledExpr::LetChain(
        vec![
            ("a".to_string(), CompiledExpr::Const(Value::Num(2.0))),
            (
                "b".to_string(),
                CompiledExpr::Binary(
                    Box::new(CompiledExpr::LoadEnv("a".to_string())),
                    BinaryOp::Mul,
                    Box::new(CompiledExpr::Const(Value::Num(3.0))),
                ),
            ),
        ],
        Box::new(CompiledExpr::Binary(
            Box::new(CompiledExpr::LoadEnv("b".to_string())),
            BinaryOp::Add,
            Box::new(CompiledExpr::Const(Value::Num(4.0))),
        )),
    );

    let (opt_chain, stats) = optimize_expr(chain, &HashSet::new());
    assert_eq!(opt_chain, CompiledExpr::Const(Value::Num(10.0)));
    assert_eq!(stats.folded, 2); // a * 3.0 folded, then b + 4.0 folded
    assert_eq!(stats.nodes_after, 1); // all bindings pruned

    // Partial constant propagation: let a = ext; let b = 2.0; a + b
    let partial_chain = CompiledExpr::LetChain(
        vec![
            ("a".to_string(), CompiledExpr::LoadEnv("ext".to_string())),
            ("b".to_string(), CompiledExpr::Const(Value::Num(2.0))),
        ],
        Box::new(CompiledExpr::Binary(
            Box::new(CompiledExpr::LoadEnv("a".to_string())),
            BinaryOp::Add,
            Box::new(CompiledExpr::LoadEnv("b".to_string())),
        )),
    );

    let (opt_partial, _) = optimize_expr(partial_chain, &HashSet::new());
    // "b" is a constant and can be pruned; "a" is non-constant and kept.
    match opt_partial {
        CompiledExpr::LetChain(bindings, tail) => {
            assert_eq!(bindings.len(), 1);
            assert_eq!(bindings[0].0, "a");
            assert_eq!(
                *tail,
                CompiledExpr::Binary(
                    Box::new(CompiledExpr::LoadEnv("a".to_string())),
                    BinaryOp::Add,
                    Box::new(CompiledExpr::Const(Value::Num(2.0))),
                )
            );
        },
        _ => panic!("expected LetChain with non-constant binding 'a'"),
    }
}

#[test]
fn test_optimize_program_eliminates_dead_if_branches() {
    // If with condition folding: 10.0 < 2.0 is false -> else branch inlined
    let mut program = ModifierIrProgram::without_fast(vec![ModifierIrStmt::If {
        condition: CompiledExpr::Binary(
            Box::new(CompiledExpr::Const(Value::Num(10.0))),
            BinaryOp::Lt,
            Box::new(CompiledExpr::Const(Value::Num(2.0))),
        ),
        then_branch: vec![ModifierIrStmt::Let {
            name: "target".to_string(),
            value: CompiledExpr::Const(Value::Num(1.0)),
        }],
        else_branch: vec![ModifierIrStmt::Let {
            name: "target".to_string(),
            value: CompiledExpr::Const(Value::Num(2.0)),
        }],
    }]);

    let stats = optimize_program(&mut program, &HashSet::new());
    assert_eq!(stats.folded, 1); // 10.0 < 2.0 folded
    assert_eq!(stats.dead_branches, 1); // then branch eliminated
    assert_eq!(program.statements.len(), 1);
    match &program.statements[0] {
        ModifierIrStmt::Let { name, value } => {
            assert_eq!(name, "target");
            assert_eq!(*value, CompiledExpr::Const(Value::Num(2.0)));
        },
        _ => panic!("expected inlined else_branch statement"),
    }

    // Execution verification
    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, &mut env, &mut overrides).expect("execution succeeds");
    assert_eq!(env.get("target"), Some(Value::Num(2.0)));
}

#[test]
fn test_match_compound_scrutinee_evaluated_once() {
    // 1. Compound scrutinee in compile_expr:
    // match (t + 1.0) { 2.0 => 100.0, _ => 0.0 }
    let compound_match = Expr::Match(
        Box::new(Expr::Binary(
            Box::new(Expr::Ident("t".to_string())),
            BinaryOp::Add,
            Box::new(Expr::Num(1.0)),
        )),
        vec![
            (MatchPattern::Num(2.0), Box::new(Expr::Num(100.0))),
            (MatchPattern::Wildcard, Box::new(Expr::Num(0.0))),
        ],
    );

    let compiled = compile_expr(&compound_match).expect("compile match");
    // Compound scrutinee must be bound to __amx_match_scrutinee in a LetChain
    match &compiled {
        CompiledExpr::LetChain(bindings, tail) => {
            assert_eq!(bindings.len(), 1);
            assert_eq!(bindings[0].0, "__amx_match_scrutinee");
            // Condition in tail Select references LoadEnv("__amx_match_scrutinee")
            assert!(tail.references_ident("__amx_match_scrutinee"));
            // Tail does NOT reference "t" directly (it is encapsulated in the binding)
            assert!(!tail.references_ident("t"));
        },
        _ => panic!("expected LetChain for compound scrutinee, got {compiled:?}"),
    }

    // 2. Simple scrutinee (Ident or Const) must NOT be wrapped in LetChain
    let simple_ident_match = Expr::Match(
        Box::new(Expr::Ident("t".to_string())),
        vec![
            (MatchPattern::Num(2.0), Box::new(Expr::Num(100.0))),
            (MatchPattern::Wildcard, Box::new(Expr::Num(0.0))),
        ],
    );
    let compiled_simple = compile_expr(&simple_ident_match).expect("compile simple match");
    assert!(
        matches!(compiled_simple, CompiledExpr::Select(..)),
        "simple ident match should lower directly to Select without LetChain"
    );

    let simple_num_match = Expr::Match(
        Box::new(Expr::Num(42.0)),
        vec![
            (MatchPattern::Num(2.0), Box::new(Expr::Num(100.0))),
            (MatchPattern::Wildcard, Box::new(Expr::Num(0.0))),
        ],
    );
    let compiled_num = compile_expr(&simple_num_match).expect("compile const match");
    assert!(
        matches!(compiled_num, CompiledExpr::Select(..)),
        "simple const match should lower directly to Select without LetChain"
    );

    // 3. Statement match with compound scrutinee lowers to Let + If
    let match_stmt = Stmt::Match {
        scrutinee: Expr::Binary(
            Box::new(Expr::Ident("t".to_string())),
            BinaryOp::Mul,
            Box::new(Expr::Num(2.0)),
        ),
        arms: vec![
            (
                MatchPattern::Num(4.0),
                vec![assignment(
                    vec![TargetSegment::Static("hero".to_string())],
                    "x",
                    Expr::Num(10.0),
                )],
            ),
            (
                MatchPattern::Wildcard,
                vec![assignment(
                    vec![TargetSegment::Static("hero".to_string())],
                    "x",
                    Expr::Num(20.0),
                )],
            ),
        ],
        span: None,
    };

    let program = lower_modifier_ir(&[Stmt::Always {
        body: vec![match_stmt],
        span: None,
    }])
    .expect("lower match stmt");
    assert_eq!(program.statements.len(), 2);
    match &program.statements[0] {
        ModifierIrStmt::Let { name, value } => {
            assert_eq!(name, "__amx_match_scrutinee");
            assert!(matches!(value, CompiledExpr::Binary(..)));
        },
        _ => panic!("first statement must be Let __amx_match_scrutinee"),
    }
    match &program.statements[1] {
        ModifierIrStmt::If { condition, .. } => {
            assert!(condition.references_ident("__amx_match_scrutinee"));
        },
        _ => panic!("second statement must be If"),
    }

    // 4. Execution of compound match statement verifies correct runtime behavior
    let mut env = Environment::new();
    env.set("t", Value::Num(2.0)); // t * 2.0 = 4.0 -> arm 1 matches
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, &mut env, &mut overrides).expect("execution succeeds");
    let hero_overrides = overrides.get("hero").expect("hero overrides");
    assert_eq!(hero_overrides.get("x"), Some(&Value::Num(10.0)));
}

#[test]
fn test_lower_modifier_body_optimizes_program() {
    // lower_modifier_body should automatically run optimize_program
    let stmts = vec![
        Stmt::LetDecl {
            name: "speed".to_string(),
            value: Expr::Binary(Box::new(Expr::Num(10.0)), BinaryOp::Mul, Box::new(Expr::Num(2.0))),
            is_pub: false,
            span: None,
        },
        Stmt::Conditional {
            condition: Expr::Bool(true),
            then_branch: vec![assignment(
                vec![TargetSegment::Static("box".to_string())],
                "opacity",
                Expr::Num(1.0),
            )],
            else_branch: Some(vec![assignment(
                vec![TargetSegment::Static("box".to_string())],
                "opacity",
                Expr::Num(0.0),
            )]),
            span: None,
        },
    ];

    let program = lower_modifier_body(&stmts).expect("lower body");
    // Dead else_branch should be eliminated and then_branch inlined directly
    assert_eq!(program.statements.len(), 2);
    match &program.statements[0] {
        ModifierIrStmt::Let { name, value } => {
            assert_eq!(name, "speed");
            assert_eq!(*value, CompiledExpr::Const(Value::Num(20.0)));
        },
        _ => panic!("expected folded Let statement"),
    }
    match &program.statements[1] {
        ModifierIrStmt::Assign {
            property, value, ..
        } => {
            assert_eq!(property, "opacity");
            assert_eq!(*value, CompiledExpr::Const(Value::Num(1.0)));
        },
        _ => panic!("expected inlined Assign statement"),
    }
}

#[test]
fn test_optimize_program_keeps_builtin_calls_for_runtime_shadowing() {
    use animatix::timeline::modifier_runtime::ir::{BuiltinFn, optimize_program};
    use std::collections::HashSet;

    let mut program = ModifierIrProgram::without_fast(vec![ModifierIrStmt::Let {
        name: "value".to_string(),
        value: CompiledExpr::CallBuiltin(
            BuiltinFn::Sqrt,
            vec![CompiledExpr::Const(Value::Num(16.0))],
        ),
    }]);

    optimize_program(&mut program, &HashSet::new());
    match &program.statements[0] {
        ModifierIrStmt::Let { value, .. } => assert!(
            matches!(value, CompiledExpr::CallBuiltin(BuiltinFn::Sqrt, _)),
            "program optimization must leave built-in calls for runtime shadow checks"
        ),
        _ => panic!("expected let statement"),
    }
}
