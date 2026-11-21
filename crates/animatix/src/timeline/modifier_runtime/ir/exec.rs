//! Direct interpreter for modifier IR programs.
//!
//! Modifier bodies (`always`, `drive`, reactive bindings) are lowered once to
//! [`ModifierIrProgram`] at build time and interpreted per frame here. All
//! expression evaluation delegates to [`super::evaluate_compiled_expr`], which
//! is also used by the plot sampling path, so there is a single expression
//! executor for frame-time code.

use super::fast_program::execute_fast_with_geometry;
use super::types::{ModifierIrProgram, ModifierIrStmt, ModifierOverrides};
use super::{CompiledExpr, evaluate_with_points};
use crate::ast::LoopPattern;
use crate::timeline::frame_env::apply_override_incremental;
use crate::timeline::{Environment, EvalError, Value};

thread_local! {
    static OVERRIDE_LABEL: std::cell::RefCell<String> =
        std::cell::RefCell::new(String::with_capacity(64));
}

/// Record a property override into `overrides`, avoiding string allocations when
/// keys already exist in the buffer.
#[inline]
pub(crate) fn record_override(
    overrides: &mut ModifierOverrides,
    target: &str,
    property: &str,
    value: &Value,
) {
    if let Some(inner) = overrides.get_mut(target) {
        if let Some(entry) = inner.get_mut(property) {
            *entry = value.clone();
        } else {
            inner.insert(property.to_string(), value.clone());
        }
    } else {
        let mut inner = std::collections::HashMap::with_capacity(4);
        inner.insert(property.to_string(), value.clone());
        overrides.insert(target.to_string(), inner);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ModifierGeometry<'a> {
    pub timeline: &'a crate::timeline::Timeline,
    pub time_ms: u64,
    pub dimensions: crate::timeline::SceneDimensions,
}

pub(super) fn evaluate(
    expr: &CompiledExpr,
    env: &Environment,
    geometry: Option<ModifierGeometry<'_>>,
    overrides: &ModifierOverrides,
) -> Result<Value, EvalError> {
    let Some(geometry) = geometry else {
        return evaluate_with_points(expr, env, None);
    };
    let frame = crate::timeline::Timeline::point_query_frame(
        geometry.time_ms,
        geometry.dimensions,
        overrides,
    );
    let read = |actor: &crate::timeline::plan_value::ActorRef, point| {
        geometry.timeline.actor_point(actor.entity_id()?, point, &frame, false)
    };
    evaluate_with_points(expr, env, Some(&read))
}

/// Execute a lowered modifier IR program against a frame environment.
pub fn execute_modifier_ir(
    program: &ModifierIrProgram,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
) -> Result<(), EvalError> {
    execute_with_geometry(program, frame_env, overrides, None)
}

pub(crate) fn execute_with_geometry(
    program: &ModifierIrProgram,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
    geometry: Option<ModifierGeometry<'_>>,
) -> Result<(), EvalError> {
    if let Some(ref fast) = program.fast {
        return execute_fast_with_geometry(fast, frame_env, overrides, geometry);
    }
    for stmt in &program.statements {
        execute_stmt(stmt, frame_env, overrides, geometry)?;
    }
    Ok(())
}

pub(crate) fn execute_stmt(
    stmt: &ModifierIrStmt,
    frame_env: &mut Environment,
    overrides: &mut ModifierOverrides,
    geometry: Option<ModifierGeometry<'_>>,
) -> Result<(), EvalError> {
    match stmt {
        ModifierIrStmt::Assign {
            target_key,
            property,
            value,
            ..
        } => {
            let value = evaluate(value, frame_env, geometry, overrides)?;
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
            } else if frame_env.set_object_field(target_key, property, value.clone()) {
                return Ok(());
            }
            record_override(overrides, target_key, property, &value);
            apply_override_incremental(frame_env, target_key, property, value);
        },
        ModifierIrStmt::AssignIndexed {
            base,
            index_position,
            index,
            property,
            value,
        } => {
            let value = evaluate(value, frame_env, geometry, overrides)?;
            let index_value = evaluate(index, frame_env, geometry, overrides)?;
            if let Some(index) = modifier_index(base, index_value) {
                write_indexed_override(
                    frame_env,
                    overrides,
                    base,
                    *index_position,
                    index,
                    property,
                    value,
                );
            }
        },
        ModifierIrStmt::Let { name, value } => {
            let value = evaluate(value, frame_env, geometry, overrides)?;
            frame_env.set(name, value);
        },
        ModifierIrStmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let condition = evaluate(condition, frame_env, geometry, overrides)?;
            let branch = if condition.is_truthy() {
                then_branch
            } else {
                else_branch
            };
            for stmt in branch {
                execute_stmt(stmt, frame_env, overrides, geometry)?;
            }
        },
        ModifierIrStmt::For {
            var,
            index_var,
            iterable,
            body,
        } => {
            let iterable = evaluate(iterable, frame_env, geometry, overrides)?;
            execute_loop(frame_env, var, index_var.as_deref(), iterable, |env, _| {
                for stmt in body {
                    execute_stmt(stmt, env, overrides, geometry)?;
                }
                Ok(())
            })?;
        },
        ModifierIrStmt::Noop => {},
    }
    Ok(())
}

pub(super) fn modifier_index(base: &str, index: Value) -> Option<usize> {
    match index {
        Value::Num(n) if n >= 0.0 && n == n.floor() => Some(n as usize),
        Value::Num(n) => {
            tracing::warn!("Array index for '{base}' must be a non-negative integer, got {n}");
            None
        },
        other => {
            tracing::warn!("Array index for '{base}' must evaluate to a number, got {other:?}");
            None
        },
    }
}

pub(super) fn write_indexed_override(
    env: &mut Environment,
    overrides: &mut ModifierOverrides,
    base: &str,
    index_position: usize,
    index: usize,
    property: &str,
    value: Value,
) {
    OVERRIDE_LABEL.with_borrow_mut(|label_buf| {
        label_buf.clear();
        use std::fmt::Write;
        for (position, segment) in base.split('.').enumerate() {
            if position > 0 {
                label_buf.push('.');
            }
            if position == index_position {
                let _ = write!(label_buf, "{segment}__{index}");
            } else {
                label_buf.push_str(segment);
            }
        }
        record_override(overrides, label_buf, property, &value);
        apply_override_incremental(env, label_buf, property, value);
    });
}

pub(super) fn execute_loop(
    env: &mut Environment,
    pattern: &LoopPattern,
    index_var: Option<&str>,
    iterable: Value,
    mut body: impl FnMut(&mut Environment, usize) -> Result<(), EvalError>,
) -> Result<(), EvalError> {
    let scope = crate::timeline::env::LoopScope::capture(env, pattern, index_var);
    let mut step = |(index, value)| {
        if index >= 100_000 {
            return Err(EvalError::TypeMismatch("for-loop exceeded 100,000 iterations".into()));
        }
        bind_loop_var(env, pattern, value);
        if let Some(name) = index_var {
            env.set(name, Value::Num(index as f64));
        }
        body(env, index)
    };
    let result = match iterable {
        Value::List(list) => list.iter().cloned().enumerate().try_for_each(&mut step),
        Value::Vec2(v) => v.into_iter().map(Value::Num).enumerate().try_for_each(&mut step),
        Value::Vec3(v) => v.into_iter().map(Value::Num).enumerate().try_for_each(&mut step),
        Value::Vec4(v) => v.into_iter().map(Value::Num).enumerate().try_for_each(&mut step),
        other => step((0, other)),
    };
    scope.restore(env);
    result
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
