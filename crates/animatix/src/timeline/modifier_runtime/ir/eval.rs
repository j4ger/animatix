use crate::ast::{BinaryOp, UnaryOp};
use crate::timeline::{Environment, EvalError, Value};
use crate::timeline::utils::{safe_div, safe_rem};

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
            for item in items {
                frame_env.set(var, item);
                for stmt in body {
                    execute_modifier_stmt(stmt, frame_env, overrides)?;
                }
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
            match builtin {
                BuiltinFn::Sin => eval_sin(&args),
                BuiltinFn::Cos => eval_cos(&args),
                BuiltinFn::Lerp => eval_lerp(&args),
                BuiltinFn::Format => eval_format(&args),
                BuiltinFn::Tan => eval_tan(&args),
                BuiltinFn::Sqrt => eval_sqrt(&args),
                BuiltinFn::Exp => eval_exp(&args),
                BuiltinFn::Log => eval_log(&args),
                BuiltinFn::Atan2 => eval_atan2(&args),
                BuiltinFn::Clamp => eval_clamp(&args),
                BuiltinFn::Abs => eval_abs(&args),
                BuiltinFn::Min => eval_min(&args),
                BuiltinFn::Max => eval_max(&args),
                BuiltinFn::Floor => eval_floor(&args),
                BuiltinFn::Ceil => eval_ceil(&args),
            }
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
            eval_method(receiver_val, name, &arg_values)
        }
    }
}

pub(crate) fn eval_sin(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(
            "sin expects 1 argument".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().sin()))
}

pub(crate) fn eval_cos(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(
            "cos expects 1 argument".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().cos()))
}

pub(crate) fn eval_lerp(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::TypeMismatch(
            "lerp expects 3 arguments".to_string(),
        ));
    }
    Ok(Value::Num(
        args[0].as_num() + (args[1].as_num() - args[0].as_num()) * args[2].as_num(),
    ))
}

pub(crate) fn eval_format(args: &[Value]) -> Result<Value, EvalError> {
    let Some((template, rest)) = args.split_first() else {
        return Ok(Value::Str(String::new()));
    };
    let mut output = template.as_str();
    for arg in rest {
        let replacement = match arg {
            Value::Num(n) => {
                if n.fract() == 0.0 {
                    format!("{}", *n as i64)
                } else {
                    n.to_string()
                }
            }
            Value::Str(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Vec2(v) => format!("({}, {})", v[0], v[1]),
            Value::Vec3(v) => format!("({}, {}, {})", v[0], v[1], v[2]),
            Value::Vec4(v) | Value::Color(v) => {
                format!("({}, {}, {}, {})", v[0], v[1], v[2], v[3])
            }
            Value::List(items) => format!("{:?}", items),
            Value::Object(name, fields) => format!("{}({:?})", name, fields),
            Value::NativeFn(_) => "<NativeFn>".to_string(),
            Value::Closure(_, _) => "<Closure>".to_string(),
        };
        output = output.replacen("{}", &replacement, 1);
    }
    Ok(Value::Str(output))
}

pub(crate) fn eval_tan(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(
            "tan expects 1 argument".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().tan()))
}

pub(crate) fn eval_sqrt(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(
            "sqrt expects 1 argument".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().sqrt()))
}

pub(crate) fn eval_exp(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(
            "exp expects 1 argument".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().exp()))
}

pub(crate) fn eval_log(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(
            "log expects 1 argument".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().ln()))
}

pub(crate) fn eval_atan2(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeMismatch(
            "atan2 expects 2 arguments".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().atan2(args[1].as_num())))
}

pub(crate) fn eval_clamp(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::TypeMismatch(
            "clamp expects 3 arguments".to_string(),
        ));
    }
    Ok(Value::Num(
        args[0].as_num().clamp(args[1].as_num(), args[2].as_num()),
    ))
}

pub(crate) fn eval_abs(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(
            "abs expects 1 argument".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().abs()))
}

pub(crate) fn eval_min(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeMismatch(
            "min expects 2 arguments".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().min(args[1].as_num())))
}

pub(crate) fn eval_max(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeMismatch(
            "max expects 2 arguments".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().max(args[1].as_num())))
}

pub(crate) fn eval_floor(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(
            "floor expects 1 argument".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().floor()))
}

pub(crate) fn eval_ceil(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(
            "ceil expects 1 argument".to_string(),
        ));
    }
    Ok(Value::Num(args[0].as_num().ceil()))
}

/// Evaluate a method call on a receiver value (modifier IR version).
pub(crate) fn eval_method(
    receiver: Value,
    name: &str,
    args: &[Value],
) -> Result<Value, EvalError> {
    match (receiver, name) {
        (Value::Str(s), "length") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "String.length() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(s.len() as f64))
        }
        (Value::Str(s), "split") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "String.split(delim) takes exactly 1 argument".to_string(),
                ));
            }
            let delim = args[0].as_str();
            let parts: Vec<Value> = s
                .split(&delim)
                .map(|part| Value::Str(part.to_string()))
                .collect();
            Ok(Value::List(parts))
        }
        (Value::Str(s), "contains") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "String.contains(substr) takes exactly 1 argument".to_string(),
                ));
            }
            let substr = args[0].as_str();
            Ok(Value::Num(if s.contains(&substr) { 1.0 } else { 0.0 }))
        }
        (Value::Str(s), "trim") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "String.trim() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Str(s.trim().to_string()))
        }
        (Value::Str(s), "starts_with") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "String.starts_with(prefix) takes exactly 1 argument".to_string(),
                ));
            }
            let prefix = args[0].as_str();
            Ok(Value::Num(if s.starts_with(&prefix) { 1.0 } else { 0.0 }))
        }
        (Value::Str(s), "ends_with") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "String.ends_with(suffix) takes exactly 1 argument".to_string(),
                ));
            }
            let suffix = args[0].as_str();
            Ok(Value::Num(if s.ends_with(&suffix) { 1.0 } else { 0.0 }))
        }
        (Value::List(items), "length") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "List.length() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(items.len() as f64))
        }
        (Value::List(items), "get") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "List.get(index) takes exactly 1 argument".to_string(),
                ));
            }
            let idx = args[0].as_num() as usize;
            items.get(idx).cloned().ok_or_else(|| {
                EvalError::TypeMismatch(format!(
                    "Index {} out of bounds for list of length {}",
                    idx,
                    items.len()
                ))
            })
        }
        (Value::List(items), "contains") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "List.contains(item) takes exactly 1 argument".to_string(),
                ));
            }
            let item = args[0].clone();
            Ok(Value::Num(if items.contains(&item) { 1.0 } else { 0.0 }))
        }
        (Value::Num(n), "abs") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "Num.abs() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(n.abs()))
        }
        (Value::Num(n), "floor") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "Num.floor() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(n.floor()))
        }
        (Value::Num(n), "ceil") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "Num.ceil() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(n.ceil()))
        }
        (Value::Num(n), "round") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "Num.round() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(n.round()))
        }
        (receiver, name) => Err(EvalError::UnsupportedMethod(format!(
            "{}.{}()",
            format_value(&receiver),
            name
        ))),
    }
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Num(n) => {
            if *n == n.floor() {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::Str(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Vec2(t) => format!("({}, {})", t[0], t[1]),
        Value::Vec3(t) => format!("({}, {}, {})", t[0], t[1], t[2]),
        Value::Vec4(t) => format!("({}, {}, {}, {})", t[0], t[1], t[2], t[3]),
        Value::Color(c) => format!("rgba({}, {}, {}, {})", c[0], c[1], c[2], c[3]),
        Value::List(items) => format!("{:?}", items),
        Value::Object(name, fields) => format!("{}({:?})", name, fields),
        Value::NativeFn(_) => "<NativeFn>".to_string(),
        Value::Closure(args, _) => format!("<Closure({:?})>", args),
    }
}

pub(crate) fn apply_binary_op(
    left: Value,
    op: &BinaryOp,
    right: Value,
) -> Result<Value, EvalError> {
    match (left.clone(), right.clone()) {
        (Value::Num(l), Value::Num(r)) => Ok(Value::Num(match op {
            BinaryOp::Add => l + r,
            BinaryOp::Sub => l - r,
            BinaryOp::Mul => l * r,
            BinaryOp::Div => safe_div(l, r),
            BinaryOp::Mod => safe_rem(l, r),
            BinaryOp::Pow => l.powf(r),
            BinaryOp::Eq => {
                if l == r {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::Neq => {
                if l != r {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::Lt => {
                if l < r {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::Gt => {
                if l > r {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::Lte => {
                if l <= r {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::Gte => {
                if l >= r {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::And => {
                if l != 0.0 && r != 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
            BinaryOp::Or => {
                if l != 0.0 || r != 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
        })),
        (Value::Vec2(l), Value::Vec2(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec2([l[0] + r[0], l[1] + r[1]])),
            BinaryOp::Sub => Ok(Value::Vec2([l[0] - r[0], l[1] - r[1]])),
            BinaryOp::Mul => Ok(Value::Vec2([l[0] * r[0], l[1] * r[1]])),
            BinaryOp::Div => Ok(Value::Vec2([
                safe_div(l[0], r[0]),
                safe_div(l[1], r[1]),
            ])),
            BinaryOp::Mod => Ok(Value::Vec2([
                safe_rem(l[0], r[0]),
                safe_rem(l[1], r[1]),
            ])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Vec2 and Vec2",
                op
            ))),
        },
        (Value::Vec3(l), Value::Vec3(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec3([l[0] + r[0], l[1] + r[1], l[2] + r[2]])),
            BinaryOp::Sub => Ok(Value::Vec3([l[0] - r[0], l[1] - r[1], l[2] - r[2]])),
            BinaryOp::Mul => Ok(Value::Vec3([l[0] * r[0], l[1] * r[1], l[2] * r[2]])),
            BinaryOp::Div => Ok(Value::Vec3([
                safe_div(l[0], r[0]),
                safe_div(l[1], r[1]),
                safe_div(l[2], r[2]),
            ])),
            BinaryOp::Mod => Ok(Value::Vec3([
                safe_rem(l[0], r[0]),
                safe_rem(l[1], r[1]),
                safe_rem(l[2], r[2]),
            ])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Vec3 and Vec3",
                op
            ))),
        },
        (Value::Color(l), Value::Color(r)) => match op {
            BinaryOp::Add => Ok(Value::Color([
                l[0] + r[0],
                l[1] + r[1],
                l[2] + r[2],
                l[3] + r[3],
            ])),
            BinaryOp::Sub => Ok(Value::Color([
                l[0] - r[0],
                l[1] - r[1],
                l[2] - r[2],
                l[3] - r[3],
            ])),
            BinaryOp::Mul => Ok(Value::Color([
                l[0] * r[0],
                l[1] * r[1],
                l[2] * r[2],
                l[3] * r[3],
            ])),
            BinaryOp::Div => Ok(Value::Color([
                safe_div(l[0], r[0]),
                safe_div(l[1], r[1]),
                safe_div(l[2], r[2]),
                safe_div(l[3], r[3]),
            ])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Color and Color",
                op
            ))),
        },
        (Value::Vec2(l), Value::Num(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec2([l[0] + r, l[1] + r])),
            BinaryOp::Sub => Ok(Value::Vec2([l[0] - r, l[1] - r])),
            BinaryOp::Mul => Ok(Value::Vec2([l[0] * r, l[1] * r])),
            BinaryOp::Div => Ok(Value::Vec2([
                safe_div(l[0], r),
                safe_div(l[1], r),
            ])),
            BinaryOp::Mod => Ok(Value::Vec2([
                safe_rem(l[0], r),
                safe_rem(l[1], r),
            ])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Vec2 and Num",
                op
            ))),
        },
        (Value::Num(l), Value::Vec2(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec2([l + r[0], l + r[1]])),
            BinaryOp::Sub => Ok(Value::Vec2([l - r[0], l - r[1]])),
            BinaryOp::Mul => Ok(Value::Vec2([l * r[0], l * r[1]])),
            BinaryOp::Div => Ok(Value::Vec2([
                safe_div(l, r[0]),
                safe_div(l, r[1]),
            ])),
            BinaryOp::Mod => Ok(Value::Vec2([
                safe_rem(l, r[0]),
                safe_rem(l, r[1]),
            ])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Num and Vec2",
                op
            ))),
        },
        (Value::Vec3(l), Value::Num(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec3([l[0] + r, l[1] + r, l[2] + r])),
            BinaryOp::Sub => Ok(Value::Vec3([l[0] - r, l[1] - r, l[2] - r])),
            BinaryOp::Mul => Ok(Value::Vec3([l[0] * r, l[1] * r, l[2] * r])),
            BinaryOp::Div => Ok(Value::Vec3([
                safe_div(l[0], r),
                safe_div(l[1], r),
                safe_div(l[2], r),
            ])),
            BinaryOp::Mod => Ok(Value::Vec3([
                safe_rem(l[0], r),
                safe_rem(l[1], r),
                safe_rem(l[2], r),
            ])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Vec3 and Num",
                op
            ))),
        },
        (Value::Num(l), Value::Vec3(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec3([l + r[0], l + r[1], l + r[2]])),
            BinaryOp::Sub => Ok(Value::Vec3([l - r[0], l - r[1], l - r[2]])),
            BinaryOp::Mul => Ok(Value::Vec3([l * r[0], l * r[1], l * r[2]])),
            BinaryOp::Div => Ok(Value::Vec3([
                safe_div(l, r[0]),
                safe_div(l, r[1]),
                safe_div(l, r[2]),
            ])),
            BinaryOp::Mod => Ok(Value::Vec3([
                safe_rem(l, r[0]),
                safe_rem(l, r[1]),
                safe_rem(l, r[2]),
            ])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Num and Vec3",
                op
            ))),
        },
        (Value::Color(l), Value::Num(r)) => match op {
            BinaryOp::Add => Ok(Value::Color([l[0] + r, l[1] + r, l[2] + r, l[3] + r])),
            BinaryOp::Sub => Ok(Value::Color([l[0] - r, l[1] - r, l[2] - r, l[3] - r])),
            BinaryOp::Mul => Ok(Value::Color([l[0] * r, l[1] * r, l[2] * r, l[3] * r])),
            BinaryOp::Div => Ok(Value::Color([
                safe_div(l[0], r),
                safe_div(l[1], r),
                safe_div(l[2], r),
                safe_div(l[3], r),
            ])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Color and Num",
                op
            ))),
        },
        (Value::Num(l), Value::Color(r)) => match op {
            BinaryOp::Add => Ok(Value::Color([l + r[0], l + r[1], l + r[2], l + r[3]])),
            BinaryOp::Sub => Ok(Value::Color([l - r[0], l - r[1], l - r[2], l - r[3]])),
            BinaryOp::Mul => Ok(Value::Color([l * r[0], l * r[1], l * r[2], l * r[3]])),
            BinaryOp::Div => Ok(Value::Color([
                safe_div(l, r[0]),
                safe_div(l, r[1]),
                safe_div(l, r[2]),
                safe_div(l, r[3]),
            ])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Num and Color",
                op
            ))),
        },
        _ => Err(EvalError::TypeMismatch(format!(
            "Unsupported operation {:?} for {:?} and {:?}",
            op, left, right
        ))),
    }
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