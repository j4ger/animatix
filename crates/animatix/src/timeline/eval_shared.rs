//! Shared evaluation helpers for binary operations and builtin functions.
//!
//! These are pure functions that can be called from both the tree-walker
//! (`utils.rs`) and the IR/VM (`ir/eval.rs`) evaluation paths, eliminating
//! code duplication between the two execution engines.
//!
//! Each function operates on already-evaluated [`Value`] arguments and returns
//! a [`Result<Value, EvalError>`] — no environment lookups or expression
//! recursion happen here.

use crate::ast::BinaryOp;
use crate::timeline::utils::{safe_div, safe_rem};
use crate::timeline::{EvalError, Value};

/// Evaluate a binary operation on two runtime values.
///
/// Supports:
/// - Num-Num ops (arithmetic, comparison, logical)
/// - Vec-Vec elementwise ops (Add, Sub, Mul, Div, Mod)
/// - Vec-Num / Num-Vec broadcasting
/// - Color-Color and Color-Num / Num-Color ops
/// - Arbitrary-type Eq/Neq (structural equality)
///
/// Returns [`EvalError::TypeMismatch`] for unsupported type/op combinations.
pub fn eval_binary_op(left: Value, op: &BinaryOp, right: Value) -> Result<Value, EvalError> {
    // `and`/`or` are truthiness-based and accept both `Num` and `Bool` operands,
    // so handle them before the type-specific arithmetic arms below.
    match op {
        BinaryOp::And => {
            return Ok(Value::Num(if left.is_truthy() && right.is_truthy() {
                1.0
            } else {
                0.0
            }));
        },
        BinaryOp::Or => {
            return Ok(Value::Num(if left.is_truthy() || right.is_truthy() {
                1.0
            } else {
                0.0
            }));
        },
        _ => {},
    }

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
        })),
        (Value::Vec2(l), Value::Vec2(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec2([l[0] + r[0], l[1] + r[1]])),
            BinaryOp::Sub => Ok(Value::Vec2([l[0] - r[0], l[1] - r[1]])),
            BinaryOp::Mul => Ok(Value::Vec2([l[0] * r[0], l[1] * r[1]])),
            BinaryOp::Div => Ok(Value::Vec2([safe_div(l[0], r[0]), safe_div(l[1], r[1])])),
            BinaryOp::Mod => Ok(Value::Vec2([safe_rem(l[0], r[0]), safe_rem(l[1], r[1])])),
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
            BinaryOp::Add => Ok(Value::Color([l[0] + r[0], l[1] + r[1], l[2] + r[2], l[3] + r[3]])),
            BinaryOp::Sub => Ok(Value::Color([l[0] - r[0], l[1] - r[1], l[2] - r[2], l[3] - r[3]])),
            BinaryOp::Mul => Ok(Value::Color([l[0] * r[0], l[1] * r[1], l[2] * r[2], l[3] * r[3]])),
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
            BinaryOp::Div => Ok(Value::Vec2([safe_div(l[0], r), safe_div(l[1], r)])),
            BinaryOp::Mod => Ok(Value::Vec2([safe_rem(l[0], r), safe_rem(l[1], r)])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Vec2 and Num",
                op
            ))),
        },
        (Value::Num(l), Value::Vec2(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec2([l + r[0], l + r[1]])),
            BinaryOp::Sub => Ok(Value::Vec2([l - r[0], l - r[1]])),
            BinaryOp::Mul => Ok(Value::Vec2([l * r[0], l * r[1]])),
            BinaryOp::Div => Ok(Value::Vec2([safe_div(l, r[0]), safe_div(l, r[1])])),
            BinaryOp::Mod => Ok(Value::Vec2([safe_rem(l, r[0]), safe_rem(l, r[1])])),
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Num and Vec2",
                op
            ))),
        },
        (Value::Vec3(l), Value::Num(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec3([l[0] + r, l[1] + r, l[2] + r])),
            BinaryOp::Sub => Ok(Value::Vec3([l[0] - r, l[1] - r, l[2] - r])),
            BinaryOp::Mul => Ok(Value::Vec3([l[0] * r, l[1] * r, l[2] * r])),
            BinaryOp::Div => {
                Ok(Value::Vec3([safe_div(l[0], r), safe_div(l[1], r), safe_div(l[2], r)]))
            },
            BinaryOp::Mod => {
                Ok(Value::Vec3([safe_rem(l[0], r), safe_rem(l[1], r), safe_rem(l[2], r)]))
            },
            _ => Err(EvalError::TypeMismatch(format!(
                "Unsupported operation {:?} for Vec3 and Num",
                op
            ))),
        },
        (Value::Num(l), Value::Vec3(r)) => match op {
            BinaryOp::Add => Ok(Value::Vec3([l + r[0], l + r[1], l + r[2]])),
            BinaryOp::Sub => Ok(Value::Vec3([l - r[0], l - r[1], l - r[2]])),
            BinaryOp::Mul => Ok(Value::Vec3([l * r[0], l * r[1], l * r[2]])),
            BinaryOp::Div => {
                Ok(Value::Vec3([safe_div(l, r[0]), safe_div(l, r[1]), safe_div(l, r[2])]))
            },
            BinaryOp::Mod => {
                Ok(Value::Vec3([safe_rem(l, r[0]), safe_rem(l, r[1]), safe_rem(l, r[2])]))
            },
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
        // Catch-all: Eq/Neq works on any pair via PartialEq;
        // everything else is a type error.
        _ => {
            if *op == BinaryOp::Eq {
                Ok(Value::Num(if left == right { 1.0 } else { 0.0 }))
            } else if *op == BinaryOp::Neq {
                Ok(Value::Num(if left != right { 1.0 } else { 0.0 }))
            } else {
                Err(EvalError::TypeMismatch(format!(
                    "Unsupported operation {:?} for {:?} and {:?}",
                    op, left, right
                )))
            }
        },
    }
}

/// Evaluate a builtin function by name with already-evaluated arguments.
///
/// Supports all builtins that the IR path handles via `BuiltinFn`:
/// sin, cos, tan, sqrt, exp, ln/log, atan2, clamp, abs, min, max,
/// floor, ceil, deg, rad, lerp, and format.
pub fn eval_builtin_fn(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    match name {
        "format" => eval_format(args),
        "sin" => eval_unary_math(args, "sin", f64::sin),
        "cos" => eval_unary_math(args, "cos", f64::cos),
        "tan" => eval_unary_math(args, "tan", f64::tan),
        "sqrt" => eval_unary_math(args, "sqrt", f64::sqrt),
        "exp" => eval_unary_math(args, "exp", f64::exp),
        "ln" | "log" => eval_unary_math(args, name, f64::ln),
        "abs" => eval_unary_math(args, "abs", f64::abs),
        "floor" => eval_unary_math(args, "floor", f64::floor),
        "ceil" => eval_unary_math(args, "ceil", f64::ceil),
        "signum" => eval_unary_math(args, "signum", f64::signum),
        "fract" => eval_unary_math(args, "fract", f64::fract),
        "deg" | "deg_to_rad" => {
            let x = single_num_arg(name, args)?;
            Ok(Value::Num(x * std::f64::consts::PI / 180.0))
        },
        "rad" | "rad_to_deg" => {
            let x = single_num_arg(name, args)?;
            Ok(Value::Num(x * 180.0 / std::f64::consts::PI))
        },
        "atan2" => {
            let (a, b) = two_num_args(name, args)?;
            Ok(Value::Num(a.atan2(b)))
        },
        "min" => {
            let (a, b) = two_num_args(name, args)?;
            Ok(Value::Num(a.min(b)))
        },
        "max" => {
            let (a, b) = two_num_args(name, args)?;
            Ok(Value::Num(a.max(b)))
        },
        "clamp" => {
            let (val, min, max) = three_num_args(name, args)?;
            Ok(Value::Num(val.clamp(min, max)))
        },
        "lerp" => {
            let (start, end, t) = three_num_args(name, args)?;
            Ok(Value::Num(start + (end - start) * t))
        },
        "hypot" => {
            let (a, b) = two_num_args(name, args)?;
            Ok(Value::Num(a.hypot(b)))
        },
        "pow" => {
            let (a, b) = two_num_args(name, args)?;
            Ok(Value::Num(a.powf(b)))
        },
        "rem" => {
            let (a, b) = two_num_args(name, args)?;
            Ok(Value::Num(a % b))
        },
        "step" => {
            let (edge, x) = two_num_args(name, args)?;
            Ok(Value::Num(if x < edge { 0.0 } else { 1.0 }))
        },
        "round" => {
            let x = single_num_arg(name, args)?;
            Ok(Value::Num(x.round()))
        },
        "list_swap" => {
            if args.len() != 3 {
                return Err(EvalError::TypeMismatch(format!(
                    "list_swap requires 3 arguments (list, i, j), got {}",
                    args.len()
                )));
            }
            match &args[0] {
                Value::List(items) => {
                    let i = args[1].as_num() as usize;
                    let j = args[2].as_num() as usize;
                    let mut new_list = items.clone();
                    if i >= new_list.len() || j >= new_list.len() {
                        tracing::warn!(
                            "list_swap: index out of range (len={}, i={}, j={})",
                            new_list.len(),
                            i,
                            j
                        );
                        return Ok(Value::List(new_list));
                    }
                    new_list.swap(i, j);
                    Ok(Value::List(new_list))
                },
                _ => Err(EvalError::TypeMismatch(format!(
                    "list_swap requires a list as first argument, got {:?}",
                    args[0]
                ))),
            }
        },
        "list_set" => {
            if args.len() != 3 {
                return Err(EvalError::TypeMismatch(format!(
                    "list_set requires 3 arguments (list, i, value), got {}",
                    args.len()
                )));
            }
            match &args[0] {
                Value::List(items) => {
                    let i = args[1].as_num() as usize;
                    let new_value = args[2].clone();
                    let mut new_list = items.clone();
                    if i >= new_list.len() {
                        tracing::warn!(
                            "list_set: index {} out of range for list of length {}",
                            i,
                            new_list.len()
                        );
                        return Ok(Value::List(new_list));
                    }
                    new_list[i] = new_value;
                    Ok(Value::List(new_list))
                },
                _ => Err(EvalError::TypeMismatch(format!(
                    "list_set requires a list as first argument, got {:?}",
                    args[0]
                ))),
            }
        },
        _ => Err(EvalError::UndefinedVariable(name.to_string())),
    }
}

// ─── Helper: unary math ───────────────────────────────────────────────────────

fn eval_unary_math(args: &[Value], name: &str, f: fn(f64) -> f64) -> Result<Value, EvalError> {
    let x = single_num_arg(name, args)?;
    Ok(Value::Num(f(x)))
}

// ─── Shared format implementation ─────────────────────────────────────────────

fn eval_format(args: &[Value]) -> Result<Value, EvalError> {
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
            },
            Value::Str(s) => s.clone(),
            Value::Bool(b) => b.to_string(),
            Value::Vec2(v) => format!("({}, {})", v[0], v[1]),
            Value::Vec3(v) => format!("({}, {}, {})", v[0], v[1], v[2]),
            Value::Vec4(v) | Value::Color(v) => {
                format!("({}, {}, {}, {})", v[0], v[1], v[2], v[3])
            },
            Value::List(items) => format!("{:?}", items),
            Value::Object(name, fields) => format!("{}({:?})", name, fields),
            Value::NativeFn(_) => "<NativeFn>".to_string(),
            Value::Closure(_, _, _) => "<Closure>".to_string(),
        };
        output = output.replacen("{}", &replacement, 1);
    }
    Ok(Value::Str(output))
}

// ─── Argument extraction ──────────────────────────────────────────────────────

/// Extract a single numeric argument, returning a type error otherwise.
fn single_num_arg(name: &str, args: &[Value]) -> Result<f64, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::TypeMismatch(format!("{} expects 1 argument", name)));
    }
    match &args[0] {
        Value::Num(n) => Ok(*n),
        other => {
            Err(EvalError::TypeMismatch(format!("{} expects a number, got {:?}", name, other)))
        },
    }
}

/// Extract two numeric arguments, returning a type error otherwise.
fn two_num_args(name: &str, args: &[Value]) -> Result<(f64, f64), EvalError> {
    if args.len() != 2 {
        return Err(EvalError::TypeMismatch(format!("{} expects 2 arguments", name)));
    }
    match (&args[0], &args[1]) {
        (Value::Num(a), Value::Num(b)) => Ok((*a, *b)),
        _ => Err(EvalError::TypeMismatch(format!(
            "{} expects numbers, got {:?} and {:?}",
            name, args[0], args[1]
        ))),
    }
}

/// Extract three numeric arguments, returning a type error otherwise.
fn three_num_args(name: &str, args: &[Value]) -> Result<(f64, f64, f64), EvalError> {
    if args.len() != 3 {
        return Err(EvalError::TypeMismatch(format!("{} expects 3 arguments", name)));
    }
    match (&args[0], &args[1], &args[2]) {
        (Value::Num(a), Value::Num(b), Value::Num(c)) => Ok((*a, *b, *c)),
        _ => Err(EvalError::TypeMismatch(format!(
            "{} expects numbers, got {:?}, {:?}, {:?}",
            name, args[0], args[1], args[2]
        ))),
    }
}

/// Try to extract a numeric value, returning an error on type mismatch.
pub fn value_to_f64(value: &Value) -> Result<f64, EvalError> {
    match value {
        Value::Num(n) => Ok(*n),
        other => Err(EvalError::TypeMismatch(format!("Expected number, got {:?}", other))),
    }
}

/// Try to extract a Vec2 value, returning an error on type mismatch.
pub fn value_to_vec2(value: &Value) -> Result<[f64; 2], EvalError> {
    match value {
        Value::Vec2(v) => Ok(*v),
        other => Err(EvalError::TypeMismatch(format!("Expected Vec2, got {:?}", other))),
    }
}

/// Try to extract a Vec3 value, returning an error on type mismatch.
pub fn value_to_vec3(value: &Value) -> Result<[f64; 3], EvalError> {
    match value {
        Value::Vec3(v) => Ok(*v),
        other => Err(EvalError::TypeMismatch(format!("Expected Vec3, got {:?}", other))),
    }
}

/// Try to extract a Color value, returning an error on type mismatch.
pub fn value_to_color(value: &Value) -> Result<[f64; 4], EvalError> {
    match value {
        Value::Color(c) => Ok(*c),
        other => Err(EvalError::TypeMismatch(format!("Expected Color, got {:?}", other))),
    }
}

/// Try to extract a List value, returning an error on type mismatch.
pub fn value_to_list(value: &Value) -> Result<Vec<Value>, EvalError> {
    match value {
        Value::List(items) => Ok(items.clone()),
        other => Err(EvalError::TypeMismatch(format!("Expected List, got {:?}", other))),
    }
}

#[cfg(test)]
mod tests {
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
            eval_binary_op(Value::Vec2([1.0, 2.0]), &BinaryOp::Add, Value::Vec2([3.0, 4.0]))
                .unwrap();
        assert_eq!(result.as_vec2(), [4.0, 6.0]);
    }

    #[test]
    fn test_binary_op_vec2_scalar_mul() {
        let result =
            eval_binary_op(Value::Vec2([2.0, 3.0]), &BinaryOp::Mul, Value::Num(4.0)).unwrap();
        assert_eq!(result.as_vec2(), [8.0, 12.0]);

        let result =
            eval_binary_op(Value::Num(4.0), &BinaryOp::Mul, Value::Vec2([2.0, 3.0])).unwrap();
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
            eval_builtin_fn("clamp", &[Value::Num(5.0), Value::Num(0.0), Value::Num(10.0)])
                .unwrap();
        assert_eq!(result.as_num(), 5.0);

        let result =
            eval_builtin_fn("clamp", &[Value::Num(-5.0), Value::Num(0.0), Value::Num(10.0)])
                .unwrap();
        assert_eq!(result.as_num(), 0.0);

        let result =
            eval_builtin_fn("clamp", &[Value::Num(15.0), Value::Num(0.0), Value::Num(10.0)])
                .unwrap();
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
            value_to_list(&Value::List(vec![Value::Num(1.0)])).unwrap(),
            vec![Value::Num(1.0)]
        );
        assert!(value_to_list(&Value::Num(0.0)).is_err());
    }
}
