//! Modifier statement execution (tree-walk, IR, bytecode).
//!
//! This module contains the runtime that applies modifier statements
//! (`always` blocks, `drive` blocks, etc.) to a frame environment.
//! It is separated from [`frame_env`](super::frame_env) because modifier
//! execution has a different stability profile — it evolves with new
//! statement types and optimization tiers (IR, bytecode) — while frame
//! environment construction is relatively stable.

use tracing::warn;

use super::modifier_runtime::vm;
use super::{
    EvalError, SceneDimensions, Stmt, Timeline, Value, assignment_target_key_with_env,
    evaluate_expr,
};
use crate::ast::LoopPattern;

impl Timeline {
    /// Apply a single modifier statement to the frame environment.
    ///
    /// Updates `overrides` and `frame_env` in place according to the
    /// statement type (assignment, let, conditional, for loop).
    pub fn apply_modifier_stmt(
        &self,
        stmt: &Stmt,
        frame_env: &mut super::Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) {
        match stmt {
            Stmt::Assignment {
                target,
                property,
                value,
                ..
            } => {
                match evaluate_expr(value, frame_env) {
                    Ok(val) => {
                        if matches!(target[0], crate::ast::TargetSegment::Static(_)) {
                            let object_path = target
                                .iter()
                                .skip(1)
                                .map(|seg| seg.label_str().to_string())
                                .collect::<Vec<_>>();
                            if frame_env.set_object_path(
                                &object_path,
                                target[0].label_str(),
                                property,
                                val.clone(),
                            ) {
                                return;
                            }
                        }

                        // Use the frame-time variant to handle Indexed segments (e.g.
                        // bars[i].color). For all-Static targets this is
                        // equivalent to assignment_target_key.
                        let label = match assignment_target_key_with_env(target, frame_env) {
                            Ok(l) => l,
                            Err(e) => {
                                warn!("Modifier assignment error for {:?}.{property}: {e}", target);
                                return;
                            },
                        };
                        overrides
                            .entry(label.clone())
                            .or_default()
                            .insert(property.clone(), val.clone());
                        super::frame_env::apply_override_incremental(
                            frame_env, &label, property, val,
                        );
                    },
                    Err(e) => warn!("Modifier assignment error for {:?}.{property}: {e}", target),
                }
            },
            Stmt::LetDecl {
                is_pub: _,
                name,
                value,
                ..
            } => match evaluate_expr(value, frame_env) {
                Ok(val) => {
                    frame_env.set(name, val);
                },
                Err(e) => warn!("Modifier let-decl error for {name}: {e}"),
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                if evaluate_expr(condition, frame_env)
                    .map(|value| value.as_num() != 0.0)
                    .unwrap_or(false)
                {
                    for stmt in then_branch {
                        self.apply_modifier_stmt(stmt, frame_env, overrides);
                    }
                } else if let Some(else_branch) = else_branch {
                    for stmt in else_branch {
                        self.apply_modifier_stmt(stmt, frame_env, overrides);
                    }
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => match evaluate_expr(scrutinee, frame_env) {
                Ok(value) => {
                    for (_pat, body) in arms {
                        if crate::timeline::build::pattern_matches(_pat, &value) {
                            for stmt in body {
                                self.apply_modifier_stmt(stmt, frame_env, overrides);
                            }
                            break;
                        }
                    }
                },
                Err(e) => warn!("match scrutinee evaluation failed: {e}"),
            },
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                ..
            } => {
                if let Ok(values) = evaluate_expr(iterable, frame_env) {
                    let items: Vec<Value> = match values {
                        Value::List(list) => list,
                        Value::Vec2(v) => v.into_iter().map(Value::Num).collect(),
                        Value::Vec3(v) => v.into_iter().map(Value::Num).collect(),
                        Value::Vec4(v) => v.into_iter().map(Value::Num).collect(),
                        other => vec![other],
                    };
                    for (idx, item) in items.into_iter().enumerate() {
                        bind_loop_var_modifier(frame_env, var, item);
                        if let Some(iv) = index_var {
                            frame_env.set(iv, Value::Num(idx as f64));
                        }
                        for stmt in body {
                            self.apply_modifier_stmt(stmt, frame_env, overrides);
                        }
                    }
                    // Keep legacy tree-walker loop scope cleanup aligned with the VM.
                    match var {
                        LoopPattern::Single(name) => {
                            frame_env.overrides.remove(name);
                        },
                        LoopPattern::Tuple(names) => {
                            for name in names {
                                frame_env.overrides.remove(name);
                            }
                        },
                    }
                    if let Some(iv) = index_var {
                        frame_env.overrides.remove(iv);
                    }
                }
            },
            _ => {}, // Non-modifier statements are not valid inside modifier bodies
        }
    }

    /// Execute a modifier bytecode program against the current frame environment.
    pub fn apply_modifier_bytecode_program(
        &self,
        program: &vm::ModifierBytecodeProgram,
        _time_ms: u64,
        _scene_dimensions: SceneDimensions,
        frame_env: &mut super::Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Result<(), EvalError> {
        vm::execute_modifier_bytecode(program, frame_env, overrides)
    }
}

/// Bind loop variables according to the pattern.
fn bind_loop_var_modifier(frame_env: &mut super::Environment, var: &LoopPattern, value: Value) {
    match var {
        LoopPattern::Single(name) => {
            frame_env.set(name, value);
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
            for (i, name) in names.iter().enumerate().take(components.len().min(names.len())) {
                if i < components.len() {
                    frame_env.set(name, components[i].clone());
                }
            }
        },
    }
}
