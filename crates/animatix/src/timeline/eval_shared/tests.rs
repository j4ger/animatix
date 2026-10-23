//! Evaluation helper tests.

use super::*;

use crate::timeline::Value;

// ─── Binary op tests ─────────────────────────────────────────────────

#[test]
fn test_binary_op_num_add() {
    let result = eval_binary_op(Value::Num(3.0), &BinaryOp::Add, Value::Num(4.0)).unwrap();
    assert_eq!(result.as_num(), 7.0);
}

#[test]
fn test_binary_op_num_sub() {
    let result = eval_binary_op(Value::Num(10.0), &BinaryOp::Sub, Value::Num(3.0)).unwrap();
    assert_eq!(result.as_num(), 7.0);
}

#[test]
fn test_binary_op_num_mul() {
    let result = eval_binary_op(Value::Num(6.0), &BinaryOp::Mul, Value::Num(7.0)).unwrap();
    assert_eq!(result.as_num(), 42.0);
}

#[test]
fn test_binary_op_num_div() {
    let result = eval_binary_op(Value::Num(10.0), &BinaryOp::Div, Value::Num(2.0)).unwrap();
    assert_eq!(result.as_num(), 5.0);
}

#[test]
fn test_binary_op_num_div_by_zero() {
    let result = eval_binary_op(Value::Num(10.0), &BinaryOp::Div, Value::Num(0.0)).unwrap();
    assert_eq!(result.as_num(), 0.0);
}

#[test]
fn test_binary_op_num_mod() {
    let result = eval_binary_op(Value::Num(10.0), &BinaryOp::Mod, Value::Num(3.0)).unwrap();
    assert_eq!(result.as_num(), 1.0);
}

#[test]
fn test_binary_op_num_pow() {
    let result = eval_binary_op(Value::Num(2.0), &BinaryOp::Pow, Value::Num(3.0)).unwrap();
    assert_eq!(result.as_num(), 8.0);
}

#[test]
fn test_binary_op_num_eq() {
    let result = eval_binary_op(Value::Num(5.0), &BinaryOp::Eq, Value::Num(5.0)).unwrap();
    assert_eq!(result.as_num(), 1.0);
    let result = eval_binary_op(Value::Num(5.0), &BinaryOp::Eq, Value::Num(6.0)).unwrap();
    assert_eq!(result.as_num(), 0.0);
}

#[test]
fn test_binary_op_num_neq() {
    let result = eval_binary_op(Value::Num(5.0), &BinaryOp::Neq, Value::Num(6.0)).unwrap();
    assert_eq!(result.as_num(), 1.0);
    let result = eval_binary_op(Value::Num(5.0), &BinaryOp::Neq, Value::Num(5.0)).unwrap();
    assert_eq!(result.as_num(), 0.0);
}

#[test]
fn test_binary_op_num_lt_gt_lte_gte() {
    assert_eq!(
        eval_binary_op(Value::Num(3.0), &BinaryOp::Lt, Value::Num(5.0))
            .unwrap()
            .as_num(),
        1.0
    );
    assert_eq!(
        eval_binary_op(Value::Num(5.0), &BinaryOp::Gt, Value::Num(3.0))
            .unwrap()
            .as_num(),
        1.0
    );
    assert_eq!(
        eval_binary_op(Value::Num(3.0), &BinaryOp::Lte, Value::Num(3.0))
            .unwrap()
            .as_num(),
        1.0
    );
    assert_eq!(
        eval_binary_op(Value::Num(3.0), &BinaryOp::Gte, Value::Num(3.0))
            .unwrap()
            .as_num(),
        1.0
    );
}

#[test]
fn test_binary_op_num_and_or() {
    assert_eq!(
        eval_binary_op(Value::Num(1.0), &BinaryOp::And, Value::Num(1.0))
            .unwrap()
            .as_num(),
        1.0
    );
    assert_eq!(
        eval_binary_op(Value::Num(1.0), &BinaryOp::And, Value::Num(0.0))
            .unwrap()
            .as_num(),
        0.0
    );
    assert_eq!(
        eval_binary_op(Value::Num(0.0), &BinaryOp::Or, Value::Num(1.0))
            .unwrap()
            .as_num(),
        1.0
    );
    assert_eq!(
        eval_binary_op(Value::Num(0.0), &BinaryOp::Or, Value::Num(0.0))
            .unwrap()
            .as_num(),
        0.0
    );
}

#[test]
fn test_binary_op_vec2_add() {
    let result =
        eval_binary_op(Value::Vec2([1.0, 2.0]), &BinaryOp::Add, Value::Vec2([3.0, 4.0])).unwrap();
    assert_eq!(result.as_vec2(), [4.0, 6.0]);
}

#[test]
fn test_binary_op_vec2_scalar_mul() {
    let result = eval_binary_op(Value::Vec2([2.0, 3.0]), &BinaryOp::Mul, Value::Num(4.0)).unwrap();
    assert_eq!(result.as_vec2(), [8.0, 12.0]);

    let result = eval_binary_op(Value::Num(4.0), &BinaryOp::Mul, Value::Vec2([2.0, 3.0])).unwrap();
    assert_eq!(result.as_vec2(), [8.0, 12.0]);
}

#[test]
fn test_binary_op_color_add() {
    let result = eval_binary_op(
        Value::Color([0.5, 0.5, 0.5, 1.0]),
        &BinaryOp::Add,
        Value::Color([0.25, 0.25, 0.25, 0.0]),
    )
    .unwrap();
    assert_eq!(result.as_color(), [0.75, 0.75, 0.75, 1.0]);
}

#[test]
fn test_binary_op_eq_fallback() {
    // Eq/Neq on non-numeric types should still work (structural equality)
    let result = eval_binary_op(
        Value::Str("hello".to_string()),
        &BinaryOp::Eq,
        Value::Str("hello".to_string()),
    )
    .unwrap();
    assert_eq!(result.as_num(), 1.0);

    let result = eval_binary_op(
        Value::Str("hello".to_string()),
        &BinaryOp::Neq,
        Value::Str("world".to_string()),
    )
    .unwrap();
    assert_eq!(result.as_num(), 1.0);
}

#[test]
fn test_binary_op_type_mismatch() {
    let result = eval_binary_op(
        Value::Str("hello".to_string()),
        &BinaryOp::Add,
        Value::Str("world".to_string()),
    );
    assert!(result.is_err());
}

// ─── Builtin function tests ──────────────────────────────────────────

#[test]
fn test_builtin_sin_cos() {
    let result = eval_builtin_fn("sin", &[Value::Num(0.0)]).unwrap();
    assert!((result.as_num()).abs() < 1e-10);

    let result = eval_builtin_fn("cos", &[Value::Num(std::f64::consts::PI)]).unwrap();
    assert!((result.as_num() + 1.0).abs() < 1e-10);
}

#[test]
fn test_builtin_abs() {
    let result = eval_builtin_fn("abs", &[Value::Num(-42.0)]).unwrap();
    assert_eq!(result.as_num(), 42.0);
}

#[test]
fn test_builtin_sqrt() {
    let result = eval_builtin_fn("sqrt", &[Value::Num(9.0)]).unwrap();
    assert_eq!(result.as_num(), 3.0);
}

#[test]
fn test_builtin_exp_log() {
    let x = eval_builtin_fn("exp", &[Value::Num(1.0)]).unwrap();
    assert!((x.as_num() - std::f64::consts::E).abs() < 1e-10);

    let result = eval_builtin_fn("ln", &[x]).unwrap();
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    let result = eval_builtin_fn("log", &[Value::Num(std::f64::consts::E)]).unwrap();
    assert!((result.as_num() - 1.0).abs() < 1e-10);
}

#[test]
fn test_builtin_atan2() {
    let result = eval_builtin_fn("atan2", &[Value::Num(0.0), Value::Num(1.0)]).unwrap();
    assert!((result.as_num()).abs() < 1e-10);
}

#[test]
fn test_builtin_clamp() {
    let result =
        eval_builtin_fn("clamp", &[Value::Num(5.0), Value::Num(0.0), Value::Num(10.0)]).unwrap();
    assert_eq!(result.as_num(), 5.0);

    let result =
        eval_builtin_fn("clamp", &[Value::Num(-5.0), Value::Num(0.0), Value::Num(10.0)]).unwrap();
    assert_eq!(result.as_num(), 0.0);

    let result =
        eval_builtin_fn("clamp", &[Value::Num(15.0), Value::Num(0.0), Value::Num(10.0)]).unwrap();
    assert_eq!(result.as_num(), 10.0);
}

#[test]
fn test_builtin_min_max() {
    let result = eval_builtin_fn("min", &[Value::Num(3.0), Value::Num(7.0)]).unwrap();
    assert_eq!(result.as_num(), 3.0);

    let result = eval_builtin_fn("max", &[Value::Num(3.0), Value::Num(7.0)]).unwrap();
    assert_eq!(result.as_num(), 7.0);
}

#[test]
fn test_builtin_floor_ceil() {
    let result = eval_builtin_fn("floor", &[Value::Num(3.7)]).unwrap();
    assert_eq!(result.as_num(), 3.0);

    let result = eval_builtin_fn("ceil", &[Value::Num(3.2)]).unwrap();
    assert_eq!(result.as_num(), 4.0);
}

#[test]
fn test_builtin_deg_rad() {
    let result = eval_builtin_fn("deg", &[Value::Num(90.0)]).unwrap();
    assert!((result.as_num() - std::f64::consts::FRAC_PI_2).abs() < 1e-10);

    let result = eval_builtin_fn("rad", &[Value::Num(std::f64::consts::PI)]).unwrap();
    assert!((result.as_num() - 180.0).abs() < 1e-10);
}

#[test]
fn test_builtin_lerp() {
    let result =
        eval_builtin_fn("lerp", &[Value::Num(0.0), Value::Num(10.0), Value::Num(0.5)]).unwrap();
    assert_eq!(result.as_num(), 5.0);
}

#[test]
fn test_builtin_format() {
    let result =
        eval_builtin_fn("format", &[Value::Str("value: {}".to_string()), Value::Num(42.0)])
            .unwrap();
    assert_eq!(result.as_str(), "value: 42");

    let result = eval_builtin_fn(
        "format",
        &[
            Value::Str("x={}, y={}".to_string()),
            Value::Num(10.0),
            Value::Num(20.0),
        ],
    )
    .unwrap();
    assert_eq!(result.as_str(), "x=10, y=20");

    let result = eval_builtin_fn("format", &[]).unwrap();
    assert_eq!(result.as_str(), "");
}

#[test]
fn test_builtin_format_precision() {
    // `{:.N}` formats the numeric value with exactly N fractional digits
    // (live readouts print 0.7071067811865476 without it).
    let result = eval_builtin_fn(
        "format",
        &[
            Value::Str("y = {:.2}".to_string()),
            Value::Num(std::f64::consts::FRAC_1_SQRT_2),
        ],
    )
    .unwrap();
    assert_eq!(result.as_str(), "y = 0.71");

    // Multiple placeholders mix precision and default formatting.
    let result = eval_builtin_fn(
        "format",
        &[
            Value::Str("{} -> {:.3} @ {}".to_string()),
            Value::Num(2.0),
            Value::Num(1.0 / 3.0),
            Value::Str("s".to_string()),
        ],
    )
    .unwrap();
    assert_eq!(result.as_str(), "2 -> 0.333 @ s");

    // Precision on a string value falls back to the plain string.
    let result = eval_builtin_fn(
        "format",
        &[
            Value::Str("{:.2}".to_string()),
            Value::Str("hi".to_string()),
        ],
    )
    .unwrap();
    assert_eq!(result.as_str(), "hi");

    // Unrecognized specs stay literal instead of silently consuming a value.
    let result =
        eval_builtin_fn("format", &[Value::Str("{:d} {}".to_string()), Value::Num(7.0)]).unwrap();
    assert_eq!(result.as_str(), "{:d} 7");

    // More placeholders than values keeps the extras literal.
    let result =
        eval_builtin_fn("format", &[Value::Str("{} and {}".to_string()), Value::Num(1.0)]).unwrap();
    assert_eq!(result.as_str(), "1 and {}");
}

#[test]
fn test_builtin_factorial_and_sum() {
    // factorial: integral non-negative only.
    let result = eval_builtin_fn("factorial", &[Value::Num(5.0)]).unwrap();
    assert_eq!(result, Value::Num(120.0));
    let result = eval_builtin_fn("factorial", &[Value::Num(0.0)]).unwrap();
    assert_eq!(result, Value::Num(1.0));
    assert!(eval_builtin_fn("factorial", &[Value::Num(-1.0)]).is_err());
    assert!(eval_builtin_fn("factorial", &[Value::Num(2.5)]).is_err());
    assert!(eval_builtin_fn("factorial", &[Value::Num(171.0)]).is_err());

    // sum: lists of any length; 2-4 numeric items evaluate to Vec2/3/4
    // and sum over their components.
    let result = eval_builtin_fn(
        "sum",
        &[Value::List(
            vec![
                Value::Num(1.0),
                Value::Num(2.0),
                Value::Num(3.0),
                Value::Num(4.0),
                Value::Num(5.0),
            ]
            .into(),
        )],
    )
    .unwrap();
    assert_eq!(result, Value::Num(15.0));
    let result = eval_builtin_fn("sum", &[Value::Vec3([1.0, 2.0, 3.5])]).unwrap();
    assert_eq!(result, Value::Num(6.5));
    let result = eval_builtin_fn("sum", &[Value::List(Vec::new().into())]).unwrap();
    assert_eq!(result, Value::Num(0.0));
    assert!(
        eval_builtin_fn("sum", &[Value::Num(1.0)]).is_err(),
        "sum of a scalar must be a type error"
    );
    assert!(
        eval_builtin_fn("sum", &[Value::List(vec![Value::Str("x".to_string())].into())]).is_err(),
        "sum must reject non-numeric elements"
    );
}

#[test]
fn test_builtin_unknown() {
    let result = eval_builtin_fn("nonexistent", &[]);
    assert!(result.is_err());
}

#[test]
fn test_builtin_wrong_arg_count() {
    let result = eval_builtin_fn("sin", &[]);
    assert!(result.is_err());
    let result = eval_builtin_fn("sin", &[Value::Num(1.0), Value::Num(2.0)]);
    assert!(result.is_err());
}

// ─── Type conversion helpers ──────────────────────────────────────────

#[test]
fn test_value_to_f64() {
    assert_eq!(value_to_f64(&Value::Num(3.25)).unwrap(), 3.25);
    assert!(value_to_f64(&Value::Str("x".to_string())).is_err());
}

#[test]
fn test_value_to_vec2() {
    assert_eq!(value_to_vec2(&Value::Vec2([1.0, 2.0])).unwrap(), [1.0, 2.0]);
    assert!(value_to_vec2(&Value::Num(0.0)).is_err());
}

#[test]
fn test_value_to_vec3() {
    assert_eq!(value_to_vec3(&Value::Vec3([1.0, 2.0, 3.0])).unwrap(), [1.0, 2.0, 3.0]);
    assert!(value_to_vec3(&Value::Num(0.0)).is_err());
}

#[test]
fn test_value_to_color() {
    assert_eq!(
        value_to_color(&Value::Color([0.5, 0.5, 0.5, 1.0])).unwrap(),
        [0.5, 0.5, 0.5, 1.0]
    );
    assert!(value_to_color(&Value::Num(0.0)).is_err());
}

#[test]
fn test_value_to_list() {
    assert_eq!(
        value_to_list(&Value::List(vec![Value::Num(1.0)].into())).unwrap(),
        vec![Value::Num(1.0)]
    );
    assert!(value_to_list(&Value::Num(0.0)).is_err());
}

// ─── Curve function tests (STAT-1) ────────────────────────────────────

#[test]
fn test_eval_curve_at() {
    // Curve: [(0.0, 10.0), (1.0, 20.0), (3.0, 40.0)]
    let curve = Value::List(
        vec![
            Value::Num(0.0),
            Value::Num(10.0),
            Value::Num(1.0),
            Value::Num(20.0),
            Value::Num(3.0),
            Value::Num(40.0),
        ]
        .into(),
    );

    // Clamping before start and after end
    assert_eq!(
        eval_builtin_fn("curve_at", &[curve.clone(), Value::Num(-1.0)])
            .unwrap()
            .as_num(),
        10.0
    );
    assert_eq!(
        eval_builtin_fn("curve_at", &[curve.clone(), Value::Num(5.0)]).unwrap().as_num(),
        40.0
    );

    // Exact points
    assert_eq!(
        eval_builtin_fn("curve_at", &[curve.clone(), Value::Num(0.0)]).unwrap().as_num(),
        10.0
    );
    assert_eq!(
        eval_builtin_fn("curve_at", &[curve.clone(), Value::Num(1.0)]).unwrap().as_num(),
        20.0
    );

    // Interpolated points
    assert_eq!(
        eval_builtin_fn("curve_at", &[curve.clone(), Value::Num(0.5)]).unwrap().as_num(),
        15.0
    );
    assert_eq!(
        eval_builtin_fn("curve_at", &[curve.clone(), Value::Num(2.0)]).unwrap().as_num(),
        30.0
    );

    // Odd length error
    let odd_curve = Value::List(vec![Value::Num(0.0), Value::Num(10.0), Value::Num(1.0)].into());
    assert!(eval_builtin_fn("curve_at", &[odd_curve, Value::Num(0.5)]).is_err());
}

#[test]
fn test_eval_curve_smooth() {
    // Constant curve: [(0.0, 25.0), (4.0, 25.0)]
    let constant_curve = Value::List(
        vec![
            Value::Num(0.0),
            Value::Num(25.0),
            Value::Num(4.0),
            Value::Num(25.0),
        ]
        .into(),
    );

    // Constant signal preserves constant value
    let smoothed = eval_builtin_fn(
        "curve_smooth",
        &[constant_curve.clone(), Value::Num(2.0), Value::Num(0.5)],
    )
    .unwrap()
    .as_num();
    assert!(
        (smoothed - 25.0).abs() < 1e-6,
        "smoothed constant must be ~25.0, got {smoothed}"
    );

    // tau <= 0 evaluates identity curve_at
    let instant =
        eval_builtin_fn("curve_smooth", &[constant_curve, Value::Num(2.0), Value::Num(0.0)])
            .unwrap()
            .as_num();
    assert_eq!(instant, 25.0);

    // Linear ramp: [(0.0, 0.0), (10.0, 10.0)]
    let ramp = Value::List(
        vec![
            Value::Num(0.0),
            Value::Num(0.0),
            Value::Num(10.0),
            Value::Num(10.0),
        ]
        .into(),
    );
    // At t = 5.0 with tau = 0.5, a 1-pole filter lags by approx tau = 0.5 -> ~4.5
    let ramp_val = eval_builtin_fn("curve_smooth", &[ramp, Value::Num(5.0), Value::Num(0.5)])
        .unwrap()
        .as_num();
    assert!(
        (ramp_val - 4.5).abs() < 0.05,
        "ramp smoothed at t=5 tau=0.5 should lag by ~0.5 (near 4.5), got {ramp_val}"
    );
}
