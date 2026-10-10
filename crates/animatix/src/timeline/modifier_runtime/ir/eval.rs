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
            make_vec_value(values)
        },
        CompiledExpr::MakeList(items) => {
            let values = items
                .iter()
                .map(|item| evaluate_compiled_expr(item, env))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Value::List(values.into()))
        },
        CompiledExpr::Unary(op, expr) => {
            if *op == crate::ast::UnaryOp::Ref {
                // lowering compiles Unary(Ref, two-segment path) into
                // CompiledExpr::PropRef; reaching this arm means the operand
                // was not a path.
                return Err(EvalError::TypeMismatch(
                    "& expects a two-segment path: &actor.prop".to_string(),
                ));
            }
            let value = evaluate_compiled_expr(expr, env)?;
            match op {
                UnaryOp::Neg => Ok(Value::Num(-value.as_num())),
                UnaryOp::Not => Ok(Value::Num(if value.is_truthy() { 0.0 } else { 1.0 })),
                UnaryOp::Ref => unreachable!("handled above"),
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
                BuiltinFn::CurveAt => "curve_at",
                BuiltinFn::CurveSmooth => "curve_smooth",
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
        CompiledExpr::PropRef { label, prop } => Ok(Value::PropRef {
            label: label.clone(),
            prop: prop.clone(),
        }),
        CompiledExpr::LetChain(bindings, tail) => {
            // Bindings evaluate in order; each becomes a let-scope entry so
            // later bindings and the tail see it. Scoped lookup lives in
            // `env.get`; pops restore the caller env exactly (the plot
            // sampler shares one env across sample points).
            let mut pushed = 0;
            let mut result = Ok(Value::Num(0.0));
            for (name, value) in bindings {
                match evaluate_compiled_expr(value, env) {
                    Ok(v) => {
                        env.push_let_scope(vec![(name.clone(), v)]);
                        pushed += 1;
                    },
                    Err(e) => {
                        result = Err(e);
                        break;
                    },
                }
            }
            if result.is_ok() {
                result = evaluate_compiled_expr(tail, env);
            }
            for _ in 0..pushed {
                env.pop_let_scope();
            }
            result
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

pub(crate) fn make_vec_value(values: Vec<Value>) -> Result<Value, EvalError> {
    // Elements must be numeric: a non-Num element is a type error, never a
    // silent 0.0 coercion (the tree-walker keeps heterogeneous lists as
    // `Value::List` and errors identically).
    for (i, v) in values.iter().enumerate() {
        if !matches!(v, Value::Num(_)) {
            return Err(EvalError::TypeMismatch(format!(
                "vector/list elements must be numbers, element {i} is {:?}",
                v
            )));
        }
    }
    Ok(match values.len() {
        2 => Value::Vec2([values[0].as_num(), values[1].as_num()]),
        3 => Value::Vec3([values[0].as_num(), values[1].as_num(), values[2].as_num()]),
        4 => Value::Vec4([
            values[0].as_num(),
            values[1].as_num(),
            values[2].as_num(),
            values[3].as_num(),
        ]),
        _ => Value::List(values.into()),
    })
}

/// Direct scalar evaluation of a `CompiledExpr` for 1D math functions f(x) -> f64.
/// Returns None if the expression cannot be evaluated purely as an f64 scalar,
/// signaling that the general evaluate_compiled_expr path should be used instead.
pub(crate) fn evaluate_compiled_expr_scalar(
    expr: &CompiledExpr,
    arg_name: &str,
    x: f64,
    constants: &[(&str, f64)],
) -> Option<f64> {
    match expr {
        CompiledExpr::Const(Value::Num(n)) => Some(*n),
        CompiledExpr::LoadEnv(name) => {
            if name == arg_name {
                Some(x)
            } else {
                constants.iter().find(|(k, _)| *k == name.as_str()).map(|(_, v)| *v)
            }
        },
        CompiledExpr::Unary(op, inner) => {
            let val = evaluate_compiled_expr_scalar(inner, arg_name, x, constants)?;
            match op {
                UnaryOp::Neg => Some(-val),
                UnaryOp::Not => Some(if val != 0.0 { 0.0 } else { 1.0 }),
                UnaryOp::Ref => None,
            }
        },
        CompiledExpr::Binary(left, op, right) => {
            let l = evaluate_compiled_expr_scalar(left, arg_name, x, constants)?;
            let r = evaluate_compiled_expr_scalar(right, arg_name, x, constants)?;
            Some(match op {
                BinaryOp::Add => l + r,
                BinaryOp::Sub => l - r,
                BinaryOp::Mul => l * r,
                BinaryOp::Div => crate::timeline::utils::safe_div(l, r),
                BinaryOp::Mod => crate::timeline::utils::safe_rem(l, r),
                BinaryOp::Pow => l.powf(r),
                BinaryOp::Eq => {
                    if l == r {
                        1.0
                    } else {
                        0.0
                    }
                },
                BinaryOp::Neq => {
                    if l != r {
                        1.0
                    } else {
                        0.0
                    }
                },
                BinaryOp::Lt => {
                    if l < r {
                        1.0
                    } else {
                        0.0
                    }
                },
                BinaryOp::Gt => {
                    if l > r {
                        1.0
                    } else {
                        0.0
                    }
                },
                BinaryOp::Lte => {
                    if l <= r {
                        1.0
                    } else {
                        0.0
                    }
                },
                BinaryOp::Gte => {
                    if l >= r {
                        1.0
                    } else {
                        0.0
                    }
                },
                BinaryOp::And => {
                    if l != 0.0 && r != 0.0 {
                        1.0
                    } else {
                        0.0
                    }
                },
                BinaryOp::Or => {
                    if l != 0.0 || r != 0.0 {
                        1.0
                    } else {
                        0.0
                    }
                },
            })
        },
        CompiledExpr::Select(condition, then_expr, else_expr) => {
            let cond = evaluate_compiled_expr_scalar(condition, arg_name, x, constants)?;
            if cond != 0.0 {
                evaluate_compiled_expr_scalar(then_expr, arg_name, x, constants)
            } else {
                evaluate_compiled_expr_scalar(else_expr, arg_name, x, constants)
            }
        },
        CompiledExpr::CallBuiltin(builtin, args) => match builtin {
            BuiltinFn::Sin => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a.sin())
            },
            BuiltinFn::Cos => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a.cos())
            },
            BuiltinFn::Tan => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a.tan())
            },
            BuiltinFn::Sqrt => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a.sqrt())
            },
            BuiltinFn::Exp => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a.exp())
            },
            BuiltinFn::Log => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a.ln())
            },
            BuiltinFn::Abs => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a.abs())
            },
            BuiltinFn::Floor => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a.floor())
            },
            BuiltinFn::Ceil => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a.ceil())
            },
            BuiltinFn::Round => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a.round())
            },
            BuiltinFn::Signum => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a.signum())
            },
            BuiltinFn::Fract => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a.fract())
            },
            BuiltinFn::Deg => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a * std::f64::consts::PI / 180.0)
            },
            BuiltinFn::Rad => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                Some(a * 180.0 / std::f64::consts::PI)
            },
            BuiltinFn::Min => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                let b = evaluate_compiled_expr_scalar(args.get(1)?, arg_name, x, constants)?;
                Some(a.min(b))
            },
            BuiltinFn::Max => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                let b = evaluate_compiled_expr_scalar(args.get(1)?, arg_name, x, constants)?;
                Some(a.max(b))
            },
            BuiltinFn::Clamp => {
                let v = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                let lo = evaluate_compiled_expr_scalar(args.get(1)?, arg_name, x, constants)?;
                let hi = evaluate_compiled_expr_scalar(args.get(2)?, arg_name, x, constants)?;
                Some(v.clamp(lo, hi))
            },
            BuiltinFn::Lerp => {
                let s = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                let e = evaluate_compiled_expr_scalar(args.get(1)?, arg_name, x, constants)?;
                let t = evaluate_compiled_expr_scalar(args.get(2)?, arg_name, x, constants)?;
                Some(s + (e - s) * t)
            },
            BuiltinFn::Hypot => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                let b = evaluate_compiled_expr_scalar(args.get(1)?, arg_name, x, constants)?;
                Some(a.hypot(b))
            },
            BuiltinFn::Pow => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                let b = evaluate_compiled_expr_scalar(args.get(1)?, arg_name, x, constants)?;
                Some(a.powf(b))
            },
            BuiltinFn::Rem => {
                let a = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                let b = evaluate_compiled_expr_scalar(args.get(1)?, arg_name, x, constants)?;
                Some(a % b)
            },
            BuiltinFn::Step => {
                let edge = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                let x_val = evaluate_compiled_expr_scalar(args.get(1)?, arg_name, x, constants)?;
                Some(if x_val < edge { 0.0 } else { 1.0 })
            },
            BuiltinFn::Atan2 => {
                let y = evaluate_compiled_expr_scalar(args.first()?, arg_name, x, constants)?;
                let x_val = evaluate_compiled_expr_scalar(args.get(1)?, arg_name, x, constants)?;
                Some(y.atan2(x_val))
            },
            _ => None,
        },
        _ => None,
    }
}

/// Direct vector evaluation of a `CompiledExpr` for 2D parametric curves f(t) -> [f64; 2].
pub(crate) fn evaluate_compiled_expr_vec2(
    expr: &CompiledExpr,
    arg_name: &str,
    t: f64,
    constants: &[(&str, f64)],
) -> Option<[f64; 2]> {
    if let CompiledExpr::MakeVec(items) = expr {
        if items.len() == 2 {
            let x = evaluate_compiled_expr_scalar(&items[0], arg_name, t, constants)?;
            let y = evaluate_compiled_expr_scalar(&items[1], arg_name, t, constants)?;
            return Some([x, y]);
        }
    }
    None
}

/// Pre-resolve non-argument identifiers referenced by `expr` against `env` and `captures`.
pub(crate) fn resolve_scalar_constants<'a>(
    expr: &'a CompiledExpr,
    arg_name: &str,
    env: &Environment,
    captures: &CapturedEnv,
    out: &mut Vec<(&'a str, f64)>,
) {
    match expr {
        CompiledExpr::LoadEnv(name)
            if name != arg_name && !out.iter().any(|(k, _)| *k == name.as_str()) =>
        {
            // Priority: env (frame-time overrides and parameters shadow captures)
            if let Some(Value::Num(n)) = env.get_path(name) {
                out.push((name.as_str(), n));
                return;
            }
            // Fallback: captured build-time variables
            if let Some(Value::Num(n)) = captures.0.get(name) {
                out.push((name.as_str(), *n));
            }
        },
        CompiledExpr::Unary(_, inner) => {
            resolve_scalar_constants(inner, arg_name, env, captures, out);
        },
        CompiledExpr::Binary(left, _, right) => {
            resolve_scalar_constants(left, arg_name, env, captures, out);
            resolve_scalar_constants(right, arg_name, env, captures, out);
        },
        CompiledExpr::Select(cond, then_e, else_e) => {
            resolve_scalar_constants(cond, arg_name, env, captures, out);
            resolve_scalar_constants(then_e, arg_name, env, captures, out);
            resolve_scalar_constants(else_e, arg_name, env, captures, out);
        },
        CompiledExpr::CallBuiltin(_, args) => {
            for arg in args {
                resolve_scalar_constants(arg, arg_name, env, captures, out);
            }
        },
        CompiledExpr::MakeVec(items) | CompiledExpr::MakeList(items) => {
            for item in items {
                resolve_scalar_constants(item, arg_name, env, captures, out);
            }
        },
        _ => {},
    }
}

/// Check if `expr` can be evaluated purely via unboxed scalar arithmetic without env mutation.
pub(crate) fn is_scalar_fast_evaluable(
    expr: &CompiledExpr,
    arg_name: &str,
    constants: &[(&str, f64)],
) -> bool {
    match expr {
        CompiledExpr::Const(Value::Num(_)) => true,
        CompiledExpr::LoadEnv(name) => {
            name == arg_name || constants.iter().any(|(k, _)| *k == name.as_str())
        },
        CompiledExpr::Unary(op, inner) => match op {
            UnaryOp::Neg | UnaryOp::Not => is_scalar_fast_evaluable(inner, arg_name, constants),
            UnaryOp::Ref => false,
        },
        CompiledExpr::Binary(left, _op, right) => {
            is_scalar_fast_evaluable(left, arg_name, constants)
                && is_scalar_fast_evaluable(right, arg_name, constants)
        },
        CompiledExpr::Select(cond, then_e, else_e) => {
            is_scalar_fast_evaluable(cond, arg_name, constants)
                && is_scalar_fast_evaluable(then_e, arg_name, constants)
                && is_scalar_fast_evaluable(else_e, arg_name, constants)
        },
        CompiledExpr::CallBuiltin(builtin, args) => match builtin {
            BuiltinFn::Sin
            | BuiltinFn::Cos
            | BuiltinFn::Tan
            | BuiltinFn::Sqrt
            | BuiltinFn::Exp
            | BuiltinFn::Log
            | BuiltinFn::Abs
            | BuiltinFn::Floor
            | BuiltinFn::Ceil
            | BuiltinFn::Round
            | BuiltinFn::Signum
            | BuiltinFn::Fract
            | BuiltinFn::Deg
            | BuiltinFn::Rad => {
                args.len() == 1 && is_scalar_fast_evaluable(&args[0], arg_name, constants)
            },
            BuiltinFn::Min
            | BuiltinFn::Max
            | BuiltinFn::Hypot
            | BuiltinFn::Pow
            | BuiltinFn::Rem
            | BuiltinFn::Step
            | BuiltinFn::Atan2 => {
                args.len() == 2
                    && is_scalar_fast_evaluable(&args[0], arg_name, constants)
                    && is_scalar_fast_evaluable(&args[1], arg_name, constants)
            },
            BuiltinFn::Clamp | BuiltinFn::Lerp => {
                args.len() == 3
                    && is_scalar_fast_evaluable(&args[0], arg_name, constants)
                    && is_scalar_fast_evaluable(&args[1], arg_name, constants)
                    && is_scalar_fast_evaluable(&args[2], arg_name, constants)
            },
            _ => false,
        },
        _ => false,
    }
}

/// Check if `expr` can be evaluated purely via unboxed 2D vector arithmetic without env mutation.
pub(crate) fn is_vec2_fast_evaluable(
    expr: &CompiledExpr,
    arg_name: &str,
    constants: &[(&str, f64)],
) -> bool {
    if let CompiledExpr::MakeVec(items) = expr {
        if items.len() == 2 {
            return is_scalar_fast_evaluable(&items[0], arg_name, constants)
                && is_scalar_fast_evaluable(&items[1], arg_name, constants);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::Environment;

    #[test]
    fn test_scalar_fast_eval_parity() {
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
        let mut constants = Vec::new();
        resolve_scalar_constants(&expr, "x", &env, &captures, &mut constants);

        assert!(is_scalar_fast_evaluable(&expr, "x", &constants));

        for x in [0.0, 0.5, 1.0, 2.5] {
            let fast_val = evaluate_compiled_expr_scalar(&expr, "x", x, &constants).unwrap();

            env.set("x", Value::Num(x));
            let slow_val = evaluate_compiled_expr(&expr, &env).unwrap().as_num();

            assert!((fast_val - slow_val).abs() < 1e-12, "Parity check failed at x={x}");
        }
    }

    #[test]
    fn test_vec2_fast_eval_parametric() {
        let expr = CompiledExpr::MakeVec(vec![
            CompiledExpr::CallBuiltin(BuiltinFn::Cos, vec![CompiledExpr::LoadEnv("t".to_string())]),
            CompiledExpr::CallBuiltin(BuiltinFn::Sin, vec![CompiledExpr::LoadEnv("t".to_string())]),
        ]);

        let constants = Vec::new();
        assert!(is_vec2_fast_evaluable(&expr, "t", &constants));

        let [x, y] = evaluate_compiled_expr_vec2(&expr, "t", 0.5, &constants).unwrap();
        assert!((x - 0.5_f64.cos()).abs() < 1e-12);
        assert!((y - 0.5_f64.sin()).abs() < 1e-12);
    }

    #[test]
    fn test_scalar_fast_eval_rejects_unsupported() {
        let expr = CompiledExpr::Method(
            Box::new(CompiledExpr::LoadEnv("list".to_string())),
            "len".to_string(),
            vec![],
        );
        let constants = Vec::new();
        assert!(!is_scalar_fast_evaluable(&expr, "x", &constants));
    }
}
