use crate::ast::{BinaryOp, LoopPattern, UnaryOp};
use crate::timeline::env::CapturedEnv;
use crate::timeline::{Environment, EvalError, Value};
use super::types::{
    BuiltinFn, CompiledExpr, ModifierExpr, ModifierIrProgram, ModifierIrStmt, ModifierOverrides,
};

/// Evaluate a modifier expression, dispatching between compiled and unsupported variants.
pub fn evaluate_modifier_expr(
    expr: &ModifierExpr,
    env: &Environment,
) -> Result<Value, EvalError> {
    match expr {
        ModifierExpr::Compiled(expr) => evaluate_compiled_expr(expr, env),
        ModifierExpr::Unsupported(expr) => crate::timeline::evaluate_expr(expr, env),
    }
}

/// Execute a modifier IR program statement by statement.
pub fn execute_modifier_ir(
    program: &ModifierIrProgram,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
) -> Result<(), EvalError> {
    for stmt in &program.statements {
        execute_modifier_stmt(stmt, frame_env, overrides)?;
    }
    Ok(())
}

fn execute_modifier_stmt(
    stmt: &ModifierIrStmt,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
) -> Result<(), EvalError> {
    match stmt {
        ModifierIrStmt::Assign {
            target,
            property,
            value,
        } => {
            let val = evaluate_modifier_expr(value, frame_env)?;
            let label = target.join(".");
            overrides
                .entry(label.clone())
                .or_default()
                .insert(property.clone(), val.clone());
            crate::timeline::frame_env::apply_override_incremental(
                frame_env, &label, property, val,
            );
            Ok(())
        }
        ModifierIrStmt::Let { name, value } => {
            let val = evaluate_modifier_expr(value, frame_env)?;
            frame_env.set(name, val);
            Ok(())
        }
        ModifierIrStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond = evaluate_modifier_expr(condition, frame_env)?;
            let branch = if cond.as_num() != 0.0 {
                then_branch
            } else {
                else_branch
            };
            for stmt in branch {
                execute_modifier_stmt(stmt, frame_env, overrides)?;
            }
            Ok(())
        }
        ModifierIrStmt::For {
            var,
            index_var,
            iterable,
            body,
        } => {
            let values = evaluate_compiled_expr(iterable, frame_env)?;
            let items: Vec<Value> = match values {
                Value::List(list) => list,
                Value::Vec2(v) => v.into_iter().map(Value::Num).collect(),
                Value::Vec3(v) => v.into_iter().map(Value::Num).collect(),
                Value::Vec4(v) => v.into_iter().map(Value::Num).collect(),
                other => vec![other],
            };
            for (idx, item) in items.into_iter().enumerate() {
                bind_loop_var_ir(frame_env, var, item);
                if let Some(iv) = index_var {
                    frame_env.set(iv, Value::Num(idx as f64));
                }
                for stmt in body {
                    execute_modifier_stmt(stmt, frame_env, overrides)?;
                }
            }
            // Clean up loop variables after the loop exits
            match var {
                LoopPattern::Single(name) => {
                    frame_env.overrides.remove(name);
                }
                LoopPattern::Tuple(names) => {
                    for name in names {
                        frame_env.overrides.remove(name);
                    }
                }
            }
            if let Some(iv) = index_var {
                frame_env.overrides.remove(iv);
            }
            Ok(())
        }
    }
}

/// Evaluate a compiled expression against the given environment.
pub fn evaluate_compiled_expr(
    expr: &CompiledExpr,
    env: &Environment,
) -> Result<Value, EvalError> {
    match expr {
        CompiledExpr::Const(value) => Ok(value.clone()),
        CompiledExpr::LoadEnv(name) => env
            .get_ref(name)
            .cloned()
            .ok_or_else(|| EvalError::UndefinedVariable(name.clone())),
        CompiledExpr::MakeVec(items) => {
            let values = items
                .iter()
                .map(|item| evaluate_compiled_expr(item, env))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(make_vec_value(values))
        }
        CompiledExpr::Unary(op, expr) => {
            let value = evaluate_compiled_expr(expr, env)?;
            match op {
                UnaryOp::Neg => Ok(Value::Num(-value.as_num())),
                UnaryOp::Not => Ok(Value::Num(if value.as_num() == 0.0 { 1.0 } else { 0.0 })),
            }
        }
        CompiledExpr::Binary(left, op, right) => {
            let left = evaluate_compiled_expr(left, env)?;
            let right = evaluate_compiled_expr(right, env)?;
            apply_binary_op(left, op, right)
        }
        CompiledExpr::Select(condition, then_expr, else_expr) => {
            let cond = evaluate_compiled_expr(condition, env)?;
            if cond.as_num() != 0.0 {
                evaluate_compiled_expr(then_expr, env)
            } else {
                evaluate_compiled_expr(else_expr, env)
            }
        }
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
            };
            crate::timeline::eval_shared::eval_builtin_fn(name, &args)
        }
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
                Value::Str(s) => s
                    .chars()
                    .nth(idx)
                    .map(|c| Value::Str(c.to_string()))
                    .ok_or_else(|| {
                        EvalError::TypeMismatch(format!(
                            "Index {} out of bounds for string of length {}",
                            idx,
                            s.len()
                        ))
                    }),
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
                other => Err(EvalError::TypeMismatch(format!(
                    "Cannot index into {:?}",
                    other
                ))),
            }
        }
        CompiledExpr::Method(receiver, name, args) => {
            let receiver_val = evaluate_compiled_expr(receiver, env)?;
            let arg_values: Vec<Value> = args
                .iter()
                .map(|arg| evaluate_compiled_expr(arg, env))
                .collect::<Result<Vec<_>, _>>()?;
            eval_method(receiver_val, name, &arg_values, env)
        }
        CompiledExpr::Closure(params, body) => {
            Ok(Value::Closure(params.clone(), body.clone(), CapturedEnv::snapshot(env)))
        }
        CompiledExpr::Construct(name, fields) => {
            let mut map = std::collections::HashMap::new();
            for (field_name, field_expr) in fields {
                let val = evaluate_compiled_expr(field_expr, env)?;
                map.insert(field_name.clone(), val);
            }
            Ok(Value::Object(name.clone(), map))
        }
    }
}

// All builtin eval functions (eval_sin, eval_cos, eval_lerp, eval_format, ...)
// have been consolidated into crate::timeline::eval_shared::eval_builtin_fn.
// The individual functions are retained as thin wrappers for backward compatibility
// but now delegate to the shared implementation.

pub(crate) fn eval_sin(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("sin", args)
}

pub(crate) fn eval_cos(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("cos", args)
}

pub(crate) fn eval_lerp(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("lerp", args)
}

pub(crate) fn eval_format(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("format", args)
}

pub(crate) fn eval_tan(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("tan", args)
}

pub(crate) fn eval_sqrt(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("sqrt", args)
}

pub(crate) fn eval_exp(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("exp", args)
}

pub(crate) fn eval_log(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("ln", args)
}

pub(crate) fn eval_atan2(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("atan2", args)
}

pub(crate) fn eval_clamp(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("clamp", args)
}

pub(crate) fn eval_abs(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("abs", args)
}

pub(crate) fn eval_min(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("min", args)
}

pub(crate) fn eval_max(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("max", args)
}

pub(crate) fn eval_floor(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("floor", args)
}

pub(crate) fn eval_ceil(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("ceil", args)
}

pub(crate) fn eval_deg(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("deg", args)
}

pub(crate) fn eval_rad(args: &[Value]) -> Result<Value, EvalError> {
    crate::timeline::eval_shared::eval_builtin_fn("rad", args)
}

/// Evaluate a method call on a receiver value (modifier IR version).
pub(crate) fn eval_method(
    receiver: Value,
    name: &str,
    args: &[Value],
    env: &Environment,
) -> Result<Value, EvalError> {
    // Delegate to shared dispatch logic
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
        _ => Value::List(values),
    }
}

/// Bind loop variables according to the pattern in the IR evaluator.
fn bind_loop_var_ir(frame_env: &mut Environment, var: &LoopPattern, value: Value) {
    match var {
        LoopPattern::Single(name) => {
            frame_env.set(name, value);
        }
        LoopPattern::Tuple(names) => {
            let components: Vec<Value> = match &value {
                Value::List(items) => items.clone(),
                Value::Vec2(v) => v.iter().map(|&x| Value::Num(x)).collect(),
                Value::Vec3(v) => v.iter().map(|&x| Value::Num(x)).collect(),
                Value::Vec4(v) => v.iter().map(|&x| Value::Num(x)).collect(),
                Value::Color(v) => v.iter().map(|&x| Value::Num(x)).collect(),
                other => vec![other.clone()],
            };
            for (i, name) in names.iter().enumerate().take(components.len().min(names.len())) {
                if i < components.len() {
                    frame_env.set(name, components[i].clone());
                }
            }
        }
    }
}