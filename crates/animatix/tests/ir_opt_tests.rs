//! Tests for IR optimization passes (constant folding, dead branch elimination, CSE).

use animatix::timeline::modifier_runtime::ir::{
    CompiledExpr, ModifierIrProgram, ModifierIrStmt, ModifierOverrides, execute_modifier_ir,
};
use animatix::timeline::{Environment, Value};
use animatix_syntax::ast::{BinaryOp, UnaryOp};

fn eval_expr_in_empty_env(expr: CompiledExpr) -> Value {
    let program = ModifierIrProgram {
        statements: vec![ModifierIrStmt::Let {
            name: "__res".to_string(),
            value: expr,
        }],
    };
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
    let program = ModifierIrProgram {
        statements: vec![ModifierIrStmt::If {
            condition: CompiledExpr::Const(Value::Num(1.0)),
            then_branch: vec![ModifierIrStmt::Let {
                name: "branch".to_string(),
                value: CompiledExpr::Const(Value::Str("then".to_string())),
            }],
            else_branch: vec![ModifierIrStmt::Let {
                name: "branch".to_string(),
                value: CompiledExpr::Const(Value::Str("else".to_string())),
            }],
        }],
    };
    let mut env = Environment::new();
    let mut overrides = ModifierOverrides::default();
    execute_modifier_ir(&program, &mut env, &mut overrides).expect("execution succeeds");
    assert_eq!(env.get("branch"), Some(Value::Str("then".to_string())));
}
