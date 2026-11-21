//! Expression evaluation: the evaluation cache, expression hashing, and the
//! call/closure/method dispatch used by the IR VM.

mod registered;
use registered::resolve_registered_callback;

use crate::timeline::modifier_runtime::ir::PointLookup;
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::ast::{Expr, MatchPattern};
use crate::timeline::env::{CapturedEnv, Environment, EvalError, Value};

use super::value::format_value;

// Thread-local expression evaluation cache.
//
// Caches `(expr_hash, env_stamp) → Value` to avoid re-evaluating the same
// expression in the same environment during build. The stamp is O(1)
// (instance id + mutation counter, see `EnvStamp`), so cache lookups no
// longer hash the whole environment. The cache is cleared between builds via
// [`clear_eval_cache`] and capped so long-running GUI sessions cannot grow
// without bound.
thread_local! {
    static EVAL_CACHE: RefCell<HashMap<(u64, crate::timeline::env::EnvStamp, u64), Value>> =
        RefCell::new(HashMap::new());
    static COMPILED_EXPR_CACHE: RefCell<
        HashMap<(u64, usize), Arc<crate::timeline::modifier_runtime::ir::CompiledExpr>>,
    > = RefCell::new(HashMap::new());
    /// Flag to disable the eval cache for tight sampling loops.
    /// When false, evaluate_expr skips hashing and the cache entirely.
    static EVAL_CACHE_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

const EVAL_CACHE_LIMIT: usize = 4096;

/// Clear the thread-local expression evaluation cache.
/// Call this at the start of each build to avoid stale results.
pub fn clear_eval_cache() {
    EVAL_CACHE.with(|cache| cache.borrow_mut().clear());
    COMPILED_EXPR_CACHE.with(|cache| cache.borrow_mut().clear());
}

#[cfg(test)]
pub(crate) fn compiled_expr_cache_len() -> usize {
    COMPILED_EXPR_CACHE.with(|cache| cache.borrow().len())
}

#[cfg(test)]
pub(crate) fn eval_cache_len() -> usize {
    EVAL_CACHE.with(|cache| cache.borrow().len())
}

/// Disable the expression evaluation cache for the current thread.
/// Call before entering a tight sampling loop where x/y change every call
/// and the cache will never hit.
pub fn disable_eval_cache() {
    EVAL_CACHE_ENABLED.set(false);
}

/// Re-enable the expression evaluation cache after a sampling loop.
pub fn enable_eval_cache() {
    EVAL_CACHE_ENABLED.set(true);
}

/// Safe scalar division: returns 0.0 when divisor is zero.
#[inline]
pub fn safe_div(l: f64, r: f64) -> f64 {
    if r != 0.0 { l / r } else { 0.0 }
}

/// Safe scalar remainder: returns 0.0 when divisor is zero.
#[inline]
pub fn safe_rem(l: f64, r: f64) -> f64 {
    if r != 0.0 { l % r } else { 0.0 }
}

/// Hash an expression tree for cache keying. Plan-bearing expressions are
/// intentionally uncached because evaluation captures a fresh plan value.
pub(crate) fn expr_hash(expr: &Expr) -> Option<u64> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hash_expr_recursive(expr, &mut hasher)?;
    Some(hasher.finish())
}

fn hash_match_pattern<V: Hasher>(pat: &MatchPattern, hasher: &mut V) {
    match pat {
        MatchPattern::Wildcard => {
            0u8.hash(hasher);
        },
        MatchPattern::Num(n) => {
            1u8.hash(hasher);
            n.to_bits().hash(hasher);
        },
        MatchPattern::Str(s) => {
            2u8.hash(hasher);
            s.hash(hasher);
        },
        MatchPattern::Bool(b) => {
            3u8.hash(hasher);
            b.hash(hasher);
        },
        MatchPattern::Range(lo, hi) => {
            4u8.hash(hasher);
            hash_match_pattern(lo, hasher);
            hash_match_pattern(hi, hasher);
        },
        MatchPattern::Or(pats) => {
            5u8.hash(hasher);
            pats.len().hash(hasher);
            for p in pats {
                hash_match_pattern(p, hasher);
            }
        },
        MatchPattern::Tuple(pats) => {
            6u8.hash(hasher);
            pats.len().hash(hasher);
            for p in pats {
                hash_match_pattern(p, hasher);
            }
        },
    }
}

fn hash_expr_recursive<V: Hasher>(expr: &Expr, hasher: &mut V) -> Option<()> {
    match expr {
        Expr::Num(n) => {
            0u8.hash(hasher);
            n.to_bits().hash(hasher);
        },
        Expr::Percent(n) => {
            1u8.hash(hasher);
            n.to_bits().hash(hasher);
        },
        Expr::Str(s) => {
            2u8.hash(hasher);
            s.hash(hasher);
        },
        Expr::Color(rgba) => {
            22u8.hash(hasher);
            rgba.hash(hasher);
        },
        Expr::Bool(b) => {
            3u8.hash(hasher);
            b.hash(hasher);
        },
        Expr::Null => {
            4u8.hash(hasher);
        },
        Expr::Ident(s) => {
            5u8.hash(hasher);
            s.hash(hasher);
        },
        Expr::Path(parts) => {
            6u8.hash(hasher);
            parts.hash(hasher);
        },
        Expr::Index(a, b) => {
            7u8.hash(hasher);
            hash_expr_recursive(a, hasher)?;
            hash_expr_recursive(b, hasher)?;
        },
        Expr::Tuple(items) => {
            8u8.hash(hasher);
            items.len().hash(hasher);
            for e in items {
                hash_expr_recursive(e, hasher)?;
            }
        },
        Expr::List(items) => {
            9u8.hash(hasher);
            items.len().hash(hasher);
            for e in items {
                hash_expr_recursive(e, hasher)?;
            }
        },
        Expr::Binary(a, op, b) => {
            10u8.hash(hasher);
            hash_expr_recursive(a, hasher)?;
            op.hash(hasher);
            hash_expr_recursive(b, hasher)?;
        },
        Expr::Unary(op, e) => {
            11u8.hash(hasher);
            op.hash(hasher);
            hash_expr_recursive(e, hasher)?;
        },
        Expr::Call(callee, args) => {
            12u8.hash(hasher);
            hash_expr_recursive(callee, hasher)?;
            args.len().hash(hasher);
            for a in args {
                a.name.hash(hasher);
                hash_expr_recursive(&a.value, hasher)?;
            }
        },
        Expr::Field(recv, name) => {
            13u8.hash(hasher);
            hash_expr_recursive(recv, hasher)?;
            name.hash(hasher);
        },
        Expr::QualifiedPath(parts) => {
            14u8.hash(hasher);
            parts.hash(hasher);
        },
        Expr::Closure(params, body) => {
            15u8.hash(hasher);
            params.hash(hasher);
            hash_expr_recursive(body, hasher)?;
        },
        Expr::Timeline(_) | Expr::Sequence(_) | Expr::Stagger { .. } => return None,
        Expr::Duration(seconds) => {
            20u8.hash(hasher);
            seconds.to_bits().hash(hasher);
        },
        Expr::Unit => {
            21u8.hash(hasher);
        },
        Expr::LetChain(bindings, tail) => {
            16u8.hash(hasher);
            bindings.len().hash(hasher);
            for (name, value) in bindings {
                name.hash(hasher);
                hash_expr_recursive(value, hasher)?;
            }
            hash_expr_recursive(tail, hasher)?;
        },
        Expr::Conditional(c, t, e) => {
            17u8.hash(hasher);
            hash_expr_recursive(c, hasher)?;
            hash_expr_recursive(t, hasher)?;
            hash_expr_recursive(e, hasher)?;
        },
        Expr::Match(scrutinee, arms) => {
            18u8.hash(hasher);
            hash_expr_recursive(scrutinee, hasher)?;
            arms.len().hash(hasher);
            for (p, e) in arms {
                hash_match_pattern(p, hasher);
                hash_expr_recursive(e, hasher)?;
            }
        },
        Expr::Construct(name, properties) => {
            19u8.hash(hasher);
            name.hash(hasher);
            properties.len().hash(hasher);
            for prop in properties {
                prop.name.hash(hasher);
                hash_expr_recursive(&prop.value, hasher)?;
            }
        },
    }
    Some(())
}

/// True for expressions whose direct evaluation is cheaper than a cache
/// round-trip: literals, single variable lookups, and tuples/lists built
/// entirely from those.
fn expr_is_trivial(expr: &Expr) -> bool {
    match expr {
        Expr::Num(_)
        | Expr::Percent(_)
        | Expr::Str(_)
        | Expr::Color(_)
        | Expr::Bool(_)
        | Expr::Null
        | Expr::Ident(_) => true,
        Expr::Tuple(items) | Expr::List(items) => items.iter().all(expr_is_trivial),
        _ => false,
    }
}

/// Represents a runtime value produced by evaluating an expression.
///
/// Uses a thread-local cache keyed on `(expr_content_hash, env.stamp())` to
/// avoid re-evaluating the same expression in the same environment during
/// build. The environment stamp is O(1) — instance id plus a mutation counter
/// (see `EnvStamp`) — so equal stamps guarantee equal contents without hashing
/// every override entry.
pub fn evaluate_expr(expr: &Expr, env: &Environment) -> Result<Value, EvalError> {
    evaluate_expr_with_points(expr, env, None)
}

pub(crate) fn evaluate_expr_with_points(
    expr: &Expr,
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    // Fast path: trivial expressions and disabled caches (e.g., inside tight
    // plot sampling loops) evaluate directly — a cache round-trip would cost
    // more than the evaluation itself.
    if points.is_some() || !EVAL_CACHE_ENABLED.get() || expr_is_trivial(expr) {
        return evaluate_expr_inner(expr, env, points);
    }

    // The let-scope epoch is part of the identity: the same env instance can
    // produce different values for the same expression under different
    // active LetChain scopes.
    let Some(hash) = expr_hash(expr) else {
        return evaluate_expr_uncached(expr, env, points);
    };
    let cache_key = (hash, env.stamp(), env.let_epoch());

    // Check cache
    {
        let hit = EVAL_CACHE.with(|cache| cache.borrow().get(&cache_key).cloned());
        if let Some(value) = hit {
            return Ok(value);
        }
    }

    // Evaluate and cache
    let result = evaluate_expr_with_hash(expr, env, hash, points)?;
    EVAL_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= EVAL_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(cache_key, result.clone());
    });
    Ok(result)
}

/// Inner evaluation function delegating to compiled IR VM.
fn evaluate_expr_inner(
    expr: &Expr,
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    let Some(hash) = expr_hash(expr) else {
        return evaluate_expr_uncached(expr, env, points);
    };
    evaluate_expr_with_hash(expr, env, hash, points)
}

fn evaluate_expr_with_hash(
    expr: &Expr,
    env: &Environment,
    hash: u64,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    let key = (hash, expr as *const Expr as usize);
    let hit = COMPILED_EXPR_CACHE.with(|cache| cache.borrow().get(&key).cloned());
    let compiled = match hit {
        Some(compiled) => compiled,
        None => {
            let compiled = Arc::new(crate::timeline::modifier_runtime::ir::compile_expr(expr)?);
            COMPILED_EXPR_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                if cache.len() >= EVAL_CACHE_LIMIT {
                    cache.clear();
                }
                cache.insert(key, Arc::clone(&compiled));
            });
            compiled
        },
    };
    crate::timeline::modifier_runtime::ir::evaluate_with_points(&compiled, env, points)
}

fn evaluate_expr_uncached(
    expr: &Expr,
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    let compiled = crate::timeline::modifier_runtime::ir::compile_expr(expr)?;
    crate::timeline::modifier_runtime::ir::evaluate_with_points(&compiled, env, points)
}

/// Invoke a named function with already-evaluated arguments.
///
/// Shared function invocation logic so that calls to `NativeFn`
/// values, user functions, and closures follow one unified semantic path.
pub(crate) fn evaluate_call_value(
    func: &str,
    arg_values: Vec<Value>,
    env: &Environment,
) -> Result<Value, EvalError> {
    evaluate_call_arguments(
        func,
        arg_values.into_iter().map(|value| (None, value)).collect(),
        env,
        None,
    )
}

/// Invoke a named callable with values that retain positional/named identity.
pub(crate) fn evaluate_call_arguments(
    func: &str,
    arg_values: Vec<(Option<String>, Value)>,
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    let unqualified = func.strip_prefix("std::").unwrap_or(func);
    let function = if func.starts_with("std::") {
        env.get(func)
    } else {
        env.get(func).or_else(|| env.get(unqualified))
    };

    if let Some(function @ (Value::RegisteredFn { .. } | Value::Closure(..))) = function.as_ref() {
        return call_with_points(function.clone(), arg_values, func, env, points);
    }

    if let Some(Value::UserFn {
        name,
        params,
        return_type,
        body,
        function_context,
        definition_path,
        component_scope,
    }) = function.as_ref()
    {
        return crate::timeline::fn_eval::evaluate_user_fn_named(
            name,
            params,
            return_type.as_ref(),
            body,
            arg_values,
            env,
            function_context.as_ref(),
            definition_path.as_deref(),
            component_scope.as_ref(),
            points,
        );
    }

    if let Some(value) = &function
        && !matches!(value, Value::NativeFn(_))
    {
        return Err(EvalError::NotCallable(format!("'{func}' resolves to {value:?}")));
    }

    let descriptor = env.callable_descriptor(func).or_else(|| env.callable_descriptor(unqualified));
    if let Some(descriptor) = descriptor {
        if env.is_frame_phase() && descriptor.phase == animatix_core::callable::CallablePhase::Build
        {
            return Err(EvalError::UnsupportedConstruct(format!(
                "'{}' is available only while building a timeline",
                descriptor.qualified_name()
            )));
        }
        let args = crate::timeline::callable::bind_descriptor_arguments(
            unqualified,
            &descriptor.params,
            arg_values,
        )?;

        if let Some(kind) =
            animatix_core::builtins::callable::TimelineBuiltin::from_name(unqualified)
        {
            if env.is_frame_phase() {
                return Err(EvalError::UnsupportedConstruct(format!(
                    "timeline callable '{}' cannot run while sampling a frame",
                    descriptor.qualified_name()
                )));
            }
            let call = crate::timeline::plan_value::BoundBuiltinCall {
                name: kind.descriptor().name.clone(),
                args: descriptor
                    .params
                    .iter()
                    .zip(args)
                    .map(|(param, value)| (param.name.clone(), value))
                    .collect(),
                byte_span: None,
            };
            return Ok(Value::TimelinePlan(std::sync::Arc::new(
                crate::timeline::plan_value::TimelinePlan::from_ops(vec![
                    crate::timeline::plan_value::PlanOp::Builtin(call),
                ]),
            )));
        }

        match (unqualified, args.as_slice()) {
            ("stretch", [Value::TimelinePlan(plan), Value::Num(factor)]) => {
                return plan
                    .stretched(*factor)
                    .map(std::sync::Arc::new)
                    .map(Value::TimelinePlan)
                    .map_err(|error| EvalError::TypeMismatch(error.to_string()));
            },
            ("fit_duration", [Value::TimelinePlan(plan), Value::Duration(seconds)]) => {
                let duration = crate::timeline::plan_value::PlanDuration::from_seconds(*seconds)
                    .map_err(|error| EvalError::TypeMismatch(error.to_string()))?;
                let fitted = plan
                    .fit_duration(duration)
                    .map_err(|error| EvalError::TypeMismatch(error.to_string()))?;
                return Ok(Value::TimelinePlan(std::sync::Arc::new(fitted)));
            },
            ("seconds", [Value::Num(seconds)]) => return Ok(Value::Duration(*seconds)),
            ("milliseconds", [Value::Num(milliseconds)]) => {
                return Ok(Value::Duration(milliseconds / 1000.0));
            },
            ("as_seconds", [Value::Duration(seconds)]) => return Ok(Value::Num(*seconds)),
            ("format", [Value::Str(template), Value::List(items)]) => {
                let mut flat = Vec::with_capacity(items.len() + 1);
                flat.push(Value::Str(template.clone()));
                flat.extend(items.iter().cloned());
                let value = crate::timeline::eval_shared::eval_builtin_fn("format", &flat)?;
                crate::timeline::callable::validate_return_value(descriptor, &value)?;
                return Ok(value);
            },
            ("spring", [Value::Num(damping), Value::Num(frequency)]) => {
                return Ok(Value::Easing(crate::easing::Easing::Spring {
                    damping: *damping as f32,
                    frequency: *frequency as f32,
                }));
            },
            (
                "cubic_bezier",
                [
                    Value::Num(x1),
                    Value::Num(y1),
                    Value::Num(x2),
                    Value::Num(y2),
                ],
            ) => {
                return Ok(Value::Easing(crate::easing::Easing::CubicBezier([
                    *x1 as f32, *y1 as f32, *x2 as f32, *y2 as f32,
                ])));
            },
            _ => {},
        }

        let native = function.clone().or_else(|| env.get_path(unqualified));
        if let Some(Value::NativeFn(native)) = native {
            let value = native(&args, env)?;
            crate::timeline::callable::validate_return_value(descriptor, &value)?;
            return Ok(value);
        }

        if let Ok(value) = crate::timeline::eval_shared::eval_builtin_fn(unqualified, &args) {
            crate::timeline::callable::validate_return_value(descriptor, &value)?;
            return Ok(value);
        }
        if let Some(Value::Closure(params, body, captures)) = function.as_ref() {
            let values = args;
            return invoke_closure_value(params, body, captures, values, func, env, points);
        }
        return Err(EvalError::UndefinedVariable(func.to_string()));
    }

    if arg_values.iter().any(|(name, _)| name.is_some()) {
        return Err(EvalError::TypeMismatch(format!(
            "named arguments cannot be bound for callable '{func}' because it has no descriptor"
        )));
    }
    let args: Vec<Value> = arg_values.into_iter().map(|(_, value)| value).collect();
    if let Some(value) = function {
        return call_with_points(
            value,
            args.into_iter().map(|v| (None, v)).collect(),
            func,
            env,
            points,
        );
    }

    // The pure math VM remains the fallback for internal built-ins which do
    // not have a native function binding in the environment.
    crate::timeline::eval_shared::eval_builtin_fn(unqualified, &args)
}

/// Invoke a function value that is not addressed by a static source name.
pub(crate) fn evaluate_callable_value(
    function: Value,
    arguments: Vec<(Option<String>, Value)>,
    name: &str,
    env: &Environment,
) -> Result<Value, EvalError> {
    call_with_points(function, arguments, name, env, None)
}

pub(crate) fn call_with_points(
    function: Value,
    arguments: Vec<(Option<String>, Value)>,
    name: &str,
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    match function {
        Value::RegisteredFn {
            name,
            descriptor,
            native,
        } => {
            if env.is_frame_phase()
                && descriptor.phase == animatix_core::callable::CallablePhase::Build
            {
                return Err(EvalError::UnsupportedConstruct(format!(
                    "'{}' is available only while building a timeline",
                    descriptor.qualified_name()
                )));
            }
            if native.is_none()
                && let Some(kind) =
                    animatix_core::builtins::callable::TimelineBuiltin::from_name(&descriptor.name)
            {
                let values = crate::timeline::callable::bind_descriptor_arguments(
                    &descriptor.qualified_name(),
                    &descriptor.params,
                    arguments,
                )?;
                let call = crate::timeline::plan_value::BoundBuiltinCall {
                    name: kind.descriptor().name.clone(),
                    args: descriptor
                        .params
                        .iter()
                        .zip(values)
                        .map(|(param, value)| (param.name.clone(), value))
                        .collect(),
                    byte_span: None,
                };
                return Ok(Value::TimelinePlan(std::sync::Arc::new(
                    crate::timeline::plan_value::TimelinePlan::from_ops(vec![
                        crate::timeline::plan_value::PlanOp::Builtin(call),
                    ]),
                )));
            }
            let native = match native {
                Some(native) => native,
                None => resolve_registered_callback(&name, &descriptor, env)?,
            };
            let values = crate::timeline::callable::bind_descriptor_arguments(
                &descriptor.qualified_name(),
                &descriptor.params,
                arguments,
            )?;
            let value = native(&values, env)?;
            crate::timeline::callable::validate_return_value(&descriptor, &value)?;
            Ok(value)
        },
        Value::NativeFn(native) => {
            if arguments.iter().any(|(arg_name, _)| arg_name.is_some()) {
                return Err(EvalError::TypeMismatch(format!(
                    "named arguments to dynamic callable '{name}' need a registered descriptor"
                )));
            }
            let values = arguments.into_iter().map(|(_, value)| value).collect::<Vec<_>>();
            native(&values, env)
        },
        Value::UserFn {
            name: fn_name,
            params,
            return_type,
            body,
            function_context,
            definition_path,
            component_scope,
        } => crate::timeline::fn_eval::evaluate_user_fn_named(
            &fn_name,
            &params,
            return_type.as_ref(),
            &body,
            arguments,
            env,
            function_context.as_ref(),
            definition_path.as_deref(),
            component_scope.as_ref(),
            points,
        ),
        Value::Closure(params, body, captures) => {
            if arguments.iter().any(|(arg_name, _)| arg_name.is_some()) {
                return Err(EvalError::TypeMismatch(format!(
                    "named arguments to closure '{name}' are not supported"
                )));
            }
            let values = arguments.into_iter().map(|(_, value)| value).collect();
            invoke_closure_value(&params, &body, &captures, values, name, env, points)
        },
        _ => Err(EvalError::NotCallable(name.to_string())),
    }
}

/// Invoke a member call whose receiver was evaluated from a non-static value.
pub(crate) fn evaluate_method_arguments(
    receiver: Value,
    name: &str,
    arguments: Vec<(Option<String>, Value)>,
    env: &Environment,
) -> Result<Value, EvalError> {
    // Primitive helpers such as Graph.map are installed under the concrete
    // actor's member key. Resolve that callable after evaluating its receiver,
    // so aliases and component-qualified actor references use the same helper.
    if let Value::ActorRef(actor) = &receiver
        && let Some(callable) =
            env.get_path(&crate::timeline::env_keys::native_fn(actor.label(), name))
    {
        return evaluate_callable_value(callable, arguments, name, env);
    }
    if let Value::NativeFn(native) = receiver {
        if arguments.iter().any(|(arg_name, _)| arg_name.is_some()) {
            return Err(EvalError::TypeMismatch(format!(
                "named arguments to method '{name}' need a registered descriptor"
            )));
        }
        let values = arguments.into_iter().map(|(_, value)| value).collect::<Vec<_>>();
        return native(&values, env);
    }
    if arguments.iter().any(|(arg_name, _)| arg_name.is_some()) {
        return Err(EvalError::TypeMismatch(format!(
            "named arguments to dynamic method '{name}' are not supported"
        )));
    }
    let values = arguments.into_iter().map(|(_, value)| value).collect::<Vec<_>>();
    eval_method_dispatch(receiver, name, &values, env)
}

/// Invoke a closure value with already-evaluated arguments.
///
/// Closures evaluate against the captured (lexical) environment, then bind
/// parameters on top. Free variables resolve to their values at creation
/// time, not call time. Shared by `evaluate_call_value` and by builtins
/// that take a closure argument (e.g. `sum_range`).
pub(crate) fn invoke_closure_value(
    params: &[String],
    body: &crate::timeline::modifier_runtime::ir::CompiledExpr,
    captures: &CapturedEnv,
    arg_values: Vec<Value>,
    func_name: &str,
    env: &Environment,
    points: Option<PointLookup<'_>>,
) -> Result<Value, EvalError> {
    if arg_values.len() != params.len() {
        return Err(EvalError::TypeMismatch(format!(
            "Closure '{}' expects {} arguments, got {}",
            func_name,
            params.len(),
            arg_values.len()
        )));
    }

    // NativeFn reachability inside the closure body depends on WHEN the
    // closure is invoked:
    // - Frame time: the caller env carries the frozen stdlib base, propagated into the child env
    //   below.
    // - Build time (pre-freeze): base is None, but the stdlib lives in the caller's overrides and
    //   is therefore part of the lexical captures.
    // Both cases resolve; the fallback `Environment::new()` covers the build
    // case, where lookup falls back to the merged captures.

    // Propagate the caller's stdlib base so NativeFn values
    // (e.g. colorscheme samplers) remain reachable inside the
    // closure body.  Captured overrides are merged on top.
    let mut child_env = if let Some(base) = env.base.as_ref() {
        Environment::with_base(std::sync::Arc::clone(base))
    } else {
        Environment::new()
    };
    child_env.inherit_callables_from(env);
    captures.merge_into(&mut child_env);
    // Propagate the caller's plot-sampling bindings (e.g. the bound plot
    // argument `x`): they live in binding slots, never in `CapturedEnv`, so
    // a nested closure referencing the outer argument would otherwise
    // resolve to UndefinedVariable. Slots whose name collides with a
    // parameter are skipped — the parameter binding must win.
    for (dst, src) in child_env.bindings.iter_mut().zip(env.bindings.iter()) {
        if let Some((name, _)) = src
            && params.contains(name)
        {
            continue;
        }
        *dst = src.clone();
    }
    for (param, val) in params.iter().zip(arg_values) {
        child_env.set(param, val);
    }

    crate::timeline::modifier_runtime::ir::evaluate_with_points(body, &child_env, points)
}

/// Evaluate a method call on a receiver value.
/// Shared method dispatch logic that operates on already-evaluated argument values.
/// This is used by the unified IR/VM expression engine (eval_method)
/// and function call dispatcher to avoid code duplication.
pub(crate) fn eval_method_dispatch(
    receiver: Value,
    name: &str,
    args: &[Value],
    _env: &Environment,
) -> Result<Value, EvalError> {
    // Dispatch to NativeFn if receiver is a NativeFn (e.g. graph.map)
    if let Value::NativeFn(f) = &receiver {
        return f(args, _env);
    }

    // Object field access support: obj.field() or obj.field
    if let Value::Object(_type_name, fields) = &receiver
        && args.is_empty()
        && let Some(field_value) = fields.get(name)
    {
        return Ok(field_value.clone());
    }

    match (receiver, name) {
        (Value::Str(s), "length") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "String.length() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(s.len() as f64))
        },
        (Value::Str(s), "split") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "String.split(delim) takes exactly 1 argument".to_string(),
                ));
            }
            let delim = args[0].as_str();
            let parts: Vec<Value> =
                s.split(&delim).map(|part| Value::Str(part.to_string())).collect();
            Ok(Value::List(parts.into()))
        },
        (Value::Str(s), "contains") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "String.contains(substr) takes exactly 1 argument".to_string(),
                ));
            }
            let substr = args[0].as_str();
            Ok(Value::Num(if s.contains(&substr) { 1.0 } else { 0.0 }))
        },
        (Value::Str(s), "trim") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "String.trim() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Str(s.trim().to_string()))
        },
        (Value::Str(s), "starts_with") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "String.starts_with(prefix) takes exactly 1 argument".to_string(),
                ));
            }
            let prefix = args[0].as_str();
            Ok(Value::Num(if s.starts_with(&prefix) { 1.0 } else { 0.0 }))
        },
        (Value::Str(s), "ends_with") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "String.ends_with(suffix) takes exactly 1 argument".to_string(),
                ));
            }
            let suffix = args[0].as_str();
            Ok(Value::Num(if s.ends_with(&suffix) { 1.0 } else { 0.0 }))
        },
        (Value::List(items), "length") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch(
                    "List.length() takes no arguments".to_string(),
                ));
            }
            Ok(Value::Num(items.len() as f64))
        },
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
        },
        (Value::List(items), "contains") => {
            if args.len() != 1 {
                return Err(EvalError::TypeMismatch(
                    "List.contains(item) takes exactly 1 argument".to_string(),
                ));
            }
            let item = args[0].clone();
            Ok(Value::Num(if items.contains(&item) { 1.0 } else { 0.0 }))
        },
        (Value::Num(n), "abs") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch("Num.abs() takes no arguments".to_string()));
            }
            Ok(Value::Num(n.abs()))
        },
        (Value::Num(n), "floor") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch("Num.floor() takes no arguments".to_string()));
            }
            Ok(Value::Num(n.floor()))
        },
        (Value::Num(n), "ceil") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch("Num.ceil() takes no arguments".to_string()));
            }
            Ok(Value::Num(n.ceil()))
        },
        (Value::Num(n), "round") => {
            if !args.is_empty() {
                return Err(EvalError::TypeMismatch("Num.round() takes no arguments".to_string()));
            }
            Ok(Value::Num(n.round()))
        },
        (receiver, name) => {
            Err(EvalError::UnsupportedMethod(format!("{}.{}()", format_value(&receiver), name)))
        },
    }
}

#[cfg(all(test, feature = "serde"))]
mod registered_callable_serde_tests {
    use super::*;
    use animatix_core::callable::{CallableDescriptor, CallableParam, CallablePhase, CallableType};

    #[test]
    fn serialized_standard_callable_alias_relinks_and_keeps_named_parameters() {
        let mut env = crate::timeline::builtins::standard_environment();
        let descriptor = animatix_core::builtins::callable::callable("std::sin")
            .expect("standard sin descriptor")
            .clone();
        let Value::NativeFn(native) = env.get("std::sin").expect("qualified standard callback")
        else {
            panic!("expected a native standard callback");
        };
        let alias = Value::RegisteredFn {
            name: descriptor.qualified_name(),
            descriptor: Arc::new(descriptor),
            native: Some(native),
        };
        let json = serde_json::to_string(&alias).expect("serialize callable alias");
        let restored: Value = serde_json::from_str(&json).expect("deserialize callable alias");
        env.set("sine_alias", restored.clone());

        let result = evaluate_callable_value(
            restored,
            vec![(Some("value".to_string()), Value::Num(0.0))],
            "sine_alias",
            &env,
        )
        .expect("relinked alias should accept named arguments");
        assert_eq!(result, Value::Num(0.0));
    }

    #[test]
    fn serialized_callable_without_its_plugin_fails_loudly() {
        let descriptor = CallableDescriptor {
            name: "twice".into(),
            namespace: "missing_plugin".into(),
            params: vec![CallableParam::required("value", CallableType::Num)],
            return_type: CallableType::Num,
            phase: CallablePhase::BuildAndFrame,
            help: String::new(),
            category: String::new(),
        };
        let value = Value::RegisteredFn {
            name: descriptor.qualified_name(),
            descriptor: Arc::new(descriptor),
            native: None,
        };
        let json = serde_json::to_string(&value).expect("serialize plugin callable identity");
        let restored: Value = serde_json::from_str(&json).expect("deserialize callable identity");
        let env = crate::timeline::builtins::standard_environment();

        let error =
            evaluate_callable_value(restored, vec![(None, Value::Num(2.0))], "twice_alias", &env)
                .expect_err("missing plugin callback must not be silently discarded");
        assert!(error.to_string().contains("missing_plugin::twice"));
    }
}
