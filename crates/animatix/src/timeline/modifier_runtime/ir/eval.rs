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

/// Pre-indexed compiled expression for high-throughput scalar math evaluation (curve plot
/// sampling). Replaces recursive AST matching and string identifier lookups with direct hardware
/// floating point operations and direct slot indexing against a contiguous `&[f64]` slice.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FastScalarExpr {
    Const(f64),
    Arg,
    Slot(usize),
    Neg(Box<FastScalarExpr>),
    Not(Box<FastScalarExpr>),
    Add(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Sub(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Mul(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Div(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Mod(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Pow(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Eq(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Neq(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Lt(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Gt(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Lte(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Gte(Box<FastScalarExpr>, Box<FastScalarExpr>),
    And(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Or(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Select(Box<FastScalarExpr>, Box<FastScalarExpr>, Box<FastScalarExpr>),
    Sin(Box<FastScalarExpr>),
    Cos(Box<FastScalarExpr>),
    Tan(Box<FastScalarExpr>),
    Sqrt(Box<FastScalarExpr>),
    Exp(Box<FastScalarExpr>),
    Log(Box<FastScalarExpr>),
    Abs(Box<FastScalarExpr>),
    Floor(Box<FastScalarExpr>),
    Ceil(Box<FastScalarExpr>),
    Round(Box<FastScalarExpr>),
    Signum(Box<FastScalarExpr>),
    Fract(Box<FastScalarExpr>),
    Deg(Box<FastScalarExpr>),
    Rad(Box<FastScalarExpr>),
    Min(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Max(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Clamp(Box<FastScalarExpr>, Box<FastScalarExpr>, Box<FastScalarExpr>),
    Lerp(Box<FastScalarExpr>, Box<FastScalarExpr>, Box<FastScalarExpr>),
    Hypot(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Step(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Atan2(Box<FastScalarExpr>, Box<FastScalarExpr>),
}

impl FastScalarExpr {
    #[inline(always)]
    pub(crate) fn eval(&self, x: f64, slots: &[f64]) -> f64 {
        match self {
            Self::Const(c) => *c,
            Self::Arg => x,
            Self::Slot(idx) => slots.get(*idx).copied().unwrap_or(f64::NAN),
            Self::Neg(inner) => -inner.eval(x, slots),
            Self::Not(inner) => {
                if inner.eval(x, slots) != 0.0 {
                    0.0
                } else {
                    1.0
                }
            },
            Self::Add(l, r) => l.eval(x, slots) + r.eval(x, slots),
            Self::Sub(l, r) => l.eval(x, slots) - r.eval(x, slots),
            Self::Mul(l, r) => l.eval(x, slots) * r.eval(x, slots),
            Self::Div(l, r) => crate::timeline::utils::safe_div(l.eval(x, slots), r.eval(x, slots)),
            Self::Mod(l, r) => crate::timeline::utils::safe_rem(l.eval(x, slots), r.eval(x, slots)),
            Self::Pow(l, r) => l.eval(x, slots).powf(r.eval(x, slots)),
            Self::Eq(l, r) => {
                if l.eval(x, slots) == r.eval(x, slots) {
                    1.0
                } else {
                    0.0
                }
            },
            Self::Neq(l, r) => {
                if l.eval(x, slots) != r.eval(x, slots) {
                    1.0
                } else {
                    0.0
                }
            },
            Self::Lt(l, r) => {
                if l.eval(x, slots) < r.eval(x, slots) {
                    1.0
                } else {
                    0.0
                }
            },
            Self::Gt(l, r) => {
                if l.eval(x, slots) > r.eval(x, slots) {
                    1.0
                } else {
                    0.0
                }
            },
            Self::Lte(l, r) => {
                if l.eval(x, slots) <= r.eval(x, slots) {
                    1.0
                } else {
                    0.0
                }
            },
            Self::Gte(l, r) => {
                if l.eval(x, slots) >= r.eval(x, slots) {
                    1.0
                } else {
                    0.0
                }
            },
            Self::And(l, r) => {
                if l.eval(x, slots) != 0.0 && r.eval(x, slots) != 0.0 {
                    1.0
                } else {
                    0.0
                }
            },
            Self::Or(l, r) => {
                if l.eval(x, slots) != 0.0 || r.eval(x, slots) != 0.0 {
                    1.0
                } else {
                    0.0
                }
            },
            Self::Select(cond, then_expr, else_expr) => {
                if cond.eval(x, slots) != 0.0 {
                    then_expr.eval(x, slots)
                } else {
                    else_expr.eval(x, slots)
                }
            },
            Self::Sin(inner) => inner.eval(x, slots).sin(),
            Self::Cos(inner) => inner.eval(x, slots).cos(),
            Self::Tan(inner) => inner.eval(x, slots).tan(),
            Self::Sqrt(inner) => inner.eval(x, slots).sqrt(),
            Self::Exp(inner) => inner.eval(x, slots).exp(),
            Self::Log(inner) => inner.eval(x, slots).ln(),
            Self::Abs(inner) => inner.eval(x, slots).abs(),
            Self::Floor(inner) => inner.eval(x, slots).floor(),
            Self::Ceil(inner) => inner.eval(x, slots).ceil(),
            Self::Round(inner) => inner.eval(x, slots).round(),
            Self::Signum(inner) => inner.eval(x, slots).signum(),
            Self::Fract(inner) => inner.eval(x, slots).fract(),
            Self::Deg(inner) => inner.eval(x, slots) * std::f64::consts::PI / 180.0,
            Self::Rad(inner) => inner.eval(x, slots) * 180.0 / std::f64::consts::PI,
            Self::Min(l, r) => l.eval(x, slots).min(r.eval(x, slots)),
            Self::Max(l, r) => l.eval(x, slots).max(r.eval(x, slots)),
            Self::Clamp(val, lo, hi) => {
                val.eval(x, slots).clamp(lo.eval(x, slots), hi.eval(x, slots))
            },
            Self::Lerp(s, e, t) => {
                let sv = s.eval(x, slots);
                let ev = e.eval(x, slots);
                let tv = t.eval(x, slots);
                sv + (ev - sv) * tv
            },
            Self::Hypot(l, r) => l.eval(x, slots).hypot(r.eval(x, slots)),
            Self::Step(edge, x_val) => {
                if x_val.eval(x, slots) < edge.eval(x, slots) {
                    0.0
                } else {
                    1.0
                }
            },
            Self::Atan2(y, x_val) => y.eval(x, slots).atan2(x_val.eval(x, slots)),
        }
    }
}

pub(crate) fn compile_fast_scalar<'a>(
    expr: &'a CompiledExpr,
    arg_name: &str,
    slot_names: &mut Vec<&'a str>,
) -> Option<FastScalarExpr> {
    match expr {
        CompiledExpr::Const(Value::Num(n)) => Some(FastScalarExpr::Const(*n)),
        CompiledExpr::LoadEnv(name) => {
            if name == arg_name {
                Some(FastScalarExpr::Arg)
            } else {
                let slot = if let Some(idx) = slot_names.iter().position(|k| *k == name.as_str()) {
                    idx
                } else {
                    let idx = slot_names.len();
                    slot_names.push(name.as_str());
                    idx
                };
                Some(FastScalarExpr::Slot(slot))
            }
        },
        CompiledExpr::Unary(op, inner) => {
            let inner_fast = compile_fast_scalar(inner, arg_name, slot_names)?;
            match op {
                UnaryOp::Neg => Some(FastScalarExpr::Neg(Box::new(inner_fast))),
                UnaryOp::Not => Some(FastScalarExpr::Not(Box::new(inner_fast))),
                UnaryOp::Ref => None,
            }
        },
        CompiledExpr::Binary(left, op, right) => {
            let l = compile_fast_scalar(left, arg_name, slot_names)?;
            let r = compile_fast_scalar(right, arg_name, slot_names)?;
            Some(match op {
                BinaryOp::Add => FastScalarExpr::Add(Box::new(l), Box::new(r)),
                BinaryOp::Sub => FastScalarExpr::Sub(Box::new(l), Box::new(r)),
                BinaryOp::Mul => FastScalarExpr::Mul(Box::new(l), Box::new(r)),
                BinaryOp::Div => FastScalarExpr::Div(Box::new(l), Box::new(r)),
                BinaryOp::Mod => FastScalarExpr::Mod(Box::new(l), Box::new(r)),
                BinaryOp::Pow => FastScalarExpr::Pow(Box::new(l), Box::new(r)),
                BinaryOp::Eq => FastScalarExpr::Eq(Box::new(l), Box::new(r)),
                BinaryOp::Neq => FastScalarExpr::Neq(Box::new(l), Box::new(r)),
                BinaryOp::Lt => FastScalarExpr::Lt(Box::new(l), Box::new(r)),
                BinaryOp::Gt => FastScalarExpr::Gt(Box::new(l), Box::new(r)),
                BinaryOp::Lte => FastScalarExpr::Lte(Box::new(l), Box::new(r)),
                BinaryOp::Gte => FastScalarExpr::Gte(Box::new(l), Box::new(r)),
                BinaryOp::And => FastScalarExpr::And(Box::new(l), Box::new(r)),
                BinaryOp::Or => FastScalarExpr::Or(Box::new(l), Box::new(r)),
            })
        },
        CompiledExpr::Select(cond, then_expr, else_expr) => {
            let c = compile_fast_scalar(cond, arg_name, slot_names)?;
            let t = compile_fast_scalar(then_expr, arg_name, slot_names)?;
            let e = compile_fast_scalar(else_expr, arg_name, slot_names)?;
            Some(FastScalarExpr::Select(Box::new(c), Box::new(t), Box::new(e)))
        },
        CompiledExpr::CallBuiltin(builtin, args) => match builtin {
            BuiltinFn::Sin if args.len() == 1 => Some(FastScalarExpr::Sin(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Cos if args.len() == 1 => Some(FastScalarExpr::Cos(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Tan if args.len() == 1 => Some(FastScalarExpr::Tan(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Sqrt if args.len() == 1 => Some(FastScalarExpr::Sqrt(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Exp if args.len() == 1 => Some(FastScalarExpr::Exp(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Log if args.len() == 1 => Some(FastScalarExpr::Log(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Abs if args.len() == 1 => Some(FastScalarExpr::Abs(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Floor if args.len() == 1 => Some(FastScalarExpr::Floor(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Ceil if args.len() == 1 => Some(FastScalarExpr::Ceil(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Round if args.len() == 1 => Some(FastScalarExpr::Round(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Signum if args.len() == 1 => Some(FastScalarExpr::Signum(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Fract if args.len() == 1 => Some(FastScalarExpr::Fract(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Deg if args.len() == 1 => Some(FastScalarExpr::Deg(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Rad if args.len() == 1 => Some(FastScalarExpr::Rad(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            BuiltinFn::Min if args.len() == 2 => Some(FastScalarExpr::Min(
                Box::new(compile_fast_scalar(&args[0], arg_name, slot_names)?),
                Box::new(compile_fast_scalar(&args[1], arg_name, slot_names)?),
            )),
            BuiltinFn::Max if args.len() == 2 => Some(FastScalarExpr::Max(
                Box::new(compile_fast_scalar(&args[0], arg_name, slot_names)?),
                Box::new(compile_fast_scalar(&args[1], arg_name, slot_names)?),
            )),
            BuiltinFn::Clamp if args.len() == 3 => Some(FastScalarExpr::Clamp(
                Box::new(compile_fast_scalar(&args[0], arg_name, slot_names)?),
                Box::new(compile_fast_scalar(&args[1], arg_name, slot_names)?),
                Box::new(compile_fast_scalar(&args[2], arg_name, slot_names)?),
            )),
            BuiltinFn::Lerp if args.len() == 3 => Some(FastScalarExpr::Lerp(
                Box::new(compile_fast_scalar(&args[0], arg_name, slot_names)?),
                Box::new(compile_fast_scalar(&args[1], arg_name, slot_names)?),
                Box::new(compile_fast_scalar(&args[2], arg_name, slot_names)?),
            )),
            BuiltinFn::Hypot if args.len() == 2 => Some(FastScalarExpr::Hypot(
                Box::new(compile_fast_scalar(&args[0], arg_name, slot_names)?),
                Box::new(compile_fast_scalar(&args[1], arg_name, slot_names)?),
            )),
            BuiltinFn::Pow if args.len() == 2 => Some(FastScalarExpr::Pow(
                Box::new(compile_fast_scalar(&args[0], arg_name, slot_names)?),
                Box::new(compile_fast_scalar(&args[1], arg_name, slot_names)?),
            )),
            BuiltinFn::Rem if args.len() == 2 => Some(FastScalarExpr::Mod(
                Box::new(compile_fast_scalar(&args[0], arg_name, slot_names)?),
                Box::new(compile_fast_scalar(&args[1], arg_name, slot_names)?),
            )),
            BuiltinFn::Step if args.len() == 2 => Some(FastScalarExpr::Step(
                Box::new(compile_fast_scalar(&args[0], arg_name, slot_names)?),
                Box::new(compile_fast_scalar(&args[1], arg_name, slot_names)?),
            )),
            BuiltinFn::Atan2 if args.len() == 2 => Some(FastScalarExpr::Atan2(
                Box::new(compile_fast_scalar(&args[0], arg_name, slot_names)?),
                Box::new(compile_fast_scalar(&args[1], arg_name, slot_names)?),
            )),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn compile_fast_vec2<'a>(
    expr: &'a CompiledExpr,
    arg_name: &str,
    slot_names: &mut Vec<&'a str>,
) -> Option<(FastScalarExpr, FastScalarExpr)> {
    if let CompiledExpr::MakeVec(items) = expr {
        if items.len() == 2 {
            let x = compile_fast_scalar(&items[0], arg_name, slot_names)?;
            let y = compile_fast_scalar(&items[1], arg_name, slot_names)?;
            return Some((x, y));
        }
    }
    None
}

pub(crate) fn resolve_slot_values(
    slot_names: &[&str],
    env: &Environment,
    captures: &CapturedEnv,
    out: &mut Vec<f64>,
) -> bool {
    out.clear();
    for name in slot_names {
        if let Some(Value::Num(n)) = env.get_path(name) {
            out.push(n);
        } else if let Some(Value::Num(n)) = captures.0.get(*name) {
            out.push(*n);
        } else {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
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
        let fast_expr = compile_fast_scalar(&expr, "x", &mut slot_names)
            .expect("Should compile to FastScalarExpr");

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
}
