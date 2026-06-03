//! Modifier statement execution (tree-walk, IR, bytecode).
//!
//! This module contains the runtime that applies modifier statements
//! (`always` blocks, `drive` blocks, etc.) to a frame environment.
//! It is separated from [`frame_env`](super::frame_env) because modifier
//! execution has a different stability profile — it evolves with new
//! statement types and optimization tiers (IR, bytecode) — while frame
//! environment construction is relatively stable.

use super::modifier_runtime::{ir, vm};
use super::{EvalError, SceneDimensions, Stmt, Timeline, Value, assignment_target_key, evaluate_expr};

impl Timeline {
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
                if let Ok(val) = evaluate_expr(value, frame_env) {
                    let label = assignment_target_key(target);
                    overrides
                        .entry(label.clone())
                        .or_default()
                        .insert(property.clone(), val.clone());
                    super::frame_env::apply_override_incremental(frame_env, &label, property, val);
                }
            }
            Stmt::LetDecl { is_pub: _, name, value, .. } => {
                if let Ok(val) = evaluate_expr(value, frame_env) {
                    frame_env.set(name, val);
                }
            }
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
            }
            Stmt::ForLoop { var, iterable, body, .. } => {
                if let Ok(values) = evaluate_expr(iterable, frame_env) {
                    let items: Vec<Value> = match values {
                        Value::List(list) => list,
                        Value::Vec2(v) => v.into_iter().map(Value::Num).collect(),
                        Value::Vec3(v) => v.into_iter().map(Value::Num).collect(),
                        Value::Vec4(v) => v.into_iter().map(Value::Num).collect(),
                        other => vec![other],
                    };
                    for item in items {
                        frame_env.set(var, item);
                        for stmt in body {
                            self.apply_modifier_stmt(stmt, frame_env, overrides);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Execute a modifier IR program against the current frame environment.
    pub fn apply_modifier_ir_program(
        &self,
        program: &ir::ModifierIrProgram,
        _time_ms: u64,
        _scene_dimensions: SceneDimensions,
        frame_env: &mut super::Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Result<(), EvalError> {
        ir::execute_modifier_ir(program, frame_env, overrides)
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
