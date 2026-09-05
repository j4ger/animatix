//! Direct interpreter for modifier IR programs.
//!
//! This replaces the previous stack-machine bytecode VM. Modifier bodies
//! (`always`, `drive`, reactive bindings) are lowered once to
//! [`ModifierIrProgram`] at build time and interpreted per frame here. All
//! expression evaluation delegates to [`super::evaluate_compiled_expr`], which
//! is also used by the plot sampling path, so there is a single expression
//! executor for frame-time code.

use crate::ast::{LoopPattern, array_actor_label};
use crate::timeline::frame_env::apply_override_incremental;
use crate::timeline::{Environment, EvalError, Value};

use super::evaluate_compiled_expr;
use super::types::{ModifierIrProgram, ModifierIrStmt, ModifierOverrides};

/// Execute a lowered modifier IR program against a frame environment.
pub fn execute_modifier_ir(
    program: &ModifierIrProgram,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
) -> Result<(), EvalError> {
    for stmt in &program.statements {
        execute_stmt(stmt, frame_env, overrides)?;
    }
    Ok(())
}

fn execute_stmt(
    stmt: &ModifierIrStmt,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
) -> Result<(), EvalError> {
    match stmt {
        ModifierIrStmt::Assign {
            target,
            property,
            value,
        } => {
            let value = evaluate_compiled_expr(value, frame_env)?;
            let target_key = target.join(".");
            // Frame-local object field writes take priority over actor/property
            // overrides. `set_object_path`/`set_object_field` return false when
            // the target is not an object, in which case we fall through to the
            // actor override path.
            if let Some((root, rest)) = target_key.split_once('.') {
                let path: Vec<String> =
                    rest.split('.').map(|segment| segment.to_string()).collect();
                if frame_env.set_object_path(&path, root, property, value.clone()) {
                    return Ok(());
                }
            } else if frame_env.set_object_field(&target_key, property, value.clone()) {
                return Ok(());
            }
            overrides
                .entry(target_key.clone())
                .or_default()
                .insert(property.clone(), value.clone());
            apply_override_incremental(frame_env, &target_key, property, value);
        },
        ModifierIrStmt::AssignIndexed {
            base,
            index,
            property,
            value,
        } => {
            let value = evaluate_compiled_expr(value, frame_env)?;
            let index_value = evaluate_compiled_expr(index, frame_env)?;
            let n = match index_value {
                Value::Num(n) if n >= 0.0 && n == n.floor() => n as usize,
                Value::Num(n) => {
                    tracing::warn!(
                        "Array index for '{}' must be a non-negative integer, got {}",
                        base,
                        n
                    );
                    return Ok(());
                },
                other => {
                    tracing::warn!(
                        "Array index for '{}' must evaluate to a number, got {:?}",
                        base,
                        other
                    );
                    return Ok(());
                },
            };
            let label = array_actor_label(base, n);
            overrides
                .entry(label.clone())
                .or_default()
                .insert(property.clone(), value.clone());
            apply_override_incremental(frame_env, &label, property, value);
        },
        ModifierIrStmt::Let { name, value } => {
            let value = evaluate_compiled_expr(value, frame_env)?;
            frame_env.set(name, value);
        },
        ModifierIrStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = evaluate_compiled_expr(condition, frame_env)?;
            let branch = if condition.is_truthy() {
                then_branch
            } else {
                else_branch
            };
            for stmt in branch {
                execute_stmt(stmt, frame_env, overrides)?;
            }
        },
        ModifierIrStmt::For {
            var,
            index_var,
            iterable,
            body,
        } => {
            let iterable = evaluate_compiled_expr(iterable, frame_env)?;
            let items: Vec<Value> = match iterable {
                Value::List(list) => list.to_vec(),
                Value::Vec2(v) => v.into_iter().map(Value::Num).collect(),
                Value::Vec3(v) => v.into_iter().map(Value::Num).collect(),
                Value::Vec4(v) => v.into_iter().map(Value::Num).collect(),
                other => vec![other],
            };

            for (idx, item) in items.into_iter().enumerate() {
                if idx >= 100_000 {
                    return Err(EvalError::TypeMismatch(
                        "for-loop exceeded 100,000 iterations".to_string(),
                    ));
                }
                bind_loop_var(frame_env, var, item);
                if let Some(index_var) = index_var {
                    frame_env.set(index_var, Value::Num(idx as f64));
                }
                for stmt in body {
                    execute_stmt(stmt, frame_env, overrides)?;
                }
            }

            // Clear loop variables from the override layer so they do not leak
            // into subsequent statements.
            match var {
                LoopPattern::Single(name) => {
                    frame_env.overrides.remove(name);
                    frame_env.mark_mutated();
                },
                LoopPattern::Tuple(names) => {
                    for name in names {
                        frame_env.overrides.remove(name);
                        frame_env.mark_mutated();
                    }
                },
            }
            if let Some(index_var) = index_var {
                frame_env.overrides.remove(index_var);
                frame_env.mark_mutated();
            }
        },
        ModifierIrStmt::Noop => {},
    }
    Ok(())
}

/// Bind a loop pattern to a value in the frame environment.
fn bind_loop_var(frame_env: &mut Environment, pattern: &LoopPattern, value: Value) {
    match pattern {
        LoopPattern::Single(name) => {
            frame_env.set(name, value);
        },
        LoopPattern::Tuple(names) => match value {
            Value::List(items) => {
                for (name, item) in names.iter().zip(items.iter()) {
                    frame_env.set(name, item.clone());
                }
            },
            Value::Vec2([x, y]) if names.len() == 2 => {
                frame_env.set(&names[0], Value::Num(x));
                frame_env.set(&names[1], Value::Num(y));
            },
            Value::Vec3([x, y, z]) if names.len() == 3 => {
                frame_env.set(&names[0], Value::Num(x));
                frame_env.set(&names[1], Value::Num(y));
                frame_env.set(&names[2], Value::Num(z));
            },
            Value::Vec4([x, y, z, w]) if names.len() == 4 => {
                frame_env.set(&names[0], Value::Num(x));
                frame_env.set(&names[1], Value::Num(y));
                frame_env.set(&names[2], Value::Num(z));
                frame_env.set(&names[3], Value::Num(w));
            },
            Value::Color([r, g, b, a]) if names.len() == 4 => {
                frame_env.set(&names[0], Value::Num(r));
                frame_env.set(&names[1], Value::Num(g));
                frame_env.set(&names[2], Value::Num(b));
                frame_env.set(&names[3], Value::Num(a));
            },
            _ => {
                if let Some(name) = names.first() {
                    frame_env.set(name, value);
                }
            },
        },
    }
}
