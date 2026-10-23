//! Expression evaluation: the evaluation cache, expression hashing, and the
//! call/closure/method dispatch used by the IR VM.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::ast::Expr;
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
    /// Flag to disable the eval cache for tight sampling loops.
    /// When false, evaluate_expr skips hashing and the cache entirely.
    static EVAL_CACHE_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

const EVAL_CACHE_LIMIT: usize = 4096;

/// Clear the thread-local expression evaluation cache.
/// Call this at the start of each build to avoid stale results.
pub fn clear_eval_cache() {
    EVAL_CACHE.with(|cache| cache.borrow_mut().clear());
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

/// Hash an expression tree for cache keying.
fn expr_hash(expr: &Expr) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hash_expr_recursive(expr, &mut hasher);
    hasher.finish()
}

fn hash_expr_recursive<V: Hasher>(expr: &Expr, hasher: &mut V) {
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
            hash_expr_recursive(a, hasher);
            hash_expr_recursive(b, hasher);
        },
        Expr::Tuple(items) => {
            8u8.hash(hasher);
            items.len().hash(hasher);
            for e in items {
                hash_expr_recursive(e, hasher);
            }
        },
        Expr::List(items) => {
            9u8.hash(hasher);
            items.len().hash(hasher);
            for e in items {
                hash_expr_recursive(e, hasher);
            }
        },
        Expr::Binary(a, op, b) => {
            10u8.hash(hasher);
            hash_expr_recursive(a, hasher);
            format!("{:?}", op).hash(hasher);
            hash_expr_recursive(b, hasher);
        },
        Expr::Unary(op, e) => {
            11u8.hash(hasher);
            format!("{:?}", op).hash(hasher);
            hash_expr_recursive(e, hasher);
        },
        Expr::Call(name, args) => {
            12u8.hash(hasher);
            name.hash(hasher);
            args.len().hash(hasher);
            for a in args {
                hash_expr_recursive(a, hasher);
            }
        },
        Expr::Method(recv, name, args) => {
            13u8.hash(hasher);
            hash_expr_recursive(recv, hasher);
            name.hash(hasher);
            args.len().hash(hasher);
            for a in args {
                hash_expr_recursive(a, hasher);
            }
        },
        Expr::Closure(params, body) => {
            14u8.hash(hasher);
            params.hash(hasher);
            hash_expr_recursive(body, hasher);
        },
        Expr::LetChain(bindings, tail) => {
            20u8.hash(hasher);
            for (name, value) in bindings {
                name.hash(hasher);
                hash_expr_recursive(value, hasher);
            }
            hash_expr_recursive(tail, hasher);
        },
        Expr::Conditional(c, t, e) => {
            15u8.hash(hasher);
            hash_expr_recursive(c, hasher);
            hash_expr_recursive(t, hasher);
            hash_expr_recursive(e, hasher);
        },
        Expr::Match(scrutinee, arms) => {
            17u8.hash(hasher);
            hash_expr_recursive(scrutinee, hasher);
            for (_p, e) in arms {
                hash_expr_recursive(e, hasher);
            }
        },
        Expr::Construct(name, _) => {
            16u8.hash(hasher);
            name.hash(hasher);
        },
    }
}

/// True for expressions whose direct evaluation is cheaper than a cache
/// round-trip: literals, single variable lookups, and tuples/lists built
/// entirely from those.
fn expr_is_trivial(expr: &Expr) -> bool {
    match expr {
        Expr::Num(_)
        | Expr::Percent(_)
        | Expr::Str(_)
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
    // Fast path: trivial expressions and disabled caches (e.g., inside tight
    // plot sampling loops) evaluate directly — a cache round-trip would cost
    // more than the evaluation itself.
    if !EVAL_CACHE_ENABLED.get() || expr_is_trivial(expr) {
        return evaluate_expr_inner(expr, env);
    }

    // The let-scope epoch is part of the identity: the same env instance can
    // produce different values for the same expression under different
    // active LetChain scopes.
    let cache_key = (expr_hash(expr), env.stamp(), env.let_epoch());

    // Check cache
    {
        let hit = EVAL_CACHE.with(|cache| cache.borrow().get(&cache_key).cloned());
        if let Some(value) = hit {
            return Ok(value);
        }
    }

    // Evaluate and cache
    let result = evaluate_expr_inner(expr, env)?;
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
fn evaluate_expr_inner(expr: &Expr, env: &Environment) -> Result<Value, EvalError> {
    let compiled = crate::timeline::modifier_runtime::ir::compile_expr(expr)?;
    crate::timeline::modifier_runtime::ir::evaluate_compiled_expr(&compiled, env)
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
    // 1. If an environment binding exists for this function, it takes precedence (e.g. shadowed builtins)
    if let Some(val) = env.get(func) {
        return match val {
            Value::NativeFn(native_func) => native_func(&arg_values, env),
            Value::UserFn {
                name, params, body, ..
            } => {
                crate::timeline::fn_eval::evaluate_user_fn(&name, &params, &body, &arg_values, env)
            },
            Value::Closure(params, body, ref captures) => {
                invoke_closure_value(&params, &body, captures, arg_values, func, env)
            },
            _ => Err(EvalError::NotCallable(func.to_string())),
        };
    }

    // 2. Known builtins go through eval_shared
    match crate::timeline::eval_shared::eval_builtin_fn(func, &arg_values) {
        Ok(val) => return Ok(val),
        Err(EvalError::UndefinedVariable(_)) => {},
        Err(e) => return Err(e),
    }

    // 3. Dotted method call on a value in the environment (e.g. `text.length()` or `items.length()`)
    if let Some((target, method)) = func.rsplit_once('.') {
        if let Some(receiver) = env.get_path(target) {
            eval_method_dispatch(receiver, method, &arg_values, env)
        } else {
            Err(EvalError::UndefinedVariable(func.to_string()))
        }
    } else {
        Err(EvalError::UndefinedVariable(func.to_string()))
    }
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
    captures.merge_into(&mut child_env);
    // Propagate the caller's plot-sampling bindings (e.g. the bound plot
    // argument `x`): they live in binding slots, never in `CapturedEnv`, so
    // a nested closure referencing the outer argument would otherwise
    // resolve to UndefinedVariable. Slots whose name collides with a
    // parameter are skipped — the parameter binding must win.
    for (dst, src) in child_env.bindings.iter_mut().zip(env.bindings.iter()) {
        if let Some((name, _)) = src {
            if params.contains(name) {
                continue;
            }
        }
        *dst = src.clone();
    }
    for (param, val) in params.iter().zip(arg_values) {
        child_env.set(param, val);
    }

    crate::timeline::modifier_runtime::ir::evaluate_compiled_expr(body, &child_env)
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
    if let Value::Object(_type_name, fields) = &receiver {
        if args.is_empty() {
            if let Some(field_value) = fields.get(name) {
                return Ok(field_value.clone());
            }
        }
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
