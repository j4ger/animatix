//! Shared builtin-function evaluation.

use super::*;

use super::convert::*;
use super::curves::*;
use super::format::*;

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
                    if i >= items.len() || j >= items.len() {
                        tracing::warn!(
                            "list_swap: index out of range (len={}, i={}, j={})",
                            items.len(),
                            i,
                            j
                        );
                        return Ok(Value::List(items.clone()));
                    }
                    let mut new_list = items.to_vec();
                    new_list.swap(i, j);
                    Ok(Value::List(new_list.into()))
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
                    if i >= items.len() {
                        tracing::warn!(
                            "list_set: index {} out of range for list of length {}",
                            i,
                            items.len()
                        );
                        return Ok(Value::List(items.clone()));
                    }
                    let mut new_list = items.to_vec();
                    new_list[i] = new_value;
                    Ok(Value::List(new_list.into()))
                },
                _ => Err(EvalError::TypeMismatch(format!(
                    "list_set requires a list as first argument, got {:?}",
                    args[0]
                ))),
            }
        },
        "factorial" => {
            let n = single_num_arg(name, args)?;
            if n < 0.0 || n.fract() != 0.0 {
                return Err(EvalError::TypeMismatch(format!(
                    "factorial expects a non-negative integer, got {}",
                    n
                )));
            }
            if n > 170.0 {
                return Err(EvalError::TypeMismatch(format!(
                    "factorial overflows for n = {} (max 170)",
                    n as u64
                )));
            }
            let mut acc = 1.0_f64;
            for k in 2..=(n as u64) {
                acc *= k as f64;
            }
            Ok(Value::Num(acc))
        },
        "sum" => {
            let Some(items) = args.first() else {
                return Err(EvalError::TypeMismatch("sum requires 1 argument (list)".to_string()));
            };
            match items {
                // List literals of 2-4 numeric items evaluate to Vec2/3/4;
                // summing their components keeps `{1, 2, 3}` and `{1, 2, 3,
                // 3.5}` usable as sums too.
                Value::List(items) => {
                    let mut acc = 0.0_f64;
                    for item in items.iter() {
                        acc += value_to_f64(item)?;
                    }
                    Ok(Value::Num(acc))
                },
                Value::Vec2(v) => Ok(Value::Num(v[0] + v[1])),
                Value::Vec3(v) => Ok(Value::Num(v[0] + v[1] + v[2])),
                Value::Vec4(v) => Ok(Value::Num(v[0] + v[1] + v[2] + v[3])),
                other => {
                    Err(EvalError::TypeMismatch(format!("sum requires a list, got {:?}", other)))
                },
            }
        },
        "curve_at" => eval_curve_at(args),
        "curve_smooth" => eval_curve_smooth(args),
        "seeded_rand" => {
            let seed = single_num_arg(name, args)?;
            let hash = crate::timeline::builtins::splitmix64(seed.to_bits());
            Ok(Value::Num(hash as f64 / u64::MAX as f64))
        },
        _ => Err(EvalError::UndefinedVariable(name.to_string())),
    }
}

// ─── Curve functions (STAT-1) ────────────────────────────────────────────────

/// Zero-allocation view over a piecewise-linear curve stored in a Value::List.
#[derive(Clone, Copy)]
pub(crate) struct CurveSlice<'a> {
    items: &'a [Value],
}

impl<'a> CurveSlice<'a> {
    pub(crate) fn new(value: &'a Value) -> Result<Self, EvalError> {
        match value {
            Value::List(items) => {
                if items.len() % 2 != 0 {
                    return Err(EvalError::TypeMismatch(format!(
                        "Curve list must have an even number of elements [t0, v0, t1, v1, ...], got length {}",
                        items.len()
                    )));
                }
                for item in items.iter() {
                    if !matches!(item, Value::Num(_)) {
                        return Err(EvalError::TypeMismatch(format!(
                            "Curve expects numerical elements, got {:?}",
                            item
                        )));
                    }
                }
                Ok(CurveSlice { items })
            },
            other => Err(EvalError::TypeMismatch(format!(
                "Curve expects a List<Num> as first argument, got {:?}",
                other
            ))),
        }
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.items.len() / 2
    }

    #[inline]
    pub(crate) fn get(&self, idx: usize) -> (f64, f64) {
        match (&self.items[idx * 2], &self.items[idx * 2 + 1]) {
            (Value::Num(t), Value::Num(v)) => (*t, *v),
            _ => (0.0, 0.0),
        }
    }

    #[inline]
    pub(crate) fn first(&self) -> Option<(f64, f64)> {
        if self.is_empty() {
            None
        } else {
            Some(self.get(0))
        }
    }

    #[inline]
    pub(crate) fn last(&self) -> Option<(f64, f64)> {
        let n = self.len();
        if n == 0 { None } else { Some(self.get(n - 1)) }
    }

    pub(crate) fn binary_search_by_t(&self, t: f64) -> Result<usize, usize> {
        let mut low = 0;
        let mut high = self.len();
        while low < high {
            let mid = low + (high - low) / 2;
            let (pt, _) = self.get(mid);
            if pt < t {
                low = mid + 1;
            } else if pt > t {
                high = mid;
            } else {
                return Ok(mid);
            }
        }
        Err(low)
    }
}
