use std::collections::HashMap;

use super::*;
use crate::ast::{BinaryOp, Expr, Time};
use crate::timeline::env::{CapturedEnv, Environment, EvalError, Value};
use crate::timeline::load_standard_library;
use crate::timeline::modifier_runtime::ir::{CompiledExpr, compile_expr};
use crate::timeline::property_track::TrackAccessor;

fn compiled_body(body: Expr) -> Box<CompiledExpr> {
    Box::new(compile_expr(&body).expect("test closure body should compile"))
}

#[test]
fn eval_cache_reflects_env_mutations() {
    // Same env instance, unmutated → cached value is reused.
    let mut env = Environment::new();
    env.set("x", Value::Num(2.0));
    let expr = Expr::Call("sin".to_string(), vec![Expr::Ident("x".to_string())]);
    let first = evaluate_expr(&expr, &env).expect("eval should succeed");

    // Mutating the environment must invalidate memoized values even
    // though the (expr, instance) pair is unchanged.
    env.set("x", Value::Num(0.0));
    let second = evaluate_expr(&expr, &env).expect("eval should succeed");
    assert_eq!(first.as_num(), 2.0f64.sin());
    assert_eq!(second.as_num(), 0.0f64.sin());

    // A clone shares contents but not cache identity: mutating one side
    // must never corrupt the other's memoized values.
    let mut clone = env.clone();
    clone.set("x", Value::Num(std::f64::consts::FRAC_PI_2));
    let from_clone = evaluate_expr(&expr, &clone).expect("eval should succeed");
    let from_original = evaluate_expr(&expr, &env).expect("eval should succeed");
    assert_eq!(from_clone.as_num(), 1.0);
    assert_eq!(from_original.as_num(), 0.0f64.sin());
}

#[test]
fn direct_overrides_removal_invalidates_cache() {
    // Code paths that mutate `overrides` directly (e.g. loop-variable
    // cleanup) must bump the stamp via mark_mutated.
    let mut env = Environment::new();
    env.set("loop_var", Value::Num(3.0));
    let expr = Expr::Binary(
        Box::new(Expr::Ident("loop_var".to_string())),
        BinaryOp::Add,
        Box::new(Expr::Num(1.0)),
    );
    let with_var = evaluate_expr(&expr, &env).expect("eval should succeed");
    assert_eq!(with_var.as_num(), 4.0);

    env.overrides.remove("loop_var");
    env.mark_mutated();
    let without_var = evaluate_expr(&expr, &env);
    assert!(without_var.is_err(), "removed variable must no longer resolve");
}

#[test]
fn test_evaluate_closure() {
    // Use an env with a (empty) base to satisfy the stdlib-base invariant
    // checked by the debug_assert in evaluate_call.
    let mut env = Environment::with_base(std::sync::Arc::new(std::collections::HashMap::new()));
    let closure = Value::Closure(
        vec!["x".to_string()],
        compiled_body(Expr::Binary(
            Box::new(Expr::Ident("x".to_string())),
            BinaryOp::Mul,
            Box::new(Expr::Num(2.0)),
        )),
        CapturedEnv::default(),
    );
    env.set("f", closure);

    let call_expr = Expr::Call("f".to_string(), vec![Expr::Num(4.0)]);
    let result = evaluate_expr(&call_expr, &env).expect("Evaluation failed");

    assert_eq!(result, Value::Num(8.0));
}

#[test]
fn test_evaluate_method_string_length() {
    let mut env = Environment::new();
    env.set("text", Value::Str("hello".to_string()));
    let expr =
        Expr::Method(Box::new(Expr::Ident("text".to_string())), "length".to_string(), vec![]);

    let result = evaluate_expr(&expr, &env).unwrap();
    assert_eq!(result.as_num(), 5.0);
}

#[test]
fn test_evaluate_method_string_split() {
    let mut env = Environment::new();
    env.set("text", Value::Str("a,b,c".to_string()));
    let expr = Expr::Method(
        Box::new(Expr::Ident("text".to_string())),
        "split".to_string(),
        vec![Expr::Str(",".to_string())],
    );

    let result = evaluate_expr(&expr, &env).unwrap();
    let list = result.as_list();
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].as_str(), "a");
    assert_eq!(list[1].as_str(), "b");
    assert_eq!(list[2].as_str(), "c");
}

#[test]
fn test_evaluate_method_list_length() {
    let mut env = Environment::new();
    env.set("items", Value::List(vec![Value::Num(1.0), Value::Num(2.0)].into()));
    let expr =
        Expr::Method(Box::new(Expr::Ident("items".to_string())), "length".to_string(), vec![]);

    let result = evaluate_expr(&expr, &env).unwrap();
    assert_eq!(result.as_num(), 2.0);
}

#[test]
fn test_evaluate_method_list_get() {
    let mut env = Environment::new();
    env.set(
        "items",
        Value::List(vec![Value::Num(10.0), Value::Num(20.0), Value::Num(30.0)].into()),
    );
    let expr = Expr::Method(
        Box::new(Expr::Ident("items".to_string())),
        "get".to_string(),
        vec![Expr::Num(2.0)],
    );

    let result = evaluate_expr(&expr, &env).unwrap();
    assert_eq!(result.as_num(), 30.0);
}

#[test]
fn test_evaluate_object_field_access() {
    let mut env = Environment::new();
    let mut fields = HashMap::new();
    fields.insert("x".to_string(), Value::Num(10.0));
    fields.insert("y".to_string(), Value::Num(20.0));
    env.set("point", Value::Object("Point".to_string(), fields));

    let expr = Expr::Method(Box::new(Expr::Ident("point".to_string())), "x".to_string(), vec![]);

    let result = evaluate_expr(&expr, &env).unwrap();
    assert_eq!(result.as_num(), 10.0);
}

#[test]
fn test_evaluate_object_field_missing() {
    let mut env = Environment::new();
    let fields = HashMap::new();
    env.set("point", Value::Object("Point".to_string(), fields));

    let expr = Expr::Method(Box::new(Expr::Ident("point".to_string())), "z".to_string(), vec![]);

    let result = evaluate_expr(&expr, &env);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("z"));
}

#[test]
fn test_evaluate_method_num_abs() {
    let mut env = Environment::new();
    env.set("x", Value::Num(-42.5));
    let expr = Expr::Method(Box::new(Expr::Ident("x".to_string())), "abs".to_string(), vec![]);

    let result = evaluate_expr(&expr, &env).unwrap();
    assert_eq!(result.as_num(), 42.5);
}

#[test]
fn test_evaluate_method_unsupported() {
    let env = Environment::new();
    let expr = Expr::Method(Box::new(Expr::Ident("graph".to_string())), "plot".to_string(), vec![]);

    let result = evaluate_expr(&expr, &env);
    assert!(result.is_err());
}

#[test]
fn test_evaluate_index_on_list() {
    let mut env = Environment::new();
    env.set(
        "items",
        Value::List(vec![Value::Num(10.0), Value::Num(20.0), Value::Num(30.0)].into()),
    );
    let expr = Expr::Index(Box::new(Expr::Ident("items".to_string())), Box::new(Expr::Num(1.0)));

    let result = evaluate_expr(&expr, &env).unwrap();
    assert_eq!(result.as_num(), 20.0);
}

#[test]
fn test_evaluate_index_on_vec2() {
    let mut env = Environment::new();
    env.set("pos", Value::Vec2([100.0, 200.0]));
    let expr = Expr::Index(Box::new(Expr::Ident("pos".to_string())), Box::new(Expr::Num(0.0)));

    let result = evaluate_expr(&expr, &env).unwrap();
    assert_eq!(result.as_num(), 100.0);
}

#[test]
fn test_evaluate_index_on_string() {
    let mut env = Environment::new();
    env.set("text", Value::Str("hello".to_string()));
    let expr = Expr::Index(Box::new(Expr::Ident("text".to_string())), Box::new(Expr::Num(1.0)));

    let result = evaluate_expr(&expr, &env).unwrap();
    assert_eq!(result.as_str(), "e");
}

#[test]
fn test_evaluate_index_out_of_bounds() {
    let mut env = Environment::new();
    env.set("items", Value::List(vec![Value::Num(10.0)].into()));
    let expr = Expr::Index(Box::new(Expr::Ident("items".to_string())), Box::new(Expr::Num(5.0)));

    let result = evaluate_expr(&expr, &env);
    assert!(result.is_err());
}

#[test]
fn test_evaluate_construct_creates_object() {
    let env = Environment::new();
    let expr = Expr::Construct(
        "Point".to_string(),
        vec![
            crate::ast::Property {
                name: "x".to_string(),
                value: Expr::Num(10.0),
                value_span: None,
                trailing_comment: None,
            },
            crate::ast::Property {
                name: "y".to_string(),
                value: Expr::Num(20.0),
                value_span: None,
                trailing_comment: None,
            },
        ],
    );

    let result = evaluate_expr(&expr, &env).unwrap();
    match result {
        Value::Object(name, fields) => {
            assert_eq!(name, "Point");
            assert_eq!(fields.get("x").unwrap().as_num(), 10.0);
            assert_eq!(fields.get("y").unwrap().as_num(), 20.0);
        },
        other => panic!("Expected Object, got: {:?}", other),
    }
}

#[test]
fn test_evaluate_closure_captures_variable_at_creation_time() {
    // Use an env with a (empty) base to satisfy the stdlib-base invariant
    // checked by the debug_assert in evaluate_call.
    let mut env = Environment::with_base(std::sync::Arc::new(std::collections::HashMap::new()));
    env.set("y", Value::Num(3.0));
    let closure = Value::Closure(
        vec!["x".to_string()],
        compiled_body(Expr::Binary(
            Box::new(Expr::Ident("x".to_string())),
            BinaryOp::Add,
            Box::new(Expr::Ident("y".to_string())),
        )),
        CapturedEnv(HashMap::from([("y".to_string(), Value::Num(3.0))])),
    );
    env.set("f", closure);
    // y is changed in the environment after closure creation,
    // but the closure captured y=3 at creation time.
    env.set("y", Value::Num(10.0));

    let call_expr = Expr::Call("f".to_string(), vec![Expr::Num(4.0)]);
    let result = evaluate_expr(&call_expr, &env).expect("Evaluation failed");

    // With capture semantics, y=3 (captured at creation time)
    assert_eq!(result, Value::Num(7.0));
}

#[test]
fn test_evaluate_path_uses_flat_dotted_lookup_key() {
    let mut env = Environment::new();
    env.set("node.at.x", Value::Num(320.0));

    let expr = Expr::Path(vec!["node".to_string(), "at".to_string(), "x".to_string()]);
    let result = evaluate_expr(&expr, &env).expect("path lookup should succeed");

    assert_eq!(result, Value::Num(320.0));
}

#[test]
fn test_time_to_ms() {
    assert_eq!(time_to_ms(&Time::Seconds(2.5)), 2500.0);
    assert_eq!(time_to_ms(&Time::Milliseconds(500)), 500.0);
}

#[test]
fn test_parse_color() {
    assert_eq!(parse_color(&Expr::Ident("red".to_string())), [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(parse_color(&Expr::Ident("unknown".to_string())), [0.8, 0.8, 0.8, 1.0]);
    assert_eq!(parse_color(&Expr::Num(1.0)), [0.8, 0.8, 0.8, 1.0]);
}

#[test]
fn test_bool_truthiness_in_conditionals_and_logic() {
    let env = Environment::new();

    let cond_true = Expr::Conditional(
        Box::new(Expr::Bool(true)),
        Box::new(Expr::Num(1.0)),
        Box::new(Expr::Num(0.0)),
    );
    assert_eq!(evaluate_expr(&cond_true, &env).unwrap(), Value::Num(1.0));

    let cond_false = Expr::Conditional(
        Box::new(Expr::Bool(false)),
        Box::new(Expr::Num(1.0)),
        Box::new(Expr::Num(0.0)),
    );
    assert_eq!(evaluate_expr(&cond_false, &env).unwrap(), Value::Num(0.0));

    let not_true = Expr::Unary(crate::ast::UnaryOp::Not, Box::new(Expr::Bool(true)));
    assert_eq!(evaluate_expr(&not_true, &env).unwrap(), Value::Num(0.0));

    let and_expr =
        Expr::Binary(Box::new(Expr::Bool(true)), BinaryOp::And, Box::new(Expr::Bool(false)));
    assert_eq!(evaluate_expr(&and_expr, &env).unwrap(), Value::Num(0.0));

    let or_expr =
        Expr::Binary(Box::new(Expr::Bool(true)), BinaryOp::Or, Box::new(Expr::Bool(false)));
    assert_eq!(evaluate_expr(&or_expr, &env).unwrap(), Value::Num(1.0));
}

#[test]
fn test_evaluate_expr_sin_cos() {
    let mut env = Environment::new();
    load_standard_library(&mut env);
    // sin(0) = 0
    let result = evaluate_expr(&Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]), &env)
        .unwrap_or(Value::Num(0.0));
    assert!({
        let v = result.as_num();
        v.abs() < 1e-10
    });

    // sin(PI/2) ≈ 1
    let result = evaluate_expr(
        &Expr::Call("sin".to_string(), vec![Expr::Num(std::f64::consts::FRAC_PI_2)]),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // cos(0) = 1
    let result = evaluate_expr(&Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]), &env)
        .unwrap_or(Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // cos(PI) ≈ -1
    let result =
        evaluate_expr(&Expr::Call("cos".to_string(), vec![Expr::Num(std::f64::consts::PI)]), &env)
            .unwrap_or(Value::Num(0.0));
    assert!((result.as_num() + 1.0).abs() < 1e-10);

    // sin nested: sin(PI/6) * 2
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Call("sin".to_string(), vec![Expr::Num(std::f64::consts::FRAC_PI_6)])),
            BinaryOp::Mul,
            Box::new(Expr::Num(2.0)),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);
}

#[test]
fn test_evaluate_expr_format() {
    let mut env = Environment::new();
    load_standard_library(&mut env);
    // format("value: {}", 42)
    let result = evaluate_expr(
        &Expr::Call(
            "format".to_string(),
            vec![Expr::Str("value: {}".to_string()), Expr::Num(42.0)],
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_str(), "value: 42");

    // format("x={}, y={}", 10, 20)
    let result = evaluate_expr(
        &Expr::Call(
            "format".to_string(),
            vec![
                Expr::Str("x={}, y={}".to_string()),
                Expr::Num(10.0),
                Expr::Num(20.0),
            ],
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_str(), "x=10, y=20");

    // format with no args
    let result =
        evaluate_expr(&Expr::Call("format".to_string(), vec![]), &env).unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_str(), "");

    // format with text and sin
    let result = evaluate_expr(
        &Expr::Call(
            "format".to_string(),
            vec![
                Expr::Str("sin(π/2) = {}".to_string()),
                Expr::Call("sin".to_string(), vec![Expr::Num(std::f64::consts::FRAC_PI_2)]),
            ],
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_str(), "sin(π/2) = 1");
}

#[test]
fn test_evaluate_expr_path_uses_dotted_environment_lookup() {
    let mut env = Environment::new();
    env.set("left.badge.color", Value::Num(7.0));

    let result = evaluate_expr(
        &Expr::Path(vec!["left".to_string(), "badge".to_string(), "color".to_string()]),
        &env,
    )
    .expect("path lookup should resolve from dotted environment key");

    assert_eq!(result.as_num(), 7.0);
}

#[test]
fn test_evaluate_expr_constants() {
    let mut env = Environment::new();
    load_standard_library(&mut env);
    assert!(
        (evaluate_expr(&Expr::Ident("PI".to_string()), &env)
            .unwrap_or(Value::Num(0.0))
            .as_num()
            - std::f64::consts::PI)
            .abs()
            < 1e-10
    );
    assert!(
        (evaluate_expr(&Expr::Ident("TAU".to_string()), &env)
            .unwrap_or(Value::Num(0.0))
            .as_num()
            - std::f64::consts::TAU)
            .abs()
            < 1e-10
    );
}

#[test]
fn test_evaluate_expr_tuple() {
    let mut env = Environment::new();
    load_standard_library(&mut env);
    let result = evaluate_expr(&Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]), &env)
        .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_vec2(), [100.0, 200.0]);

    // Tuple with call expressions
    let result = evaluate_expr(
        &Expr::Tuple(vec![
            Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]),
            Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]),
        ]),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_vec2(), [0.0, 1.0]);
}

#[test]
fn test_evaluate_expr_modulo() {
    let mut env = Environment::new();
    load_standard_library(&mut env);

    // Basic modulo: 10 % 3 = 1
    let result = evaluate_expr(
        &Expr::Binary(Box::new(Expr::Num(10.0)), BinaryOp::Mod, Box::new(Expr::Num(3.0))),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // Modulo with division: 7 % 2 = 1
    let result = evaluate_expr(
        &Expr::Binary(Box::new(Expr::Num(7.0)), BinaryOp::Mod, Box::new(Expr::Num(2.0))),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // Modulo with sin result: sin(PI/2) % 2 = 1 % 2 = 1
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Call("sin".to_string(), vec![Expr::Num(std::f64::consts::FRAC_PI_2)])),
            BinaryOp::Mod,
            Box::new(Expr::Num(2.0)),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);

    // Nested modulo: (10 % 3) % 2 = 1 % 2 = 1
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Binary(
                Box::new(Expr::Num(10.0)),
                BinaryOp::Mod,
                Box::new(Expr::Num(3.0)),
            )),
            BinaryOp::Mod,
            Box::new(Expr::Num(2.0)),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert!((result.as_num() - 1.0).abs() < 1e-10);
}

#[test]
fn test_evaluate_expr_vec2_operations() {
    let mut env = Environment::new();
    load_standard_library(&mut env);

    // Vec2 addition: (10, 20) + (5, 5) = (15, 25)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            BinaryOp::Add,
            Box::new(Expr::Tuple(vec![Expr::Num(5.0), Expr::Num(5.0)])),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_vec2(), [15.0, 25.0]);

    // Vec2 subtraction: (10, 20) - (3, 8) = (7, 12)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            BinaryOp::Sub,
            Box::new(Expr::Tuple(vec![Expr::Num(3.0), Expr::Num(8.0)])),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_vec2(), [7.0, 12.0]);

    // Vec2 multiplication: (10, 20) * (2, 3) = (20, 60)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            BinaryOp::Mul,
            Box::new(Expr::Tuple(vec![Expr::Num(2.0), Expr::Num(3.0)])),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_vec2(), [20.0, 60.0]);

    // Vec2 division: (10, 20) / (2, 4) = (5, 5)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            BinaryOp::Div,
            Box::new(Expr::Tuple(vec![Expr::Num(2.0), Expr::Num(4.0)])),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_vec2(), [5.0, 5.0]);

    // Vec2 modulo: (10, 21) % (3, 4) = (1, 1)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(21.0)])),
            BinaryOp::Mod,
            Box::new(Expr::Tuple(vec![Expr::Num(3.0), Expr::Num(4.0)])),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_vec2(), [1.0, 1.0]);

    // Scalar-Vec2 multiplication: 2 * (10, 20) = (20, 40)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(2.0)),
            BinaryOp::Mul,
            Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_vec2(), [20.0, 40.0]);

    // Vec2 with sin/cos: (sin(0), cos(0)) = (0, 1)
    let result = evaluate_expr(
        &Expr::Tuple(vec![
            Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]),
            Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]),
        ]),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_vec2(), [0.0, 1.0]);
}

#[test]
fn test_evaluate_expr_vec3_operations() {
    let mut env = Environment::new();
    load_standard_library(&mut env);

    // Vec3 addition: (1, 2, 3) + (4, 5, 6) = (5, 7, 9)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)])),
            BinaryOp::Add,
            Box::new(Expr::Tuple(vec![Expr::Num(4.0), Expr::Num(5.0), Expr::Num(6.0)])),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_vec3(), [5.0, 7.0, 9.0]);

    // Vec3 scalar multiplication: 2 * (1, 2, 3) = (2, 4, 6)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(2.0)),
            BinaryOp::Mul,
            Box::new(Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)])),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    assert_eq!(result.as_vec3(), [2.0, 4.0, 6.0]);
}

#[test]
fn test_evaluate_expr_color_operations() {
    let mut env = Environment::new();
    load_standard_library(&mut env);

    // Color addition: RED + GREEN = (1, 1, 0, 2)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Ident("RED".to_string())),
            BinaryOp::Add,
            Box::new(Expr::Ident("GREEN".to_string())),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    let color = result.as_color();
    assert!((color[0] - 1.0).abs() < 1e-10);
    assert!((color[1] - 1.0).abs() < 1e-10);
    assert!((color[2] - 0.0).abs() < 1e-10);
    assert!((color[3] - 2.0).abs() < 1e-10);

    // Color scalar multiplication: 0.5 * BLUE = (0, 0, 0.5, 0.5)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Num(0.5)),
            BinaryOp::Mul,
            Box::new(Expr::Ident("BLUE".to_string())),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    let color = result.as_color();
    assert!((color[0] - 0.0).abs() < 1e-10);
    assert!((color[1] - 0.0).abs() < 1e-10);
    assert!((color[2] - 0.5).abs() < 1e-10);
    assert!((color[3] - 0.5).abs() < 1e-10);

    // Color subtraction: WHITE - RED = (0, 1, 1, 0) - alpha fades out
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Ident("WHITE".to_string())),
            BinaryOp::Sub,
            Box::new(Expr::Ident("RED".to_string())),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    let color = result.as_color();
    assert!((color[0] - 0.0).abs() < 1e-10);
    assert!((color[1] - 1.0).abs() < 1e-10);
    assert!((color[2] - 1.0).abs() < 1e-10);
}

#[test]
fn test_evaluate_expr_rand() {
    let mut env = Environment::new();
    load_standard_library(&mut env);

    // rand() should return a value between 0 and 1
    let result =
        evaluate_expr(&Expr::Call("rand".to_string(), vec![]), &env).unwrap_or(Value::Num(0.0));
    let val = result.as_num();
    assert!((0.0..1.0).contains(&val), "rand() should return value in [0, 1), got {}", val);

    // rand() with expressions: rand() * 100 should be in [0, 100)
    let result = evaluate_expr(
        &Expr::Binary(
            Box::new(Expr::Call("rand".to_string(), vec![])),
            BinaryOp::Mul,
            Box::new(Expr::Num(100.0)),
        ),
        &env,
    )
    .unwrap_or(Value::Num(0.0));
    let val = result.as_num();
    assert!((0.0..100.0).contains(&val), "rand() * 100 should be in [0, 100), got {}", val);
}

#[test]
fn seeded_rand_is_deterministic() {
    let source = r#"
        c: Ellipse, radius: 50, color: red
        always {
            c.position = (seeded_rand(1.0) * 100, seeded_rand(2.0) * 100)
        }
    "#;
    let ast = animatix_syntax::parser::parse_simple(source).0.unwrap();
    let timeline = super::super::Timeline::build(&ast);
    let track = timeline.world.tracks.get_id("c").unwrap();
    let pos1 = timeline.world.geometry(track).position.get(0, [0.0, 0.0]);

    // Rebuild and re-evaluate — same seed must produce same value
    let timeline2 = super::super::Timeline::build(&ast);
    let track2 = timeline2.world.tracks.get_id("c").unwrap();
    let pos2 = timeline2.world.geometry(track2).position.get(0, [0.0, 0.0]);
    assert_eq!(pos1, pos2, "seeded_rand must be deterministic for the same seed");
}

#[test]
fn seeded_rand_returns_value_in_range() {
    let source = r#"
        c: Ellipse, radius: 50, color: red
        always {
            c.position = (seeded_rand(42.0) * 100, seeded_rand(42.0) * 100)
        }
    "#;
    let ast = animatix_syntax::parser::parse_simple(source).0.unwrap();
    let timeline = super::super::Timeline::build(&ast);
    let track = timeline.world.tracks.get_id("c").unwrap();
    let pos = timeline.world.geometry(track).position.get(0, [0.0, 0.0]);
    assert!(
        pos[0] >= 0.0 && pos[0] <= 100.0,
        "seeded_rand should return value in [0,1] range, got {}",
        pos[0]
    );
    assert!(
        pos[1] >= 0.0 && pos[1] <= 100.0,
        "seeded_rand should return value in [0,1] range, got {}",
        pos[1]
    );
}

#[test]
fn seeded_rand_different_seeds_produce_different_values() {
    let source = r#"
        c: Ellipse, radius: 50, color: red
    "#;
    let ast = animatix_syntax::parser::parse_simple(source).0.unwrap();
    let timeline = super::super::Timeline::build(&ast);

    // Evaluate seeded_rand with different seeds directly in the timeline's env
    let expr1 = Expr::Call("seeded_rand".to_string(), vec![Expr::Num(1.0)]);
    let expr2 = Expr::Call("seeded_rand".to_string(), vec![Expr::Num(2.0)]);

    let val1 = evaluate_expr(&expr1, &timeline.env).unwrap();
    let val2 = evaluate_expr(&expr2, &timeline.env).unwrap();

    let n1 = match val1 {
        Value::Num(n) => n,
        _ => panic!("expected num"),
    };
    let n2 = match val2 {
        Value::Num(n) => n,
        _ => panic!("expected num"),
    };

    assert_ne!(n1, n2, "different seeds should produce different values");
}

#[test]
fn test_object_field_read_via_path_walk() {
    // Directly set up an Object in the env and read a field via path walk
    let mut env = Environment::new();
    let mut fields = std::collections::HashMap::new();
    fields.insert("x".to_string(), Value::Num(10.0));
    fields.insert("y".to_string(), Value::Num(20.0));
    env.set("p", Value::Object("Point".to_string(), fields));

    // Read p.x via path walk (no dotted sub-key in env)
    let expr = Expr::Path(vec!["p".to_string(), "x".to_string()]);
    let result = evaluate_expr(&expr, &env).expect("path walk should succeed");
    assert_eq!(result, Value::Num(10.0));

    // Read p.y
    let expr = Expr::Path(vec!["p".to_string(), "y".to_string()]);
    let result = evaluate_expr(&expr, &env).expect("path walk should succeed");
    assert_eq!(result, Value::Num(20.0));
}

#[test]
fn test_object_field_read_nonexistent_field() {
    let mut env = Environment::new();
    let mut fields = std::collections::HashMap::new();
    fields.insert("x".to_string(), Value::Num(10.0));
    env.set("p", Value::Object("Point".to_string(), fields));

    // Read p.z — field doesn't exist
    let expr = Expr::Path(vec!["p".to_string(), "z".to_string()]);
    let result = evaluate_expr(&expr, &env);
    assert!(result.is_err(), "expected error for nonexistent field");
    if let Err(EvalError::UndefinedVariable(path)) = result {
        assert_eq!(path, "p.z", "error should reference full path");
    } else {
        panic!("expected UndefinedVariable error");
    }
}

#[test]
fn test_object_field_read_non_object_intermediate() {
    let mut env = Environment::new();
    env.set("x", Value::Num(42.0));

    // x.y — x is a number, not an object
    let expr = Expr::Path(vec!["x".to_string(), "y".to_string()]);
    let result = evaluate_expr(&expr, &env);
    assert!(result.is_err(), "expected error for non-object intermediate");
}

#[test]
fn test_path_dotted_backward_compat() {
    // Direct dotted lookup must still work for injected sub-keys
    let mut env = Environment::new();
    env.set("node.at.x", Value::Num(320.0));

    let expr = Expr::Path(vec!["node".to_string(), "at".to_string(), "x".to_string()]);
    let result = evaluate_expr(&expr, &env).expect("path lookup should succeed");
    assert_eq!(result, Value::Num(320.0));
}
