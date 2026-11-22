//! Numeric specialization of closures stored in the frozen scene environment.

use std::collections::HashMap;
use std::sync::Arc;

use smallvec::SmallVec;

use super::fast_scalar::{FastScalarExpr, compile_fast_scalar};
use super::{BuiltinFn, CompiledExpr};
use crate::ast::{BinaryOp, UnaryOp};
use crate::timeline::env::{CapturedEnv, EnvMap};
use crate::timeline::{Environment, Value};

pub(crate) type NumericClosures = Option<Arc<HashMap<String, NumericClosure>>>;

pub(crate) struct NumericClosure {
    expr: FastScalarExpr,
    inputs: Vec<Input>,
    caller_builtins: Vec<&'static str>,
}

enum Input {
    Parameter(usize),
    Captured(f64),
}

pub(crate) fn compile_frozen(
    env: &Environment,
    base: &mut EnvMap,
    tracks: &std::collections::BTreeMap<String, crate::timeline::VariableTrack>,
) -> NumericClosures {
    let mut static_tracks = Vec::new();
    let plans: HashMap<_, _> = base
        .iter()
        .filter_map(|(name, value)| {
            let track_value = match tracks.get(name) {
                Some(track) if track.keyframes.len() == 1 => Some(track.keyframes.get(&0)?),
                Some(_) => return None,
                None => None,
            };
            let value = track_value.unwrap_or(value);
            let Value::Closure(params, body, captures) = value else {
                return None;
            };
            // A registered contract can affect argument evaluation even when shadowed.
            if env.callable_descriptor(name).is_some()
                || name
                    .strip_prefix("std::")
                    .is_some_and(|name| env.callable_descriptor(name).is_some())
            {
                return None;
            }
            let plan = NumericClosure::compile(params, body, captures, base)?;
            if let Some(value) = track_value {
                static_tracks.push((name.clone(), value.clone()));
            }
            Some((name.clone(), plan))
        })
        .collect();
    // These immutable tracks would otherwise clone the closure on every frame.
    // Freeze precisely the value taken from the frame track.
    base.extend(static_tracks);
    (!plans.is_empty()).then(|| Arc::new(plans))
}

impl NumericClosure {
    fn compile(
        params: &[String],
        body: &CompiledExpr,
        captures: &CapturedEnv,
        base: &EnvMap,
    ) -> Option<Self> {
        let mut caller_builtins = Vec::new();
        check_numeric(body, &mut |name| {
            if params.iter().any(|param| param == name) {
                return false;
            }
            let captured = captures.0.get(name);
            let effective = captured.or_else(|| base.get(name));
            if !crate::timeline::builtins::is_standard_binding(name, effective) {
                return false;
            }
            if captured.is_none() && !caller_builtins.contains(&name) {
                caller_builtins.push(name);
            }
            true
        })
        .then_some(())?;
        let mut slots = Vec::new();
        let expr = compile_fast_scalar(body, "\0", &mut slots)?;
        let inputs = slots
            .into_iter()
            .map(|name| {
                if let Some(index) = params.iter().rposition(|param| param == name) {
                    return Some(Input::Parameter(index));
                }
                let captured = captures.0.get(name);
                // Only lexical captures are immutable here. Caller base names can
                // be shadowed by callables inherited into the closure invocation.
                match captured {
                    Some(Value::Num(value)) => Some(Input::Captured(*value)),
                    _ => None,
                }
            })
            .collect::<Option<_>>()?;
        Some(Self {
            expr,
            inputs,
            caller_builtins,
        })
    }

    pub(crate) fn is_applicable(&self, env: &Environment) -> bool {
        self.caller_builtins.iter().all(|name| !env.overrides.contains_key(*name))
    }

    pub(crate) fn sample(&self, args: &[Value]) -> Option<Value> {
        // Keep type-sensitive or otherwise unsupported calls on the generic path.
        if args.iter().any(|arg| !matches!(arg, Value::Num(_))) {
            return None;
        }
        let slots: SmallVec<[f64; 8]> = self
            .inputs
            .iter()
            .map(|input| match *input {
                Input::Captured(value) => value,
                Input::Parameter(index) => args[index].as_num(),
            })
            .collect();
        Some(Value::Num(self.expr.eval(0.0, &slots)))
    }
}

pub(crate) fn try_named_call(
    env: &Environment,
    name: &str,
    args: &[(Option<String>, CompiledExpr)],
    points: Option<super::GeometryLookup<'_>>,
) -> Option<Result<Value, crate::timeline::EvalError>> {
    if args.iter().any(|(name, _)| name.is_some()) {
        return None;
    }
    let (plan, frozen) = named_plan(env, name, args.len())?;
    let Value::Closure(params, body, captures) = frozen else {
        return None;
    };
    Some((|| {
        let values: SmallVec<[Value; 4]> = args
            .iter()
            .map(|(_, value)| super::evaluate_with_geometry(value, env, points))
            .collect::<Result<_, _>>()?;
        if let Some(value) = plan.sample(&values) {
            return Ok(value);
        }
        crate::timeline::utils::invoke_closure_value(
            params,
            body,
            captures,
            values.into_vec(),
            name,
            env,
            points,
        )
    })())
}

pub(super) fn named_plan<'a>(
    env: &'a Environment,
    name: &str,
    arity: usize,
) -> Option<(&'a NumericClosure, &'a Value)> {
    let plan = env.numeric_closures.as_ref()?.get(name)?;
    if !env.let_scopes.borrow().is_empty()
        || env.bindings.iter().any(Option::is_some)
        || !plan.is_applicable(env)
    {
        return None;
    }
    let frozen = env.base.as_ref()?.get(name)?;
    if !std::ptr::eq(frozen, env.get_ref(name)?) {
        return None;
    }
    let Value::Closure(params, _, _) = frozen else {
        return None;
    };
    if arity != params.len() {
        return None;
    }
    Some((plan, frozen))
}

fn check_numeric(expr: &CompiledExpr, intrinsic: &mut impl FnMut(&'static str) -> bool) -> bool {
    match expr {
        CompiledExpr::Const(Value::Num(_)) => true,
        CompiledExpr::LoadEnv(name) => {
            !name.contains('.') && name != crate::timeline::env::FRAME_TIME_IDENT
        },
        CompiledExpr::Unary(UnaryOp::Neg, inner) => check_numeric(inner, intrinsic),
        CompiledExpr::Binary(
            left,
            BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Mod
            | BinaryOp::Pow,
            right,
        ) => check_numeric(left, intrinsic) && check_numeric(right, intrinsic),
        CompiledExpr::CallBuiltin(builtin, args) => {
            let name = match builtin {
                BuiltinFn::Sin => "sin",
                BuiltinFn::Cos => "cos",
                BuiltinFn::Tan => "tan",
                BuiltinFn::Sqrt => "sqrt",
                BuiltinFn::Exp => "exp",
                BuiltinFn::Log => "ln",
                BuiltinFn::Abs => "abs",
                BuiltinFn::Floor => "floor",
                BuiltinFn::Ceil => "ceil",
                BuiltinFn::Round => "round",
                _ => return false, // Other operations retain the generic evaluator's type and error rules.
            };
            args.len() == 1 && intrinsic(name) && check_numeric(&args[0], intrinsic)
        },
        _ => false, // Reads requiring actor geometry or dynamic dispatch cannot be folded.
    }
}
