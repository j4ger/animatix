//! Build-time evaluation of pure user functions (`fn f(...) -> T`).
//!
//! Pure functions have statement bodies (`let`/`if`/`match`/`for`/`return`)
//! and are evaluated against a local environment layered over the caller's
//! stdlib base. They never touch the timeline: the purity checker rejects
//! actions, actor assignments, and actor declarations in their bodies.

use std::sync::Arc;

use crate::ast::{LoopPattern, Stmt};
use crate::timeline::build::pattern_matches;
use crate::timeline::env::{Environment, Value};
use crate::timeline::lookup::for_iter_values;
use crate::timeline::{EvalError, utils};

/// Control flow signal produced by a statement block.
enum Flow {
    /// The block finished without a `return`.
    Continue,
    /// A `return` unwound with a value.
    Return(Value),
}

/// Evaluate a pure function against argument values.
pub(crate) fn evaluate_user_fn(
    name: &str,
    params: &[crate::ast::ParamDef],
    body: &[Stmt],
    arg_values: &[Value],
    caller_env: &Environment,
) -> Result<Value, EvalError> {
    let mut local = match caller_env.base.as_ref() {
        Some(base) => Environment::with_base(Arc::clone(base)),
        None => Environment::new(),
    };

    for (index, param) in params.iter().enumerate() {
        match arg_values.get(index) {
            Some(value) => {
                local.set(&param.name, value.clone());
            },
            None => {
                if let Some(default) = &param.default {
                    let value = utils::evaluate_expr(default, &local)?;
                    local.set(&param.name, value);
                } else {
                    return Err(EvalError::TypeMismatch(format!(
                        "function '{name}' is missing argument for parameter '{}'",
                        param.name
                    )));
                }
            },
        }
    }

    match exec_block(body, &mut local)? {
        Flow::Return(value) => Ok(value),
        Flow::Continue => Err(EvalError::TypeMismatch(format!(
            "function '{name}' reached the end of its body without returning a value"
        ))),
    }
}

/// Execute a statement block, propagating `return` unwinds.
fn exec_block(stmts: &[Stmt], env: &mut Environment) -> Result<Flow, EvalError> {
    for stmt in stmts {
        match stmt {
            Stmt::LetDecl { name, value, .. } => {
                let value = utils::evaluate_expr(value, env)?;
                env.set(name, value);
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let truthy = utils::evaluate_expr(condition, env)?.is_truthy();
                let flow = if truthy {
                    exec_block(then_branch, env)?
                } else if let Some(else_branch) = else_branch {
                    exec_block(else_branch, env)?
                } else {
                    Flow::Continue
                };
                if let Flow::Return(value) = flow {
                    return Ok(Flow::Return(value));
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                let value = utils::evaluate_expr(scrutinee, env)?;
                for (pat, arm_body) in arms {
                    if pattern_matches(pat, &value) {
                        if let Flow::Return(value) = exec_block(arm_body, env)? {
                            return Ok(Flow::Return(value));
                        }
                        break;
                    }
                }
            },
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                ..
            } => {
                for (idx, value) in for_iter_values(iterable, env).into_iter().enumerate() {
                    bind_loop_var_local(env, var, value, idx);
                    if let Some(iv) = index_var {
                        env.set(iv, Value::Num(idx as f64));
                    }
                    if let Flow::Return(value) = exec_block(body, env)? {
                        remove_loop_vars_local(env, var, index_var);
                        return Ok(Flow::Return(value));
                    }
                }
                remove_loop_vars_local(env, var, index_var);
            },
            Stmt::Return { value, .. } => {
                let value = match value {
                    Some(expr) => utils::evaluate_expr(expr, env)?,
                    None => Value::Bool(false),
                };
                return Ok(Flow::Return(value));
            },
            // Timeline constructs are rejected by the purity checker before a
            // pure function is ever evaluated.
            _ => {},
        }
    }
    Ok(Flow::Continue)
}

/// Bind a loop variable locally (mirrors the timeline's `bind_loop_var`).
fn bind_loop_var_local(env: &mut Environment, var: &LoopPattern, value: Value, index: usize) {
    match var {
        LoopPattern::Single(name) => {
            env.set(name, value);
        },
        LoopPattern::Tuple(names) => {
            let components: Vec<Value> = match &value {
                Value::List(items) => items.clone(),
                Value::Vec2(v) => v.iter().map(|&x| Value::Num(x)).collect(),
                Value::Vec3(v) => v.iter().map(|&x| Value::Num(x)).collect(),
                Value::Vec4(v) => v.iter().map(|&x| Value::Num(x)).collect(),
                Value::Color(v) => v.iter().map(|&x| Value::Num(x)).collect(),
                other => vec![other.clone()],
            };
            let min_len = names.len().min(components.len());
            for (i, name) in names.iter().enumerate().take(min_len) {
                env.set(name, components[i].clone());
            }
            let _ = index;
        },
    }
}

/// Remove loop-variable bindings after a loop exits.
fn remove_loop_vars_local(env: &mut Environment, var: &LoopPattern, index_var: &Option<String>) {
    match var {
        LoopPattern::Single(name) => {
            env.overrides.remove(name);
        },
        LoopPattern::Tuple(names) => {
            for name in names {
                env.overrides.remove(name);
            }
        },
    }
    if let Some(iv) = index_var {
        env.overrides.remove(iv);
    }
}

/// Collect pure function declarations from the AST so the build environment
/// can seed them as callable [`Value::UserFn`] values.
pub(crate) fn collect_pure_fns(
    ast: &[Stmt],
) -> Vec<(String, Vec<crate::ast::ParamDef>, Vec<Stmt>)> {
    let mut fns = Vec::new();
    for stmt in ast {
        if let Stmt::FnDecl {
            name,
            params,
            return_type,
            body,
            ..
        } = stmt
        {
            if return_type.is_some() {
                fns.push((name.clone(), params.clone(), body.clone()));
            }
        }
    }
    fns
}
