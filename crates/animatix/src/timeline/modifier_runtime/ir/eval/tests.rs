use std::collections::BTreeMap;

use super::*;
use crate::timeline::Environment;

#[test]
fn test_scalar_fast_compile_parity() {
    // (4 / pi) * sin(th - x)
    let expr = CompiledExpr::Binary(
        Box::new(CompiledExpr::Binary(
            Box::new(CompiledExpr::Const(Value::Num(4.0))),
            BinaryOp::Div,
            Box::new(CompiledExpr::LoadEnv("pi".to_string())),
        )),
        BinaryOp::Mul,
        Box::new(CompiledExpr::CallBuiltin(
            BuiltinFn::Sin,
            vec![CompiledExpr::Binary(
                Box::new(CompiledExpr::LoadEnv("th".to_string())),
                BinaryOp::Sub,
                Box::new(CompiledExpr::LoadEnv("x".to_string())),
            )],
        )),
    );

    let mut env = Environment::new();
    env.set("pi", Value::Num(std::f64::consts::PI));
    env.set("th", Value::Num(1.23));

    let captures = CapturedEnv::default();
    let mut slot_names = Vec::new();
    let fast_expr =
        compile_fast_scalar(&expr, "x", &mut slot_names).expect("Should compile to FastScalarExpr");

    let mut slots = Vec::new();
    assert!(resolve_slot_values(&slot_names, &env, &captures, &mut slots));

    for x in [0.0, 0.5, 1.0, 2.5] {
        let fast_val = fast_expr.eval(x, &slots);

        env.set("x", Value::Num(x));
        let slow_val = evaluate_compiled_expr(&expr, &env).unwrap().as_num();

        assert!((fast_val - slow_val).abs() < 1e-12, "Parity check failed at x={x}");
    }
}

#[test]
fn runtime_inputs_are_read_only_below_lexical_bindings_in_fast_and_slow_paths() {
    let body = CompiledExpr::Binary(
        Box::new(CompiledExpr::LoadEnv("x".to_string())),
        BinaryOp::Mul,
        Box::new(CompiledExpr::LoadEnv("gain".to_string())),
    );
    let mut env = Environment::new();
    env.set("gain", Value::Num(1.0));
    env.set_runtime_inputs(Some(std::sync::Arc::new(BTreeMap::from([
        ("gain".to_string(), Value::Num(2.5)),
        ("x".to_string(), Value::Num(999.0)),
    ]))));

    let mut slot_names = Vec::new();
    let fast_expr = compile_fast_scalar(&body, "x", &mut slot_names)
        .expect("x * gain should compile to the scalar fast path");
    assert_eq!(slot_names, vec!["gain".to_string()]);

    // A closure argument shadows the same-named external input. The other
    // slot resolves through the runtime overlay in both evaluators.
    let captures = CapturedEnv::default();
    let mut slots = Vec::new();
    assert!(resolve_slot_values(&slot_names, &env, &captures, &mut slots));
    assert_eq!(slots, vec![2.5]);
    for x in [-1.0, 0.25, 3.0] {
        env.set_binding("x", Value::Num(x));
        let slow = evaluate_compiled_expr(&body, &env).unwrap().as_num();
        let fast = fast_expr.eval(x, &slots);
        assert!((fast - slow).abs() < 1e-12, "fast/slow mismatch at x={x}");
        env.clear_bindings();
    }

    // A lexical let binding has the same priority, including in fast slot
    // resolution, while the overlay remains above the global driver.
    env.push_let_scope([("gain".to_string(), Value::Num(4.0))]);
    assert!(resolve_slot_values(&slot_names, &env, &captures, &mut slots));
    assert_eq!(slots, vec![4.0]);
    env.set_binding("x", Value::Num(2.0));
    let slow = evaluate_compiled_expr(&body, &env).unwrap().as_num();
    let fast = fast_expr.eval(2.0, &slots);
    assert_eq!(slow, 8.0);
    assert_eq!(fast, slow);
    env.clear_bindings();
    env.pop_let_scope();

    // With lexical values gone, the read-only input takes precedence over
    // the mutable global driver value.
    assert_eq!(env.get("gain"), Some(Value::Num(2.5)));
}

#[test]
fn test_vec2_fast_compile_parametric() {
    let expr = CompiledExpr::MakeVec(vec![
        CompiledExpr::CallBuiltin(BuiltinFn::Cos, vec![CompiledExpr::LoadEnv("t".to_string())]),
        CompiledExpr::CallBuiltin(BuiltinFn::Sin, vec![CompiledExpr::LoadEnv("t".to_string())]),
    ]);

    let mut slot_names = Vec::new();
    let (fx, fy) = compile_fast_vec2(&expr, "t", &mut slot_names)
        .expect("Should compile to FastScalarExpr vec2");

    let env = Environment::new();
    let captures = CapturedEnv::default();
    let mut slots = Vec::new();
    assert!(resolve_slot_values(&slot_names, &env, &captures, &mut slots));

    let x = fx.eval(0.5, &slots);
    let y = fy.eval(0.5, &slots);
    assert!((x - 0.5_f64.cos()).abs() < 1e-12);
    assert!((y - 0.5_f64.sin()).abs() < 1e-12);
}

#[test]
fn test_scalar_fast_compile_rejects_unsupported() {
    let expr = CompiledExpr::Method(
        Box::new(CompiledExpr::LoadEnv("list".to_string())),
        "len".to_string(),
        vec![],
    );
    let mut slot_names = Vec::new();
    assert!(compile_fast_scalar(&expr, "x", &mut slot_names).is_none());
}

#[test]
fn test_muladd_fusion_and_evaluation() {
    let x_mul_2 = || {
        Box::new(CompiledExpr::Binary(
            Box::new(CompiledExpr::LoadEnv("x".to_string())),
            BinaryOp::Mul,
            Box::new(CompiledExpr::Const(Value::Num(2.0))),
        ))
    };
    let c3 = || Box::new(CompiledExpr::Const(Value::Num(3.0)));
    let left_mul = CompiledExpr::Binary(x_mul_2(), BinaryOp::Add, c3());
    let right_mul = CompiledExpr::Binary(c3(), BinaryOp::Add, x_mul_2());
    let mut slots = Vec::new();
    let expected = FastScalarExpr::MulAdd(
        Box::new(FastScalarExpr::Arg),
        Box::new(FastScalarExpr::Const(2.0)),
        Box::new(FastScalarExpr::Const(3.0)),
    );
    let f_left = compile_fast_scalar(&left_mul, "x", &mut slots).unwrap();
    let f_right = compile_fast_scalar(&right_mul, "x", &mut slots).unwrap();
    assert_eq!(f_left, expected);
    assert_eq!(f_right, expected);
    assert_eq!(f_left.eval(5.0, &[]), 13.0);
}

#[test]
fn test_fast_scalar_fold_slots_and_contains_arg() {
    let inv_expr = FastScalarExpr::Div(
        Box::new(FastScalarExpr::Const(4.0)),
        Box::new(FastScalarExpr::Slot(0)),
    );
    assert!(!inv_expr.contains_arg());
    assert_eq!(
        inv_expr.fold_slots(&[std::f64::consts::PI]),
        FastScalarExpr::Const(4.0 / std::f64::consts::PI)
    );

    let expr = FastScalarExpr::Mul(
        Box::new(inv_expr),
        Box::new(FastScalarExpr::Sin(Box::new(FastScalarExpr::Sub(
            Box::new(FastScalarExpr::Slot(1)),
            Box::new(FastScalarExpr::Arg),
        )))),
    );
    assert!(expr.contains_arg());
    let slots = [std::f64::consts::PI, 1.23];
    let folded = expr.fold_slots(&slots);
    assert!(folded.contains_arg());
    for x in [0.0, 0.5, 1.0, 2.5] {
        assert!((expr.eval(x, &slots) - folded.eval(x, &[])).abs() < 1e-12);
    }

    let all_const = FastScalarExpr::MulAdd(
        Box::new(FastScalarExpr::Slot(0)),
        Box::new(FastScalarExpr::Slot(1)),
        Box::new(FastScalarExpr::Slot(2)),
    );
    assert_eq!(all_const.fold_slots(&[2.0, 3.0, 4.0]), FastScalarExpr::Const(10.0));

    let with_arg = FastScalarExpr::MulAdd(
        Box::new(FastScalarExpr::Slot(0)),
        Box::new(FastScalarExpr::Arg),
        Box::new(FastScalarExpr::Slot(1)),
    );
    assert_eq!(
        with_arg.fold_slots(&[2.0, 4.0]),
        FastScalarExpr::MulAdd(
            Box::new(FastScalarExpr::Const(2.0)),
            Box::new(FastScalarExpr::Arg),
            Box::new(FastScalarExpr::Const(4.0))
        )
    );
}
