use super::types::{BuiltinFn, CompiledExpr};
use crate::ast::{BinaryOp, UnaryOp};
use crate::timeline::env::CapturedEnv;
use crate::timeline::{Environment, Value};

/// Pre-indexed compiled expression for high-throughput scalar math evaluation (curve plot
/// sampling): direct floating-point operations and slot indexing against a contiguous
/// `&[f64]` slice, with no AST matching or string identifier lookups.
#[allow(missing_docs)]
#[derive(Clone, Debug, PartialEq)]
pub enum FastScalarExpr {
    Const(f64),
    Arg,
    Slot(usize),
    Neg(Box<FastScalarExpr>),
    Not(Box<FastScalarExpr>),
    Add(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Sub(Box<FastScalarExpr>, Box<FastScalarExpr>),
    Mul(Box<FastScalarExpr>, Box<FastScalarExpr>),
    MulAdd(Box<FastScalarExpr>, Box<FastScalarExpr>, Box<FastScalarExpr>),
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
    SeededRand(Box<FastScalarExpr>),
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
            Self::MulAdd(a, b, c) => a.eval(x, slots) * b.eval(x, slots) + c.eval(x, slots),
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
            Self::SeededRand(inner) => {
                let seed = inner.eval(x, slots);
                let hash = crate::timeline::builtins::splitmix64(seed.to_bits());
                hash as f64 / u64::MAX as f64
            },
        }
    }

    pub(crate) fn contains_arg(&self) -> bool {
        match self {
            Self::Const(_) | Self::Slot(_) => false,
            Self::Arg => true,
            Self::Neg(inner)
            | Self::Not(inner)
            | Self::Sin(inner)
            | Self::Cos(inner)
            | Self::Tan(inner)
            | Self::Sqrt(inner)
            | Self::Exp(inner)
            | Self::Log(inner)
            | Self::Abs(inner)
            | Self::Floor(inner)
            | Self::Ceil(inner)
            | Self::Round(inner)
            | Self::Signum(inner)
            | Self::Fract(inner)
            | Self::Deg(inner)
            | Self::Rad(inner)
            | Self::SeededRand(inner) => inner.contains_arg(),
            Self::Add(l, r)
            | Self::Sub(l, r)
            | Self::Mul(l, r)
            | Self::Div(l, r)
            | Self::Mod(l, r)
            | Self::Pow(l, r)
            | Self::Eq(l, r)
            | Self::Neq(l, r)
            | Self::Lt(l, r)
            | Self::Gt(l, r)
            | Self::Lte(l, r)
            | Self::Gte(l, r)
            | Self::And(l, r)
            | Self::Or(l, r)
            | Self::Min(l, r)
            | Self::Max(l, r)
            | Self::Hypot(l, r)
            | Self::Step(l, r)
            | Self::Atan2(l, r) => l.contains_arg() || r.contains_arg(),
            Self::Select(c, t, e) => c.contains_arg() || t.contains_arg() || e.contains_arg(),
            Self::Clamp(v, lo, hi) => v.contains_arg() || lo.contains_arg() || hi.contains_arg(),
            Self::Lerp(s, e, t) => s.contains_arg() || e.contains_arg() || t.contains_arg(),
            Self::MulAdd(a, b, c) => a.contains_arg() || b.contains_arg() || c.contains_arg(),
        }
    }

    pub(crate) fn fold_slots(&self, slots: &[f64]) -> FastScalarExpr {
        if !self.contains_arg() {
            return Self::Const(self.eval(0.0, slots));
        }
        match self {
            Self::Const(c) => Self::Const(*c),
            Self::Arg => Self::Arg,
            Self::Slot(idx) => Self::Const(slots.get(*idx).copied().unwrap_or(f64::NAN)),
            Self::Neg(inner) => Self::Neg(Box::new(inner.fold_slots(slots))),
            Self::Not(inner) => Self::Not(Box::new(inner.fold_slots(slots))),
            Self::Add(l, r) => match (l.fold_slots(slots), r.fold_slots(slots)) {
                (Self::Mul(a, b), r) => Self::MulAdd(a, b, Box::new(r)),
                (l, Self::Mul(a, b)) => Self::MulAdd(a, b, Box::new(l)),
                (l, r) => Self::Add(Box::new(l), Box::new(r)),
            },
            Self::Sub(l, r) => {
                Self::Sub(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Mul(l, r) => {
                Self::Mul(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::MulAdd(a, b, c) => Self::MulAdd(
                Box::new(a.fold_slots(slots)),
                Box::new(b.fold_slots(slots)),
                Box::new(c.fold_slots(slots)),
            ),
            Self::Div(l, r) => {
                Self::Div(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Mod(l, r) => {
                Self::Mod(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Pow(l, r) => {
                Self::Pow(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Eq(l, r) => {
                Self::Eq(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Neq(l, r) => {
                Self::Neq(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Lt(l, r) => {
                Self::Lt(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Gt(l, r) => {
                Self::Gt(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Lte(l, r) => {
                Self::Lte(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Gte(l, r) => {
                Self::Gte(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::And(l, r) => {
                Self::And(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Or(l, r) => {
                Self::Or(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Select(c, t, e) => Self::Select(
                Box::new(c.fold_slots(slots)),
                Box::new(t.fold_slots(slots)),
                Box::new(e.fold_slots(slots)),
            ),
            Self::Sin(inner) => Self::Sin(Box::new(inner.fold_slots(slots))),
            Self::Cos(inner) => Self::Cos(Box::new(inner.fold_slots(slots))),
            Self::Tan(inner) => Self::Tan(Box::new(inner.fold_slots(slots))),
            Self::Sqrt(inner) => Self::Sqrt(Box::new(inner.fold_slots(slots))),
            Self::Exp(inner) => Self::Exp(Box::new(inner.fold_slots(slots))),
            Self::Log(inner) => Self::Log(Box::new(inner.fold_slots(slots))),
            Self::Abs(inner) => Self::Abs(Box::new(inner.fold_slots(slots))),
            Self::Floor(inner) => Self::Floor(Box::new(inner.fold_slots(slots))),
            Self::Ceil(inner) => Self::Ceil(Box::new(inner.fold_slots(slots))),
            Self::Round(inner) => Self::Round(Box::new(inner.fold_slots(slots))),
            Self::Signum(inner) => Self::Signum(Box::new(inner.fold_slots(slots))),
            Self::Fract(inner) => Self::Fract(Box::new(inner.fold_slots(slots))),
            Self::Deg(inner) => Self::Deg(Box::new(inner.fold_slots(slots))),
            Self::Rad(inner) => Self::Rad(Box::new(inner.fold_slots(slots))),
            Self::Min(l, r) => {
                Self::Min(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Max(l, r) => {
                Self::Max(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Clamp(v, lo, hi) => Self::Clamp(
                Box::new(v.fold_slots(slots)),
                Box::new(lo.fold_slots(slots)),
                Box::new(hi.fold_slots(slots)),
            ),
            Self::Lerp(s, e, t) => Self::Lerp(
                Box::new(s.fold_slots(slots)),
                Box::new(e.fold_slots(slots)),
                Box::new(t.fold_slots(slots)),
            ),
            Self::Hypot(l, r) => {
                Self::Hypot(Box::new(l.fold_slots(slots)), Box::new(r.fold_slots(slots)))
            },
            Self::Step(edge, x) => {
                Self::Step(Box::new(edge.fold_slots(slots)), Box::new(x.fold_slots(slots)))
            },
            Self::Atan2(y, x) => {
                Self::Atan2(Box::new(y.fold_slots(slots)), Box::new(x.fold_slots(slots)))
            },
            Self::SeededRand(inner) => Self::SeededRand(Box::new(inner.fold_slots(slots))),
        }
    }
}

pub(crate) fn boolean_result(expr: &CompiledExpr) -> bool {
    match expr {
        CompiledExpr::Const(Value::Bool(_)) | CompiledExpr::Unary(UnaryOp::Not, _) => true,
        CompiledExpr::Binary(_, op, _) => matches!(
            op,
            BinaryOp::Eq
                | BinaryOp::Neq
                | BinaryOp::Lt
                | BinaryOp::Gt
                | BinaryOp::Lte
                | BinaryOp::Gte
                | BinaryOp::And
                | BinaryOp::Or
        ),
        CompiledExpr::Select(_, yes, no) => boolean_result(yes) && boolean_result(no),
        CompiledExpr::LetChain(_, tail) => boolean_result(tail),
        _ => false, // Remaining scalar leaves are numeric or use generic evaluation.
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
            if *op == UnaryOp::Neg && boolean_result(inner) {
                return None;
            }
            let inner_fast = compile_fast_scalar(inner, arg_name, slot_names)?;
            match op {
                UnaryOp::Neg => Some(FastScalarExpr::Neg(Box::new(inner_fast))),
                UnaryOp::Not => Some(FastScalarExpr::Not(Box::new(inner_fast))),
                UnaryOp::Ref => None,
            }
        },
        CompiledExpr::Binary(left, op, right) => {
            if matches!(
                op,
                BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod
                    | BinaryOp::Pow
            ) && (boolean_result(left) || boolean_result(right))
            {
                return None;
            }
            let l = compile_fast_scalar(left, arg_name, slot_names)?;
            let r = compile_fast_scalar(right, arg_name, slot_names)?;
            Some(match op {
                BinaryOp::Add => match (l, r) {
                    (FastScalarExpr::Mul(a, b), r) => FastScalarExpr::MulAdd(a, b, Box::new(r)),
                    (l, FastScalarExpr::Mul(a, b)) => FastScalarExpr::MulAdd(a, b, Box::new(l)),
                    (l, r) => FastScalarExpr::Add(Box::new(l), Box::new(r)),
                },
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
        CompiledExpr::CallBuiltin(_, args) if args.iter().any(boolean_result) => None,
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
            BuiltinFn::SeededRand if args.len() == 1 => Some(FastScalarExpr::SeededRand(Box::new(
                compile_fast_scalar(&args[0], arg_name, slot_names)?,
            ))),
            _ => None,
        },
        _ => None,
    }
}

/// Compile a scalar specialization only when every intrinsic call resolves to
/// the standard library implementation in the current lexical environment.
/// The full VM remains the fallback for a user or plugin shadow of a builtin.
pub(crate) fn compile_fast_scalar_with_env<'a>(
    expr: &'a CompiledExpr,
    arg_name: &str,
    slot_names: &mut Vec<&'a str>,
    env: &Environment,
) -> Option<FastScalarExpr> {
    if !fast_scalar_intrinsics_are_standard(expr, env) {
        return None;
    }
    compile_fast_scalar(expr, arg_name, slot_names)
}

/// Compile a vector specialization after the same lexical intrinsic check.
pub(crate) fn compile_fast_vec2_with_env<'a>(
    expr: &'a CompiledExpr,
    arg_name: &str,
    slot_names: &mut Vec<&'a str>,
    env: &Environment,
) -> Option<(FastScalarExpr, FastScalarExpr)> {
    if !fast_scalar_intrinsics_are_standard(expr, env) {
        return None;
    }
    compile_fast_vec2(expr, arg_name, slot_names)
}

/// Check intrinsic identity separately from compilation so cached plot
/// programs are revalidated against each lexical environment on cache hits.
pub(crate) fn fast_scalar_intrinsics_are_standard(expr: &CompiledExpr, env: &Environment) -> bool {
    fn builtin_name(builtin: &BuiltinFn) -> &'static str {
        match builtin {
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
            BuiltinFn::SeededRand => "seeded_rand",
        }
    }

    match expr {
        CompiledExpr::CallBuiltin(builtin, args) => {
            crate::timeline::builtins::is_standard_callable(builtin_name(builtin), env)
                && args.iter().all(|argument| fast_scalar_intrinsics_are_standard(argument, env))
        },
        CompiledExpr::Unary(_, inner) => fast_scalar_intrinsics_are_standard(inner, env),
        CompiledExpr::Binary(left, _, right) => {
            fast_scalar_intrinsics_are_standard(left, env)
                && fast_scalar_intrinsics_are_standard(right, env)
        },
        CompiledExpr::Select(condition, yes, no) => {
            fast_scalar_intrinsics_are_standard(condition, env)
                && fast_scalar_intrinsics_are_standard(yes, env)
                && fast_scalar_intrinsics_are_standard(no, env)
        },
        CompiledExpr::MakeVec(items) | CompiledExpr::MakeList(items) => {
            items.iter().all(|item| fast_scalar_intrinsics_are_standard(item, env))
        },
        _ => true,
    }
}

pub(crate) fn compile_fast_vec2<'a>(
    expr: &'a CompiledExpr,
    arg_name: &str,
    slot_names: &mut Vec<&'a str>,
) -> Option<(FastScalarExpr, FastScalarExpr)> {
    if let CompiledExpr::MakeVec(items) = expr
        && items.len() == 2
        && !items.iter().any(boolean_result)
    {
        let x = compile_fast_scalar(&items[0], arg_name, slot_names)?;
        let y = compile_fast_scalar(&items[1], arg_name, slot_names)?;
        return Some((x, y));
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
