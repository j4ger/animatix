use super::types::{BuiltinFn, CompiledExpr};
use crate::ast::{BinaryOp, UnaryOp};
use crate::timeline::callout_geometry::env_anchor_point;
use crate::timeline::env::CapturedEnv;
use crate::timeline::{Environment, EvalError, Value};

/// Evaluate a compiled expression against the given environment.
pub(crate) fn evaluate_compiled_expr(
    expr: &CompiledExpr,
    env: &Environment,
) -> Result<Value, EvalError> {
    match expr {
        CompiledExpr::Const(value) => Ok(value.clone()),
        CompiledExpr::LoadEnv(name) => {
            env.get_path(name).ok_or_else(|| EvalError::UndefinedVariable(name.clone()))
        },
        CompiledExpr::MakeVec(items) => {
            let values = items
                .iter()
                .map(|item| evaluate_compiled_expr(item, env))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(make_vec_value(values))
        },
        CompiledExpr::Unary(op, expr) => {
            let value = evaluate_compiled_expr(expr, env)?;
            match op {
                UnaryOp::Neg => Ok(Value::Num(-value.as_num())),
                UnaryOp::Not => Ok(Value::Num(if value.is_truthy() { 0.0 } else { 1.0 })),
            }
        },
        CompiledExpr::Binary(left, op, right) => {
            let left = evaluate_compiled_expr(left, env)?;
            let right = evaluate_compiled_expr(right, env)?;
            apply_binary_op(left, op, right)
        },
        CompiledExpr::Select(condition, then_expr, else_expr) => {
            let cond = evaluate_compiled_expr(condition, env)?;
            if cond.is_truthy() {
                evaluate_compiled_expr(then_expr, env)
            } else {
                evaluate_compiled_expr(else_expr, env)
            }
        },
        CompiledExpr::CallBuiltin(builtin, args) => {
            let args = args
                .iter()
                .map(|arg| evaluate_compiled_expr(arg, env))
                .collect::<Result<Vec<_>, _>>()?;
            let name = match builtin {
                BuiltinFn::Sin => "sin",
                BuiltinFn::Cos => "cos",
                BuiltinFn::Lerp => "lerp",
                BuiltinFn::Format => "format",
                BuiltinFn::Tan => "tan",
                BuiltinFn::Sqrt => "sqrt",
                BuiltinFn::Exp => "exp",
                BuiltinFn::Log => "ln",
                BuiltinFn::Atan2 => "atan2",
                BuiltinFn::Clamp => "clamp",
                BuiltinFn::Abs => "abs",
                BuiltinFn::Min => "min",
                BuiltinFn::Max => "max",
                BuiltinFn::Floor => "floor",
                BuiltinFn::Ceil => "ceil",
                BuiltinFn::Deg => "deg",
                BuiltinFn::Rad => "rad",
                BuiltinFn::ListSwap => "list_swap",
                BuiltinFn::ListSet => "list_set",
                BuiltinFn::Signum => "signum",
                BuiltinFn::Fract => "fract",
                BuiltinFn::Hypot => "hypot",
                BuiltinFn::Pow => "pow",
                BuiltinFn::Rem => "rem",
                BuiltinFn::Step => "step",
                BuiltinFn::Round => "round",
                BuiltinFn::Factorial => "factorial",
                BuiltinFn::SumList => "sum",
            };
            crate::timeline::eval_shared::eval_builtin_fn(name, &args)
        },
        CompiledExpr::CallEnv(name, args) => {
            let arg_values = args
                .iter()
                .map(|arg| evaluate_compiled_expr(arg, env))
                .collect::<Result<Vec<_>, _>>()?;
            crate::timeline::utils::evaluate_call_value(name, arg_values, env)
        },
        CompiledExpr::Index(container, index) => {
            let container_val = evaluate_compiled_expr(container, env)?;
            let index_val = evaluate_compiled_expr(index, env)?;
            let idx = index_val.as_num() as usize;
            match container_val {
                Value::List(items) => items.get(idx).cloned().ok_or_else(|| {
                    EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for list of length {}",
                        idx,
                        items.len()
                    ))
                }),
                Value::Str(s) => {
                    s.chars().nth(idx).map(|c| Value::Str(c.to_string())).ok_or_else(|| {
                        EvalError::TypeMismatch(format!(
                            "Index {} out of bounds for string of length {}",
                            idx,
                            s.len()
                        ))
                    })
                },
                Value::Vec2(v) => match idx {
                    0 => Ok(Value::Num(v[0])),
                    1 => Ok(Value::Num(v[1])),
                    _ => Err(EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for Vec2",
                        idx
                    ))),
                },
                Value::Vec3(v) => match idx {
                    0 => Ok(Value::Num(v[0])),
                    1 => Ok(Value::Num(v[1])),
                    2 => Ok(Value::Num(v[2])),
                    _ => Err(EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for Vec3",
                        idx
                    ))),
                },
                Value::Vec4(v) => match idx {
                    0 => Ok(Value::Num(v[0])),
                    1 => Ok(Value::Num(v[1])),
                    2 => Ok(Value::Num(v[2])),
                    3 => Ok(Value::Num(v[3])),
                    _ => Err(EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for Vec4",
                        idx
                    ))),
                },
                Value::Color(c) => match idx {
                    0 => Ok(Value::Num(c[0])),
                    1 => Ok(Value::Num(c[1])),
                    2 => Ok(Value::Num(c[2])),
                    3 => Ok(Value::Num(c[3])),
                    _ => Err(EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for Color",
                        idx
                    ))),
                },
                other => Err(EvalError::TypeMismatch(format!("Cannot index into {:?}", other))),
            }
        },
        CompiledExpr::Method(receiver, name, args) => {
            let receiver_val = evaluate_compiled_expr(receiver, env)?;
            let arg_values: Vec<Value> = args
                .iter()
                .map(|arg| evaluate_compiled_expr(arg, env))
                .collect::<Result<Vec<_>, _>>()?;
            eval_method(receiver_val, name, &arg_values, env)
        },
        CompiledExpr::Closure(params, body) => {
            Ok(Value::Closure(params.clone(), body.clone(), CapturedEnv::snapshot(env)))
        },
        CompiledExpr::Construct(name, fields) => {
            let mut map = std::collections::HashMap::new();
            for (field_name, field_expr) in fields {
                let val = evaluate_compiled_expr(field_expr, env)?;
                map.insert(field_name.clone(), val);
            }
            Ok(Value::Object(name.clone(), map))
        },
        CompiledExpr::AnchorLookup { actor, anchor } => env_anchor_point(env, actor, *anchor)
            .map(Value::Vec2)
            .ok_or_else(|| EvalError::UndefinedVariable(format!("{actor}.{}", anchor.as_str()))),
    }
}

/// Evaluate a method call on a receiver value.
pub(crate) fn eval_method(
    receiver: Value,
    name: &str,
    args: &[Value],
    env: &Environment,
) -> Result<Value, EvalError> {
    crate::timeline::utils::eval_method_dispatch(receiver, name, args, env)
}

pub(crate) fn apply_binary_op(
    left: Value,
    op: &BinaryOp,
    right: Value,
) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_binary_op(left, op, right)
}

pub(crate) fn make_vec_value(values: Vec<Value>) -> Value {
    match values.len() {
        2 => Value::Vec2([values[0].as_num(), values[1].as_num()]),
        3 => Value::Vec3([values[0].as_num(), values[1].as_num(), values[2].as_num()]),
        4 => Value::Vec4([
            values[0].as_num(),
            values[1].as_num(),
            values[2].as_num(),
            values[3].as_num(),
        ]),
        _ => Value::List(values.into()),
    }
}
