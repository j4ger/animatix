//! Expression evaluator for the animation timeline.
//!
//! Uses an [`Environment`] (`Rc<RefCell<HashMap>>`) for shared mutable scope.
//! Built-ins (sin, cos, lerp, rand, format) resolve through the environment.
//! Closures evaluate against a clone of the caller environment with parameter bindings added.

use crate::ast::{BinaryOp, Expr, Time};
use crate::timeline::env::{Environment, EvalError, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

// Thread-local expression evaluation cache.
//
// Caches `(expr_ptr, env_hash) → Value` to avoid re-evaluating the same
// expression in the same environment during build. The cache is cleared
// between builds via [`clear_eval_cache`].
thread_local! {
    static EVAL_CACHE: RefCell<HashMap<(usize, u64), Value>> =
        RefCell::new(HashMap::new());
    /// Flag to disable the eval cache for tight sampling loops.
    /// When false, evaluate_expr skips env_hash() and the cache entirely.
    static EVAL_CACHE_ENABLED: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

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

/// Compute a hash of the environment's override entries.
/// Skips NativeFn values (which don't implement Hash).
fn env_hash(env: &Environment) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // Hash the base Arc pointer identity (same base = same stdlib)
    if let Some(ref base) = env.base {
        let ptr = std::sync::Arc::as_ptr(base) as usize;
        ptr.hash(&mut hasher);
    }
    // Hash override entries (skip NativeFn which can't be hashed)
    let mut entries: Vec<(&String, &Value)> = env.overrides.iter()
        .filter(|(_, v)| !matches!(v, Value::NativeFn(_)))
        .collect();
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (key, value) in entries {
        key.hash(&mut hasher);
        hash_value(value, &mut hasher);
    }
    // Hash bindings if present
    for binding in env.bindings.iter().flatten() {
        binding.0.hash(&mut hasher);
        hash_value(&binding.1, &mut hasher);
    }
    hasher.finish()
}

/// Hash a Value (skipping NativeFn which can't be hashed).
fn hash_value<V: Hasher>(value: &Value, hasher: &mut V) {
    match value {
        Value::Num(n) => { 0u8.hash(hasher); n.to_bits().hash(hasher); }
        Value::Str(s) => { 1u8.hash(hasher); s.hash(hasher); }
        Value::Bool(b) => { 2u8.hash(hasher); b.hash(hasher); }
        Value::Vec2(v) => { 3u8.hash(hasher); v[0].to_bits().hash(hasher); v[1].to_bits().hash(hasher); }
        Value::Vec3(v) => { 4u8.hash(hasher); for x in v { x.to_bits().hash(hasher); } }
        Value::Vec4(v) => { 5u8.hash(hasher); for x in v { x.to_bits().hash(hasher); } }
        Value::Color(c) => { 6u8.hash(hasher); for x in c { x.to_bits().hash(hasher); } }
        Value::List(items) => { 7u8.hash(hasher); items.len().hash(hasher); }
        Value::Object(name, _) => { 8u8.hash(hasher); name.hash(hasher); }
        Value::NativeFn(_) => { 9u8.hash(hasher); } // pointer identity
        Value::Closure(params, _, _) => { 10u8.hash(hasher); params.hash(hasher); }
    }
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
        Expr::Num(n) => { 0u8.hash(hasher); n.to_bits().hash(hasher); }
        Expr::Percent(n) => { 1u8.hash(hasher); n.to_bits().hash(hasher); }
        Expr::Str(s) => { 2u8.hash(hasher); s.hash(hasher); }
        Expr::Bool(b) => { 3u8.hash(hasher); b.hash(hasher); }
        Expr::Null => { 4u8.hash(hasher); }
        Expr::Ident(s) => { 5u8.hash(hasher); s.hash(hasher); }
        Expr::Path(parts) => { 6u8.hash(hasher); parts.hash(hasher); }
        Expr::Index(a, b) => { 7u8.hash(hasher); hash_expr_recursive(a, hasher); hash_expr_recursive(b, hasher); }
        Expr::Tuple(items) => { 8u8.hash(hasher); items.len().hash(hasher); for e in items { hash_expr_recursive(e, hasher); } }
        Expr::List(items) => { 9u8.hash(hasher); items.len().hash(hasher); for e in items { hash_expr_recursive(e, hasher); } }
        Expr::Binary(a, op, b) => { 10u8.hash(hasher); hash_expr_recursive(a, hasher); format!("{:?}", op).hash(hasher); hash_expr_recursive(b, hasher); }
        Expr::Unary(op, e) => { 11u8.hash(hasher); format!("{:?}", op).hash(hasher); hash_expr_recursive(e, hasher); }
        Expr::Call(name, args) => { 12u8.hash(hasher); name.hash(hasher); args.len().hash(hasher); for a in args { hash_expr_recursive(a, hasher); } }
        Expr::Method(recv, name, args) => { 13u8.hash(hasher); hash_expr_recursive(recv, hasher); name.hash(hasher); args.len().hash(hasher); for a in args { hash_expr_recursive(a, hasher); } }
        Expr::Closure(params, body) => { 14u8.hash(hasher); params.hash(hasher); hash_expr_recursive(body, hasher); }
        Expr::Conditional(c, t, e) => { 15u8.hash(hasher); hash_expr_recursive(c, hasher); hash_expr_recursive(t, hasher); hash_expr_recursive(e, hasher); }
        Expr::Construct(name, _) => { 16u8.hash(hasher); name.hash(hasher); }
    }
}

/// Represents a runtime value produced by evaluating an expression.
///
/// Uses a thread-local cache keyed on `(expr_content_hash, env_hash)` to
/// avoid re-evaluating the same expression in the same environment during build.
pub fn evaluate_expr(expr: &Expr, env: &Environment) -> Result<Value, EvalError> {
    // Fast path: when cache is disabled (e.g., inside tight plot sampling loops),
    // skip env_hash() and cache entirely
    if !EVAL_CACHE_ENABLED.get() {
        return evaluate_expr_inner(expr, env);
    }

    // Check cache for a hit (only for non-trivial expressions)
    let cache_key = match expr {
        Expr::Num(_) | Expr::Percent(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => {
            // Literals are cheap — evaluate directly without caching
            return evaluate_expr_inner(expr, env);
        }
        _ => {
            let expr_h = expr_hash(expr);
            let env_h = env_hash(env);
            (expr_h as usize, env_h)
        }
    };

    // Check cache
    {
        let hit = EVAL_CACHE.with(|cache| {
            cache.borrow().get(&cache_key).cloned()
        });
        if let Some(value) = hit {
            return Ok(value);
        }
    }

    // Evaluate and cache
    let result = evaluate_expr_inner(expr, env)?;
    EVAL_CACHE.with(|cache| {
        cache.borrow_mut().insert(cache_key, result.clone());
    });
    Ok(result)
}

/// Inner evaluation function (the actual logic, moved from the original evaluate_expr).
fn evaluate_expr_inner(expr: &Expr, env: &Environment) -> Result<Value, EvalError> {
    match expr {
        Expr::Num(n) => Ok(Value::Num(*n)),
        Expr::Percent(n) => Ok(Value::Num(*n / 100.0)),
        Expr::Str(s) => Ok(Value::Str(s.clone())),
        Expr::Bool(b) => Ok(Value::Bool(*b)),
        Expr::Null => Ok(Value::Num(0.0)),

        Expr::Ident(name) => env
            .get(name)
            .ok_or_else(|| EvalError::UndefinedVariable(name.clone())),

        Expr::Tuple(items) => {
            if items.len() == 2 {
                let x = evaluate_expr(&items[0], env)?.as_num();
                let y = evaluate_expr(&items[1], env)?.as_num();
                Ok(Value::Vec2([x, y]))
            } else if items.len() == 3 {
                let x = evaluate_expr(&items[0], env)?.as_num();
                let y = evaluate_expr(&items[1], env)?.as_num();
                let z = evaluate_expr(&items[2], env)?.as_num();
                Ok(Value::Vec3([x, y, z]))
            } else if items.len() == 4 {
                let x = evaluate_expr(&items[0], env)?.as_num();
                let y = evaluate_expr(&items[1], env)?.as_num();
                let z = evaluate_expr(&items[2], env)?.as_num();
                let w = evaluate_expr(&items[3], env)?.as_num();
                Ok(Value::Vec4([x, y, z, w]))
            } else {
                // Arbitrary-length tuples become lists
                let values: Result<Vec<Value>, EvalError> =
                    items.iter().map(|item| evaluate_expr(item, env)).collect();
                Ok(Value::List(values?))
            }
        }

        Expr::List(items) => {
            let vals: Vec<Value> = items.iter().map(|i| evaluate_expr(i, env)).collect::<Result<Vec<Value>, _>>()?;
            Ok(Value::List(vals))
        }
        Expr::Call(func, args) => evaluate_call(func, args, env),

        Expr::Binary(left, op, right) => {
            let l_val = evaluate_expr(left, env)?;
            let r_val = evaluate_expr(right, env)?;

            match (l_val.clone(), r_val.clone()) {
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
                _ => {
                    if *op == BinaryOp::Eq {
                        Ok(Value::Num(if l_val == r_val { 1.0 } else { 0.0 }))
                    } else if *op == BinaryOp::Neq {
                        Ok(Value::Num(if l_val != r_val { 1.0 } else { 0.0 }))
                    } else {
                        Err(EvalError::TypeMismatch(format!(
                            "Unsupported operation {:?} between {:?} and {:?}",
                            op, l_val, r_val
                        )))
                    }
                }
            }
        }

        Expr::Unary(op, inner) => {
            let v = evaluate_expr(inner, env)?.as_num();
            Ok(Value::Num(match op {
                crate::ast::UnaryOp::Neg => -v,
                crate::ast::UnaryOp::Not => {
                    if v == 0.0 {
                        1.0
                    } else {
                        0.0
                    }
                }
            }))
        }

        Expr::Conditional(cond, then_branch, else_branch) => {
            if evaluate_expr(cond, env)?.as_num() != 0.0 {
                evaluate_expr(then_branch, env)
            } else {
                evaluate_expr(else_branch, env)
            }
        }

        // Closures capture the current override environment at creation time (lexical scope).
        Expr::Closure(args, body) => {
            let captures: HashMap<String, Value> = env.overrides.clone();
            Ok(Value::Closure(args.clone(), body.clone(), captures))
        }

        Expr::Path(parts) => {
            let dotted = parts.join(".");
            // First try direct lookup (backward compatible with injected
            // compound sub-keys like "node.position.x").
            if let Some(val) = env.get(&dotted) {
                return Ok(val);
            }
            // Multi-part path: try walking through object fields.
            if parts.len() > 1 {
                let base = env.get(&parts[0])
                    .ok_or(EvalError::UndefinedVariable(parts[0].clone()))?;
                let mut current = base;
                for segment in &parts[1..] {
                    match current {
                        Value::Object(_, fields) => {
                            current = fields.get(segment.as_str())
                                .ok_or_else(|| EvalError::UndefinedVariable(dotted.clone()))?
                                .clone();
                        }
                        _ => return Err(EvalError::UndefinedVariable(dotted)),
                    }
                }
                return Ok(current);
            }
            Err(EvalError::UndefinedVariable(dotted))
        }

        Expr::Method(receiver, name, args) => {
            let receiver_val = evaluate_expr(receiver, env)?;
            evaluate_method(receiver_val, name, args, env)
        }

        Expr::Index(container, index) => {
            let container_val = evaluate_expr(container, env)?;
            let index_val = evaluate_expr(index, env)?;
            let idx = index_val.as_num() as usize;
            match container_val {
                Value::List(items) => items
                    .get(idx)
                    .cloned()
                    .ok_or_else(|| EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for list of length {}",
                        idx,
                        items.len()
                    ))),
                Value::Str(s) => s
                    .chars()
                    .nth(idx)
                    .map(|c| Value::Str(c.to_string()))
                    .ok_or_else(|| EvalError::TypeMismatch(format!(
                        "Index {} out of bounds for string of length {}",
                        idx,
                        s.len()
                    ))),
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

        Expr::Construct(name, properties) => {
            let mut fields = std::collections::HashMap::new();
            for prop in properties {
                let value = evaluate_expr(&prop.value, env)?;
                fields.insert(prop.name.clone(), value);
            }
            Ok(Value::Object(name.clone(), fields))
        }
    }
}

/// Evaluate a function call.
fn evaluate_call(func: &str, args: &[Expr], env: &Environment) -> Result<Value, EvalError> {
    if func == "format" {
        // format("template {}", arg1, arg2)
        if args.is_empty() {
            return Ok(Value::Str(String::new()));
        }
        let template = evaluate_expr(&args[0], env)?.as_str();
        let mut result = String::new();
        let mut placeholder_idx = 0;
        let mut chars = template.chars().peekable();

        let mut arg_values = Vec::new();
        for arg in &args[1..] {
            arg_values.push(evaluate_expr(arg, env)?);
        }

        while let Some(ch) = chars.next() {
            if ch == '{' {
                if chars.peek() == Some(&'}') {
                    chars.next(); // consume '}'
                    if placeholder_idx < arg_values.len() {
                        result.push_str(&format_value(&arg_values[placeholder_idx]));
                    }
                    placeholder_idx += 1;
                } else {
                    result.push(ch);
                }
            } else {
                result.push(ch);
            }
        }
        return Ok(Value::Str(result));
    }

    // Look up the function in the environment
    if let Some(val) = env.get(func) {
        match val {
            Value::NativeFn(native_func) => {
                let mut arg_values = Vec::new();
                for arg in args {
                    arg_values.push(evaluate_expr(arg, env)?);
                }
                native_func(&arg_values, env)
            }
            // Closures evaluate against the captured (lexical) environment,
            // then bind parameters on top. Free variables resolve to their
            // values at creation time, not call time.
            Value::Closure(params, body, ref captures) => {
                if args.len() != params.len() {
                    return Err(EvalError::TypeMismatch(format!(
                        "Closure '{}' expects {} arguments, got {}",
                        func,
                        params.len(),
                        args.len()
                    )));
                }

                let mut arg_values = Vec::new();
                for arg in args {
                    arg_values.push(evaluate_expr(arg, env)?);
                }

                let mut child_env = Environment::new();
                for (k, v) in captures {
                    child_env.set(k, v.clone());
                }
                for (param, val) in params.iter().zip(arg_values) {
                    child_env.set(param, val);
                }

                evaluate_expr(&body, &child_env)
            }
            _ => Err(EvalError::NotCallable(func.to_string())),
        }
    } else {
        Err(EvalError::UndefinedVariable(func.to_string()))
    }
}

/// Evaluate a method call on a receiver value.
/// Shared method dispatch logic that operates on already-evaluated argument values.
/// This is called by both the tree-walker (evaluate_method) and IR/VM (eval_method)
/// to avoid code duplication.
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
            items
                .get(idx)
                .cloned()
                .ok_or_else(|| {
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

/// Evaluate a method call on a receiver value (tree-walker version).
/// Evaluates Expr arguments to Values, then delegates to eval_method_dispatch.
fn evaluate_method(
    receiver: Value,
    name: &str,
    args: &[Expr],
    env: &Environment,
) -> Result<Value, EvalError> {
    // Evaluate arguments first
    let arg_values: Vec<Value> = args
        .iter()
        .map(|arg| evaluate_expr(arg, env))
        .collect::<Result<Vec<_>, _>>()?;

    // Call shared dispatch
    match eval_method_dispatch(receiver.clone(), name, &arg_values, env) {
        Ok(value) => Ok(value),
        Err(EvalError::UnsupportedMethod(msg)) => {
            // Tree-walker has additional Object field access support
            if let Value::Object(_type_name, fields) = &receiver {
                if arg_values.is_empty() {
                    if let Some(field_value) = fields.get(name) {
                        return Ok(field_value.clone());
                    }
                }
            }
            // Not an Object field access, return the original error
            Err(EvalError::UnsupportedMethod(msg))
        }
        Err(e) => Err(e),
    }
}

/// Format a single Value into its display string.
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
        Value::Closure(args, _, _) => format!("<Closure({:?})>", args),
    }
}

/// Parse a color expression into an `[r, g, b, a]` array.
pub fn parse_color(expr: &Expr) -> [f32; 4] {
    parse_color_in_env(expr, &Environment::new())
}

fn named_color(name: &str) -> Option<[f32; 4]> {
    match name {
        "red" | "RED" => Some([1.0, 0.0, 0.0, 1.0]),
        "green" | "GREEN" => Some([0.0, 1.0, 0.0, 1.0]),
        "blue" | "BLUE" => Some([0.0, 0.0, 1.0, 1.0]),
        "black" | "BLACK" => Some([0.0, 0.0, 0.0, 1.0]),
        "white" | "WHITE" => Some([1.0, 1.0, 1.0, 1.0]),
        "yellow" | "YELLOW" => Some([1.0, 1.0, 0.0, 1.0]),
        "orange" | "ORANGE" => Some([1.0, 0.65, 0.0, 1.0]),
        _ => None,
    }
}

fn color_from_value(value: Value) -> Option<[f32; 4]> {
    match value {
        Value::Color([r, g, b, a]) => Some([r as f32, g as f32, b as f32, a as f32]),
        Value::Vec4([r, g, b, a]) => Some([r as f32, g as f32, b as f32, a as f32]),
        Value::Vec3([r, g, b]) => Some([r as f32, g as f32, b as f32, 1.0]),
        _ => None,
    }
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
    resolve_color_in_env(expr, env)
        .ok()
        .flatten()
        .unwrap_or([0.8, 0.8, 0.8, 1.0])
}

/// Convert a `Time` value to milliseconds.
pub fn time_to_ms(time: &Time) -> f64 {
    match time {
        Time::Seconds(s) => *s * 1000.0,
        Time::Milliseconds(ms) => *ms as f64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::BinaryOp;
    use crate::timeline::load_standard_library;
    use crate::timeline::property_track::TrackAccessor;
    use chumsky::Parser;
    use std::collections::HashMap;

    #[test]
    fn test_evaluate_closure() {
        let mut env = Environment::new();
        let closure = Value::Closure(
            vec!["x".to_string()],
            Box::new(Expr::Binary(
                Box::new(Expr::Ident("x".to_string())),
                BinaryOp::Mul,
                Box::new(Expr::Num(2.0)),
            )),
            HashMap::new(),
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
        let expr = Expr::Method(
            Box::new(Expr::Ident("text".to_string())),
            "length".to_string(),
            vec![],
        );

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
        env.set("items", Value::List(vec![Value::Num(1.0), Value::Num(2.0)]));
        let expr = Expr::Method(
            Box::new(Expr::Ident("items".to_string())),
            "length".to_string(),
            vec![],
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 2.0);
    }

    #[test]
    fn test_evaluate_method_list_get() {
        let mut env = Environment::new();
        env.set("items", Value::List(vec![Value::Num(10.0), Value::Num(20.0), Value::Num(30.0)]));
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

        let expr = Expr::Method(
            Box::new(Expr::Ident("point".to_string())),
            "x".to_string(),
            vec![],
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 10.0);
    }

    #[test]
    fn test_evaluate_object_field_missing() {
        let mut env = Environment::new();
        let fields = HashMap::new();
        env.set("point", Value::Object("Point".to_string(), fields));

        let expr = Expr::Method(
            Box::new(Expr::Ident("point".to_string())),
            "z".to_string(),
            vec![],
        );

        let result = evaluate_expr(&expr, &env);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("z"));
    }

    #[test]
    fn test_evaluate_method_num_abs() {
        let mut env = Environment::new();
        env.set("x", Value::Num(-42.5));
        let expr = Expr::Method(
            Box::new(Expr::Ident("x".to_string())),
            "abs".to_string(),
            vec![],
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 42.5);
    }

    #[test]
    fn test_evaluate_method_unsupported() {
        let env = Environment::new();
        let expr = Expr::Method(
            Box::new(Expr::Ident("graph".to_string())),
            "plot".to_string(),
            vec![],
        );

        let result = evaluate_expr(&expr, &env);
        assert!(result.is_err());
    }

    #[test]
    fn test_evaluate_index_on_list() {
        let mut env = Environment::new();
        env.set("items", Value::List(vec![Value::Num(10.0), Value::Num(20.0), Value::Num(30.0)]));
        let expr = Expr::Index(
            Box::new(Expr::Ident("items".to_string())),
            Box::new(Expr::Num(1.0)),
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 20.0);
    }

    #[test]
    fn test_evaluate_index_on_vec2() {
        let mut env = Environment::new();
        env.set("pos", Value::Vec2([100.0, 200.0]));
        let expr = Expr::Index(
            Box::new(Expr::Ident("pos".to_string())),
            Box::new(Expr::Num(0.0)),
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_num(), 100.0);
    }

    #[test]
    fn test_evaluate_index_on_string() {
        let mut env = Environment::new();
        env.set("text", Value::Str("hello".to_string()));
        let expr = Expr::Index(
            Box::new(Expr::Ident("text".to_string())),
            Box::new(Expr::Num(1.0)),
        );

        let result = evaluate_expr(&expr, &env).unwrap();
        assert_eq!(result.as_str(), "e");
    }

    #[test]
    fn test_evaluate_index_out_of_bounds() {
        let mut env = Environment::new();
        env.set("items", Value::List(vec![Value::Num(10.0)]));
        let expr = Expr::Index(
            Box::new(Expr::Ident("items".to_string())),
            Box::new(Expr::Num(5.0)),
        );

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
            }
            other => panic!("Expected Object, got: {:?}", other),
        }
    }

    #[test]
    fn test_evaluate_closure_captures_variable_at_creation_time() {
        let mut env = Environment::new();
        env.set("y", Value::Num(3.0));
        let closure = Value::Closure(
            vec!["x".to_string()],
            Box::new(Expr::Binary(
                Box::new(Expr::Ident("x".to_string())),
                BinaryOp::Add,
                Box::new(Expr::Ident("y".to_string())),
            )),
            HashMap::from([("y".to_string(), Value::Num(3.0))]),
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
        assert_eq!(
            parse_color(&Expr::Ident("red".to_string())),
            [1.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(
            parse_color(&Expr::Ident("unknown".to_string())),
            [0.8, 0.8, 0.8, 1.0]
        );
        assert_eq!(parse_color(&Expr::Num(1.0)), [0.8, 0.8, 0.8, 1.0]);
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
                    Expr::Call(
                        "sin".to_string(),
                        vec![Expr::Num(std::f64::consts::FRAC_PI_2)],
                    ),
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
            &Expr::Binary(
                Box::new(Expr::Num(10.0)),
                BinaryOp::Mod,
                Box::new(Expr::Num(3.0)),
            ),
            &env,
        )
        .unwrap_or(Value::Num(0.0));
        assert!((result.as_num() - 1.0).abs() < 1e-10);

        // Modulo with division: 7 % 2 = 1
        let result = evaluate_expr(
            &Expr::Binary(
                Box::new(Expr::Num(7.0)),
                BinaryOp::Mod,
                Box::new(Expr::Num(2.0)),
            ),
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
                Box::new(Expr::Tuple(vec![
                    Expr::Num(1.0),
                    Expr::Num(2.0),
                    Expr::Num(3.0),
                ])),
                BinaryOp::Add,
                Box::new(Expr::Tuple(vec![
                    Expr::Num(4.0),
                    Expr::Num(5.0),
                    Expr::Num(6.0),
                ])),
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
                Box::new(Expr::Tuple(vec![
                    Expr::Num(1.0),
                    Expr::Num(2.0),
                    Expr::Num(3.0),
                ])),
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
        let result = evaluate_expr(&Expr::Call("rand".to_string(), vec![]), &env)
            .unwrap_or(Value::Num(0.0));
        let val = result.as_num();
        assert!(
            (0.0..1.0).contains(&val),
            "rand() should return value in [0, 1), got {}",
            val
        );

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
        assert!(
            (0.0..100.0).contains(&val),
            "rand() * 100 should be in [0, 100), got {}",
            val
        );
    }

    #[test]
    fn seeded_rand_is_deterministic() {
        let source = r#"
            c: Ellipse, radius: 50, color: red
            always {
                c.position = (seeded_rand(1.0) * 100, seeded_rand(2.0) * 100)
            }
        "#;
        let ast = animatix_syntax::parser::parser_simple().parse(source).into_result().unwrap();
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
        let ast = animatix_syntax::parser::parser_simple().parse(source).into_result().unwrap();
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
        let ast = animatix_syntax::parser::parser_simple().parse(source).into_result().unwrap();
        let timeline = super::super::Timeline::build(&ast);

        // Evaluate seeded_rand with different seeds directly in the timeline's env
        let expr1 = Expr::Call("seeded_rand".to_string(), vec![Expr::Num(1.0)]);
        let expr2 = Expr::Call("seeded_rand".to_string(), vec![Expr::Num(2.0)]);

        let val1 = evaluate_expr(&expr1, &timeline.env).unwrap();
        let val2 = evaluate_expr(&expr2, &timeline.env).unwrap();

        let n1 = match val1 { Value::Num(n) => n, _ => panic!("expected num") };
        let n2 = match val2 { Value::Num(n) => n, _ => panic!("expected num") };

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
