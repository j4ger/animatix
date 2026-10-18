//! Expression evaluator for the animation timeline.
//!
//! Uses an [`Environment`] (`Rc<RefCell<HashMap>>`) for shared mutable scope.
//! Built-ins (sin, cos, lerp, rand, format) resolve through the environment.
//! Closures evaluate against a clone of the caller environment with parameter bindings added.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::ast::{Expr, Time};
use crate::timeline::env::{CapturedEnv, Environment, EvalError, Value};

// Thread-local expression evaluation cache.
//
// Caches `(expr_hash, env_stamp) → Value` to avoid re-evaluating the same
// expression in the same environment during build. The stamp is O(1)
// (instance id + mutation counter, see `EnvStamp`), so cache lookups no
// longer hash the whole environment. The cache is cleared between builds via
// [`clear_eval_cache`] and capped so long-running GUI sessions cannot grow
// without bound.
thread_local! {
    static EVAL_CACHE: RefCell<HashMap<(u64, super::env::EnvStamp, u64), Value>> =
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
            } => super::fn_eval::evaluate_user_fn(&name, &params, &body, &arg_values, env),
            Value::Closure(params, body, ref captures) => {
                invoke_closure_value(&params, &body, captures, arg_values, func, env)
            },
            _ => Err(EvalError::NotCallable(func.to_string())),
        };
    }

    // 2. Known builtins go through eval_shared
    match super::eval_shared::eval_builtin_fn(func, &arg_values) {
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

/// Format a single Value into its display string.
fn format_value(value: &Value) -> String {
    match value {
        Value::PropRef { label, prop } => {
            format!("&{label}.{prop} (not a value; read {label}.{prop})")
        },
        Value::Num(n) => {
            if *n == n.floor() {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        },
        Value::Str(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Vec2(t) => format!("({}, {})", t[0], t[1]),
        Value::Vec3(t) => format!("({}, {}, {})", t[0], t[1], t[2]),
        Value::Vec4(t) => format!("({}, {}, {}, {})", t[0], t[1], t[2], t[3]),
        Value::Color(c) => format!("rgba({}, {}, {}, {})", c[0], c[1], c[2], c[3]),
        Value::List(items) => format!("{:?}", items),
        Value::Object(name, fields) => format!("{}({:?})", name, fields),
        Value::NativeFn(_) => "<NativeFn>".to_string(),
        Value::Closure(args, _, _) => format!("<Closure({:?})>", args),
        Value::UserFn { name, .. } => format!("<UserFn({})>", name),
    }
}

/// Parse a color expression into an `[r, g, b, a]` array.
pub fn parse_color(expr: &Expr) -> [f32; 4] {
    parse_color_in_env(expr, &Environment::new())
}

fn named_color(name: &str) -> Option<[f32; 4]> {
    animatix_syntax::typing::named_color_rgba(name).map(|c| c.map(|v| v as f32))
}

fn color_from_value(value: Value) -> Option<[f32; 4]> {
    // All-numeric brace lists of length 3/4 are accepted alongside
    // Vec3/Vec4/Color so `color = {1, 0, 0, 1}` works like the paren form;
    // heterogeneous or wrong-arity lists stay None (fallback color) — the
    // position/size paths warn on those instead.
    fn all_nums(items: &[Value]) -> Option<Vec<f64>> {
        items
            .iter()
            .map(|item| match item {
                Value::Num(n) => Some(*n),
                _ => None,
            })
            .collect()
    }
    match value {
        Value::Color([r, g, b, a]) => Some([r as f32, g as f32, b as f32, a as f32]),
        Value::Vec4([r, g, b, a]) => Some([r as f32, g as f32, b as f32, a as f32]),
        Value::Vec3([r, g, b]) => Some([r as f32, g as f32, b as f32, 1.0]),
        Value::List(items) if items.len() == 4 => {
            all_nums(&items).map(|n| [n[0] as f32, n[1] as f32, n[2] as f32, n[3] as f32])
        },
        Value::List(items) if items.len() == 3 => {
            all_nums(&items).map(|n| [n[0] as f32, n[1] as f32, n[2] as f32, 1.0])
        },
        Value::Str(text) => color_from_text(&text),
        _ => None,
    }
}

/// Resolve a color written as text: a CSS-style hex literal (`#rgb`, `#rgba`,
/// `#rrggbb`, `#rrggbbaa`) or one of the built-in color names.
///
/// Both forms are accepted anywhere a color is, so `color: "#ff2d55"` and the
/// stops of `fill_gradient: linear(90, {"#ff2d55", "#5e5ce6"})` read the same
/// way they do on the web.
pub fn color_from_text(text: &str) -> Option<[f32; 4]> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let digits: Vec<u32> = hex.chars().map(|c| c.to_digit(16)).collect::<Option<_>>()?;
        let bytes: Vec<u32> = match digits.len() {
            3 | 4 => digits.iter().map(|n| n * 17).collect(),
            6 | 8 => digits.chunks(2).map(|c| c[0] * 16 + c[1]).collect(),
            _ => return None,
        };
        let scale = |v: u32| v as f32 / 255.0;
        return Some([
            scale(bytes[0]),
            scale(bytes[1]),
            scale(bytes[2]),
            bytes.get(3).map_or(1.0, |&a| scale(a)),
        ]);
    }
    named_color(text)
}

/// Resolve a color expression in the given environment, returning `None` if not a color.
pub fn resolve_color_in_env(expr: &Expr, env: &Environment) -> Result<Option<[f32; 4]>, EvalError> {
    if let Expr::Ident(name) = expr
        && let Some(color) = named_color(name)
    {
        return Ok(Some(color));
    }

    evaluate_expr(expr, env).map(color_from_value)
}

/// Parse a color expression in the given environment, falling back to a default gray.
pub fn parse_color_in_env(expr: &Expr, env: &Environment) -> [f32; 4] {
    resolve_color_in_env(expr, env).ok().flatten().unwrap_or([0.8, 0.8, 0.8, 1.0])
}

/// Convert a `Time` value to milliseconds.
///
/// A beat stamp resolves at the default tempo; a build that knows the scene's
/// `config { bpm: … }` uses [`time_to_ms_at_beat`] instead.
pub fn time_to_ms(time: &Time) -> f64 {
    time_to_ms_at_beat(time, crate::ast::beat_seconds(crate::ast::DEFAULT_BPM) * 1000.0)
}

/// Convert a `Time` to milliseconds with `beat_ms` as the length of one beat.
pub fn time_to_ms_at_beat(time: &Time, beat_ms: f64) -> f64 {
    match time {
        Time::Seconds(s) => *s * 1000.0,
        Time::Milliseconds(ms) => *ms as f64,
        Time::Beats(b) => *b * beat_ms,
        Time::SpeechMarker { offset_ms, .. } | Time::SpeechWord { offset_ms, .. } => {
            *offset_ms as f64
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::ast::BinaryOp;
    use crate::timeline::load_standard_library;
    use crate::timeline::modifier_runtime::ir::{CompiledExpr, compile_expr};
    use crate::timeline::property_track::TrackAccessor;

    fn compiled_body(body: Expr) -> Box<CompiledExpr> {
        Box::new(compile_expr(&body).expect("test closure body should compile"))
    }

    #[test]
    fn eval_cache_reflects_env_mutations() {
        // Same env instance, unmutated → cached value is reused.
        let mut env = Environment::new();
        env.set("x", Value::Num(2.0));
        let expr = Expr::Call("sin".to_string(), vec![Expr::Ident("x".to_string())]);
        let first = evaluate_expr(&expr, &env).expect("eval should succeed");

        // Mutating the environment must invalidate memoized values even
        // though the (expr, instance) pair is unchanged.
        env.set("x", Value::Num(0.0));
        let second = evaluate_expr(&expr, &env).expect("eval should succeed");
        assert_eq!(first.as_num(), 2.0f64.sin());
        assert_eq!(second.as_num(), 0.0f64.sin());

        // A clone shares contents but not cache identity: mutating one side
        // must never corrupt the other's memoized values.
        let mut clone = env.clone();
        clone.set("x", Value::Num(std::f64::consts::FRAC_PI_2));
        let from_clone = evaluate_expr(&expr, &clone).expect("eval should succeed");
        let from_original = evaluate_expr(&expr, &env).expect("eval should succeed");
        assert_eq!(from_clone.as_num(), 1.0);
        assert_eq!(from_original.as_num(), 0.0f64.sin());
    }

    #[test]
    fn direct_overrides_removal_invalidates_cache() {
        // Code paths that mutate `overrides` directly (e.g. loop-variable
        // cleanup) must bump the stamp via mark_mutated.
        let mut env = Environment::new();
        env.set("loop_var", Value::Num(3.0));
        let expr = Expr::Binary(
            Box::new(Expr::Ident("loop_var".to_string())),
            BinaryOp::Add,
            Box::new(Expr::Num(1.0)),
        );
        let with_var = evaluate_expr(&expr, &env).expect("eval should succeed");
        assert_eq!(with_var.as_num(), 4.0);

        env.overrides.remove("loop_var");
        env.mark_mutated();
        let without_var = evaluate_expr(&expr, &env);
        assert!(without_var.is_err(), "removed variable must no longer resolve");
    }

    #[test]
    fn test_evaluate_closure() {
        // Use an env with a (empty) base to satisfy the stdlib-base invariant
        // checked by the debug_assert in evaluate_call.
        let mut env = Environment::with_base(std::sync::Arc::new(std::collections::HashMap::new()));
        let closure = Value::Closure(
            vec!["x".to_string()],
            compiled_body(Expr::Binary(
                Box::new(Expr::Ident("x".to_string())),
                BinaryOp::Mul,
                Box::new(Expr::Num(2.0)),
            )),
            CapturedEnv::default(),
        );
        env.set("f", closure);

        let call_expr = Expr::Call("f".to_string(), vec![Expr::Num(4.0)]);
        let result = evaluate_expr(&call_expr, &env).expect("Evaluation failed");

        assert_eq!(result, Value::Num(8.0));
    }

    #[test]
    fn test_evaluate_method_string_length() {
        let mut env = Environment::new();
        env.set("text", Value::Str("hello".to_string()));
        let expr =
            Expr::Method(Box::new(Expr::Ident("text".to_string())), "length".to_string(), vec![]);

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 5.0);
    }

    #[test]
    fn test_evaluate_method_string_split() {
        let mut env = Environment::new();
        env.set("text", Value::Str("a,b,c".to_string()));
        let expr = Expr::Method(
            Box::new(Expr::Ident("text".to_string())),
            "split".to_string(),
            vec![Expr::Str(",".to_string())],
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        let list = result.as_list();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].as_str(), "a");
        assert_eq!(list[1].as_str(), "b");
        assert_eq!(list[2].as_str(), "c");
    }

    #[test]
    fn test_evaluate_method_list_length() {
        let mut env = Environment::new();
        env.set("items", Value::List(vec![Value::Num(1.0), Value::Num(2.0)].into()));
        let expr =
            Expr::Method(Box::new(Expr::Ident("items".to_string())), "length".to_string(), vec![]);

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 2.0);
    }

    #[test]
    fn test_evaluate_method_list_get() {
        let mut env = Environment::new();
        env.set(
            "items",
            Value::List(vec![Value::Num(10.0), Value::Num(20.0), Value::Num(30.0)].into()),
        );
        let expr = Expr::Method(
            Box::new(Expr::Ident("items".to_string())),
            "get".to_string(),
            vec![Expr::Num(2.0)],
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 30.0);
    }

    #[test]
    fn test_evaluate_object_field_access() {
        let mut env = Environment::new();
        let mut fields = HashMap::new();
        fields.insert("x".to_string(), Value::Num(10.0));
        fields.insert("y".to_string(), Value::Num(20.0));
        env.set("point", Value::Object("Point".to_string(), fields));

        let expr =
            Expr::Method(Box::new(Expr::Ident("point".to_string())), "x".to_string(), vec![]);

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 10.0);
    }

    #[test]
    fn test_evaluate_object_field_missing() {
        let mut env = Environment::new();
        let fields = HashMap::new();
        env.set("point", Value::Object("Point".to_string(), fields));

        let expr =
            Expr::Method(Box::new(Expr::Ident("point".to_string())), "z".to_string(), vec![]);

        let result = evaluate_expr(&expr, &env);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("z"));
    }

    #[test]
    fn test_evaluate_method_num_abs() {
        let mut env = Environment::new();
        env.set("x", Value::Num(-42.5));
        let expr = Expr::Method(Box::new(Expr::Ident("x".to_string())), "abs".to_string(), vec![]);

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 42.5);
    }

    #[test]
    fn test_evaluate_method_unsupported() {
        let env = Environment::new();
        let expr =
            Expr::Method(Box::new(Expr::Ident("graph".to_string())), "plot".to_string(), vec![]);

        let result = evaluate_expr(&expr, &env);
        assert!(result.is_err());
    }

    #[test]
    fn test_evaluate_index_on_list() {
        let mut env = Environment::new();
        env.set(
            "items",
            Value::List(vec![Value::Num(10.0), Value::Num(20.0), Value::Num(30.0)].into()),
        );
        let expr =
            Expr::Index(Box::new(Expr::Ident("items".to_string())), Box::new(Expr::Num(1.0)));

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 20.0);
    }

    #[test]
    fn test_evaluate_index_on_vec2() {
        let mut env = Environment::new();
        env.set("pos", Value::Vec2([100.0, 200.0]));
        let expr = Expr::Index(Box::new(Expr::Ident("pos".to_string())), Box::new(Expr::Num(0.0)));

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 100.0);
    }

    #[test]
    fn test_evaluate_index_on_string() {
        let mut env = Environment::new();
        env.set("text", Value::Str("hello".to_string()));
        let expr = Expr::Index(Box::new(Expr::Ident("text".to_string())), Box::new(Expr::Num(1.0)));

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_str(), "e");
    }

    #[test]
    fn test_evaluate_index_out_of_bounds() {
        let mut env = Environment::new();
        env.set("items", Value::List(vec![Value::Num(10.0)].into()));
        let expr =
            Expr::Index(Box::new(Expr::Ident("items".to_string())), Box::new(Expr::Num(5.0)));

        let result = evaluate_expr(&expr, &env);
        assert!(result.is_err());
    }

    #[test]
    fn test_evaluate_construct_creates_object() {
        let env = Environment::new();
        let expr = Expr::Construct(
            "Point".to_string(),
            vec![
                crate::ast::Property {
                    name: "x".to_string(),
                    value: Expr::Num(10.0),
                    value_span: None,
                    trailing_comment: None,
                },
                crate::ast::Property {
                    name: "y".to_string(),
                    value: Expr::Num(20.0),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        match result {
            Value::Object(name, fields) => {
                assert_eq!(name, "Point");
                assert_eq!(fields.get("x").unwrap().as_num(), 10.0);
                assert_eq!(fields.get("y").unwrap().as_num(), 20.0);
            },
            other => panic!("Expected Object, got: {:?}", other),
        }
    }

    #[test]
    fn test_evaluate_closure_captures_variable_at_creation_time() {
        // Use an env with a (empty) base to satisfy the stdlib-base invariant
        // checked by the debug_assert in evaluate_call.
        let mut env = Environment::with_base(std::sync::Arc::new(std::collections::HashMap::new()));
        env.set("y", Value::Num(3.0));
        let closure = Value::Closure(
            vec!["x".to_string()],
            compiled_body(Expr::Binary(
                Box::new(Expr::Ident("x".to_string())),
                BinaryOp::Add,
                Box::new(Expr::Ident("y".to_string())),
            )),
            CapturedEnv(HashMap::from([("y".to_string(), Value::Num(3.0))])),
        );
        env.set("f", closure);
        // y is changed in the environment after closure creation,
        // but the closure captured y=3 at creation time.
        env.set("y", Value::Num(10.0));

        let call_expr = Expr::Call("f".to_string(), vec![Expr::Num(4.0)]);
        let result = evaluate_expr(&call_expr, &env).expect("Evaluation failed");

        // With capture semantics, y=3 (captured at creation time)
        assert_eq!(result, Value::Num(7.0));
    }

    #[test]
    fn test_evaluate_path_uses_flat_dotted_lookup_key() {
        let mut env = Environment::new();
        env.set("node.at.x", Value::Num(320.0));

        let expr = Expr::Path(vec!["node".to_string(), "at".to_string(), "x".to_string()]);
        let result = evaluate_expr(&expr, &env).expect("path lookup should succeed");

        assert_eq!(result, Value::Num(320.0));
    }

    #[test]
    fn test_time_to_ms() {
        assert_eq!(time_to_ms(&Time::Seconds(2.5)), 2500.0);
        assert_eq!(time_to_ms(&Time::Milliseconds(500)), 500.0);
    }

    #[test]
    fn test_parse_color() {
        assert_eq!(parse_color(&Expr::Ident("red".to_string())), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(parse_color(&Expr::Ident("unknown".to_string())), [0.8, 0.8, 0.8, 1.0]);
        assert_eq!(parse_color(&Expr::Num(1.0)), [0.8, 0.8, 0.8, 1.0]);
    }

    #[test]
    fn test_bool_truthiness_in_conditionals_and_logic() {
        let env = Environment::new();

        let cond_true = Expr::Conditional(
            Box::new(Expr::Bool(true)),
            Box::new(Expr::Num(1.0)),
            Box::new(Expr::Num(0.0)),
        );
        assert_eq!(evaluate_expr(&cond_true, &env).unwrap(), Value::Num(1.0));

        let cond_false = Expr::Conditional(
            Box::new(Expr::Bool(false)),
            Box::new(Expr::Num(1.0)),
            Box::new(Expr::Num(0.0)),
        );
        assert_eq!(evaluate_expr(&cond_false, &env).unwrap(), Value::Num(0.0));

        let not_true = Expr::Unary(crate::ast::UnaryOp::Not, Box::new(Expr::Bool(true)));
        assert_eq!(evaluate_expr(&not_true, &env).unwrap(), Value::Num(0.0));

        let and_expr =
            Expr::Binary(Box::new(Expr::Bool(true)), BinaryOp::And, Box::new(Expr::Bool(false)));
        assert_eq!(evaluate_expr(&and_expr, &env).unwrap(), Value::Num(0.0));

        let or_expr =
            Expr::Binary(Box::new(Expr::Bool(true)), BinaryOp::Or, Box::new(Expr::Bool(false)));
        assert_eq!(evaluate_expr(&or_expr, &env).unwrap(), Value::Num(1.0));
    }

    #[test]
    fn test_evaluate_expr_sin_cos() {
        let mut env = Environment::new();
        load_standard_library(&mut env);
        // sin(0) = 0
        let result = evaluate_expr(&Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]), &env)
            .unwrap_or(Value::Num(0.0));
        assert!({
            let v = result.as_num();
            v.abs() < 1e-10
        });

        // sin(PI/2) ≈ 1
        let result = evaluate_expr(
            &Expr::Call("sin".to_string(), vec![Expr::Num(std::f64::consts::FRAC_PI_2)]),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert!((result.as_num() - 1.0).abs() < 1e-10);

        // cos(0) = 1
        let result = evaluate_expr(&Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]), &env)
            .unwrap_or(Value::Num(0.0));
        assert!((result.as_num() - 1.0).abs() < 1e-10);

        // cos(PI) ≈ -1
        let result = evaluate_expr(
            &Expr::Call("cos".to_string(), vec![Expr::Num(std::f64::consts::PI)]),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert!((result.as_num() + 1.0).abs() < 1e-10);

        // sin nested: sin(PI/6) * 2
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Call(
                    "sin".to_string(),
                    vec![Expr::Num(std::f64::consts::FRAC_PI_6)],
                )),
                BinaryOp::Mul,
                Box::new(Expr::Num(2.0)),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert!((result.as_num() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_evaluate_expr_format() {
        let mut env = Environment::new();
        load_standard_library(&mut env);
        // format("value: {}", 42)
        let result = evaluate_expr(
            &Expr::Call(
                "format".to_string(),
                vec![Expr::Str("value: {}".to_string()), Expr::Num(42.0)],
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_str(), "value: 42");

        // format("x={}, y={}", 10, 20)
        let result = evaluate_expr(
            &Expr::Call(
                "format".to_string(),
                vec![
                    Expr::Str("x={}, y={}".to_string()),
                    Expr::Num(10.0),
                    Expr::Num(20.0),
                ],
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_str(), "x=10, y=20");

        // format with no args
        let result = evaluate_expr(&Expr::Call("format".to_string(), vec![]), &env)
            .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_str(), "");

        // format with text and sin
        let result = evaluate_expr(
            &Expr::Call(
                "format".to_string(),
                vec![
                    Expr::Str("sin(π/2) = {}".to_string()),
                    Expr::Call("sin".to_string(), vec![Expr::Num(std::f64::consts::FRAC_PI_2)]),
                ],
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_str(), "sin(π/2) = 1");
    }

    #[test]
    fn test_evaluate_expr_path_uses_dotted_environment_lookup() {
        let mut env = Environment::new();
        env.set("left.badge.color", Value::Num(7.0));

        let result = evaluate_expr(
            &Expr::Path(vec!["left".to_string(), "badge".to_string(), "color".to_string()]),
            &env,
        )
        .expect("path lookup should resolve from dotted environment key");

        assert_eq!(result.as_num(), 7.0);
    }

    #[test]
    fn test_evaluate_expr_constants() {
        let mut env = Environment::new();
        load_standard_library(&mut env);
        assert!(
            (evaluate_expr(&Expr::Ident("PI".to_string()), &env)
                .unwrap_or(Value::Num(0.0))
                .as_num()
                - std::f64::consts::PI)
                .abs()
                < 1e-10
        );
        assert!(
            (evaluate_expr(&Expr::Ident("TAU".to_string()), &env)
                .unwrap_or(Value::Num(0.0))
                .as_num()
                - std::f64::consts::TAU)
                .abs()
                < 1e-10
        );
    }

    #[test]
    fn test_evaluate_expr_tuple() {
        let mut env = Environment::new();
        load_standard_library(&mut env);
        let result = evaluate_expr(&Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]), &env)
            .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_vec2(), [100.0, 200.0]);

        // Tuple with call expressions
        let result = evaluate_expr(
            &Expr::Tuple(vec![
                Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]),
                Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]),
            ]),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_vec2(), [0.0, 1.0]);
    }

    #[test]
    fn test_evaluate_expr_modulo() {
        let mut env = Environment::new();
        load_standard_library(&mut env);

        // Basic modulo: 10 % 3 = 1
        let result = evaluate_expr(
            &Expr::Binary(Box::new(Expr::Num(10.0)), BinaryOp::Mod, Box::new(Expr::Num(3.0))),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert!((result.as_num() - 1.0).abs() < 1e-10);

        // Modulo with division: 7 % 2 = 1
        let result = evaluate_expr(
            &Expr::Binary(Box::new(Expr::Num(7.0)), BinaryOp::Mod, Box::new(Expr::Num(2.0))),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert!((result.as_num() - 1.0).abs() < 1e-10);

        // Modulo with sin result: sin(PI/2) % 2 = 1 % 2 = 1
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Call(
                    "sin".to_string(),
                    vec![Expr::Num(std::f64::consts::FRAC_PI_2)],
                )),
                BinaryOp::Mod,
                Box::new(Expr::Num(2.0)),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert!((result.as_num() - 1.0).abs() < 1e-10);

        // Nested modulo: (10 % 3) % 2 = 1 % 2 = 1
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Binary(
                    Box::new(Expr::Num(10.0)),
                    BinaryOp::Mod,
                    Box::new(Expr::Num(3.0)),
                )),
                BinaryOp::Mod,
                Box::new(Expr::Num(2.0)),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert!((result.as_num() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_evaluate_expr_vec2_operations() {
        let mut env = Environment::new();
        load_standard_library(&mut env);

        // Vec2 addition: (10, 20) + (5, 5) = (15, 25)
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
                BinaryOp::Add,
                Box::new(Expr::Tuple(vec![Expr::Num(5.0), Expr::Num(5.0)])),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_vec2(), [15.0, 25.0]);

        // Vec2 subtraction: (10, 20) - (3, 8) = (7, 12)
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
                BinaryOp::Sub,
                Box::new(Expr::Tuple(vec![Expr::Num(3.0), Expr::Num(8.0)])),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_vec2(), [7.0, 12.0]);

        // Vec2 multiplication: (10, 20) * (2, 3) = (20, 60)
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
                BinaryOp::Mul,
                Box::new(Expr::Tuple(vec![Expr::Num(2.0), Expr::Num(3.0)])),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_vec2(), [20.0, 60.0]);

        // Vec2 division: (10, 20) / (2, 4) = (5, 5)
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
                BinaryOp::Div,
                Box::new(Expr::Tuple(vec![Expr::Num(2.0), Expr::Num(4.0)])),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_vec2(), [5.0, 5.0]);

        // Vec2 modulo: (10, 21) % (3, 4) = (1, 1)
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(21.0)])),
                BinaryOp::Mod,
                Box::new(Expr::Tuple(vec![Expr::Num(3.0), Expr::Num(4.0)])),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_vec2(), [1.0, 1.0]);

        // Scalar-Vec2 multiplication: 2 * (10, 20) = (20, 40)
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Num(2.0)),
                BinaryOp::Mul,
                Box::new(Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)])),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_vec2(), [20.0, 40.0]);

        // Vec2 with sin/cos: (sin(0), cos(0)) = (0, 1)
        let result = evaluate_expr(
            &Expr::Tuple(vec![
                Expr::Call("sin".to_string(), vec![Expr::Num(0.0)]),
                Expr::Call("cos".to_string(), vec![Expr::Num(0.0)]),
            ]),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_vec2(), [0.0, 1.0]);
    }

    #[test]
    fn test_evaluate_expr_vec3_operations() {
        let mut env = Environment::new();
        load_standard_library(&mut env);

        // Vec3 addition: (1, 2, 3) + (4, 5, 6) = (5, 7, 9)
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)])),
                BinaryOp::Add,
                Box::new(Expr::Tuple(vec![Expr::Num(4.0), Expr::Num(5.0), Expr::Num(6.0)])),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_vec3(), [5.0, 7.0, 9.0]);

        // Vec3 scalar multiplication: 2 * (1, 2, 3) = (2, 4, 6)
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Num(2.0)),
                BinaryOp::Mul,
                Box::new(Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)])),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert_eq!(result.as_vec3(), [2.0, 4.0, 6.0]);
    }

    #[test]
    fn test_evaluate_expr_color_operations() {
        let mut env = Environment::new();
        load_standard_library(&mut env);

        // Color addition: RED + GREEN = (1, 1, 0, 2)
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Ident("RED".to_string())),
                BinaryOp::Add,
                Box::new(Expr::Ident("GREEN".to_string())),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        let color = result.as_color();
        assert!((color[0] - 1.0).abs() < 1e-10);
        assert!((color[1] - 1.0).abs() < 1e-10);
        assert!((color[2] - 0.0).abs() < 1e-10);
        assert!((color[3] - 2.0).abs() < 1e-10);

        // Color scalar multiplication: 0.5 * BLUE = (0, 0, 0.5, 0.5)
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Num(0.5)),
                BinaryOp::Mul,
                Box::new(Expr::Ident("BLUE".to_string())),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        let color = result.as_color();
        assert!((color[0] - 0.0).abs() < 1e-10);
        assert!((color[1] - 0.0).abs() < 1e-10);
        assert!((color[2] - 0.5).abs() < 1e-10);
        assert!((color[3] - 0.5).abs() < 1e-10);

        // Color subtraction: WHITE - RED = (0, 1, 1, 0) - alpha fades out
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Ident("WHITE".to_string())),
                BinaryOp::Sub,
                Box::new(Expr::Ident("RED".to_string())),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        let color = result.as_color();
        assert!((color[0] - 0.0).abs() < 1e-10);
        assert!((color[1] - 1.0).abs() < 1e-10);
        assert!((color[2] - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_evaluate_expr_rand() {
        let mut env = Environment::new();
        load_standard_library(&mut env);

        // rand() should return a value between 0 and 1
        let result =
            evaluate_expr(&Expr::Call("rand".to_string(), vec![]), &env).unwrap_or(Value::Num(0.0));
        let val = result.as_num();
        assert!((0.0..1.0).contains(&val), "rand() should return value in [0, 1), got {}", val);

        // rand() with expressions: rand() * 100 should be in [0, 100)
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Call("rand".to_string(), vec![])),
                BinaryOp::Mul,
                Box::new(Expr::Num(100.0)),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        let val = result.as_num();
        assert!((0.0..100.0).contains(&val), "rand() * 100 should be in [0, 100), got {}", val);
    }

    #[test]
    fn seeded_rand_is_deterministic() {
        let source = r#"
            c: Ellipse, radius: 50, color: red
            always {
                c.position = (seeded_rand(1.0) * 100, seeded_rand(2.0) * 100)
            }
        "#;
        let ast = animatix_syntax::parser::parse_simple(source).0.unwrap();
        let timeline = super::super::Timeline::build(&ast);
        let pos1 = timeline.tracks.get("c").unwrap().geometry.position.get(0, [0.0, 0.0]);

        // Rebuild and re-evaluate — same seed must produce same value
        let timeline2 = super::super::Timeline::build(&ast);
        let pos2 = timeline2.tracks.get("c").unwrap().geometry.position.get(0, [0.0, 0.0]);
        assert_eq!(pos1, pos2, "seeded_rand must be deterministic for the same seed");
    }

    #[test]
    fn seeded_rand_returns_value_in_range() {
        let source = r#"
            c: Ellipse, radius: 50, color: red
            always {
                c.position = (seeded_rand(42.0) * 100, seeded_rand(42.0) * 100)
            }
        "#;
        let ast = animatix_syntax::parser::parse_simple(source).0.unwrap();
        let timeline = super::super::Timeline::build(&ast);
        let pos = timeline.tracks.get("c").unwrap().geometry.position.get(0, [0.0, 0.0]);
        assert!(
            pos[0] >= 0.0 && pos[0] <= 100.0,
            "seeded_rand should return value in [0,1] range, got {}",
            pos[0]
        );
        assert!(
            pos[1] >= 0.0 && pos[1] <= 100.0,
            "seeded_rand should return value in [0,1] range, got {}",
            pos[1]
        );
    }

    #[test]
    fn seeded_rand_different_seeds_produce_different_values() {
        let source = r#"
            c: Ellipse, radius: 50, color: red
        "#;
        let ast = animatix_syntax::parser::parse_simple(source).0.unwrap();
        let timeline = super::super::Timeline::build(&ast);

        // Evaluate seeded_rand with different seeds directly in the timeline's env
        let expr1 = Expr::Call("seeded_rand".to_string(), vec![Expr::Num(1.0)]);
        let expr2 = Expr::Call("seeded_rand".to_string(), vec![Expr::Num(2.0)]);

        let val1 = evaluate_expr(&expr1, &timeline.env).unwrap();
        let val2 = evaluate_expr(&expr2, &timeline.env).unwrap();

        let n1 = match val1 {
            Value::Num(n) => n,
            _ => panic!("expected num"),
        };
        let n2 = match val2 {
            Value::Num(n) => n,
            _ => panic!("expected num"),
        };

        assert_ne!(n1, n2, "different seeds should produce different values");
    }

    #[test]
    fn test_object_field_read_via_path_walk() {
        // Directly set up an Object in the env and read a field via path walk
        let mut env = Environment::new();
        let mut fields = std::collections::HashMap::new();
        fields.insert("x".to_string(), Value::Num(10.0));
        fields.insert("y".to_string(), Value::Num(20.0));
        env.set("p", Value::Object("Point".to_string(), fields));

        // Read p.x via path walk (no dotted sub-key in env)
        let expr = Expr::Path(vec!["p".to_string(), "x".to_string()]);
        let result = evaluate_expr(&expr, &env).expect("path walk should succeed");
        assert_eq!(result, Value::Num(10.0));

        // Read p.y
        let expr = Expr::Path(vec!["p".to_string(), "y".to_string()]);
        let result = evaluate_expr(&expr, &env).expect("path walk should succeed");
        assert_eq!(result, Value::Num(20.0));
    }

    #[test]
    fn test_object_field_read_nonexistent_field() {
        let mut env = Environment::new();
        let mut fields = std::collections::HashMap::new();
        fields.insert("x".to_string(), Value::Num(10.0));
        env.set("p", Value::Object("Point".to_string(), fields));

        // Read p.z — field doesn't exist
        let expr = Expr::Path(vec!["p".to_string(), "z".to_string()]);
        let result = evaluate_expr(&expr, &env);
        assert!(result.is_err(), "expected error for nonexistent field");
        if let Err(EvalError::UndefinedVariable(path)) = result {
            assert_eq!(path, "p.z", "error should reference full path");
        } else {
            panic!("expected UndefinedVariable error");
        }
    }

    #[test]
    fn test_object_field_read_non_object_intermediate() {
        let mut env = Environment::new();
        env.set("x", Value::Num(42.0));

        // x.y — x is a number, not an object
        let expr = Expr::Path(vec!["x".to_string(), "y".to_string()]);
        let result = evaluate_expr(&expr, &env);
        assert!(result.is_err(), "expected error for non-object intermediate");
    }

    #[test]
    fn test_path_dotted_backward_compat() {
        // Direct dotted lookup must still work for injected sub-keys
        let mut env = Environment::new();
        env.set("node.at.x", Value::Num(320.0));

        let expr = Expr::Path(vec!["node".to_string(), "at".to_string(), "x".to_string()]);
        let result = evaluate_expr(&expr, &env).expect("path lookup should succeed");
        assert_eq!(result, Value::Num(320.0));
    }
}
