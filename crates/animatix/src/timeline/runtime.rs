use super::modifier_runtime::{ir, vm};
use super::{
    Environment, EvalError, SceneAnchor, SceneDimensions, Stmt, Timeline, Value,
    assignment_target_key, evaluate_expr, scene_anchor_point, set_lookup_color,
    set_lookup_vec2,
};

/// Apply a modifier override incrementally to the frame environment.
///
/// Instead of rebuilding the entire environment from scratch (which requires
/// cloning `self.env` and re-evaluating all track properties), this updates
/// only the specific key that changed plus any derived values.
pub(crate) fn apply_override_incremental(
    env: &mut Environment,
    label: &str,
    property: &str,
    value: Value,
) {
    let key = format!("{label}.{property}");
    env.set(&key, value.clone());

    // Inject typed sub-keys for known compound types
    match &value {
        Value::Vec2([x, y]) => {
            env.set(&format!("{key}.x"), Value::Num(*x));
            env.set(&format!("{key}.y"), Value::Num(*y));
        }
        Value::Color([r, g, b, a]) => {
            env.set(&format!("{key}.r"), Value::Num(*r));
            env.set(&format!("{key}.g"), Value::Num(*g));
            env.set(&format!("{key}.b"), Value::Num(*b));
            env.set(&format!("{key}.a"), Value::Num(*a));
        }
        _ => {}
    }

    // Recalculate derived values when size changes
    if property == "size" {
        if let Value::Vec2([w, h]) = value {
            let r = w.min(h) / 2.0;
            env.set(&format!("{label}.radius"), Value::Num(r));
            env.set(&format!("{label}.radius_x"), Value::Num(w / 2.0));
            env.set(&format!("{label}.radius_y"), Value::Num(h / 2.0));
        }
    }
}

impl Timeline {
    pub(super) fn build_eval_env(&self, time_ms: u64) -> Environment {
        let mut env = self.env.clone();
        self.inject_runtime_lookup_values(&mut env, time_ms, None, None);
        env
    }

    pub(super) fn frame_eval_env(
        &self,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Environment {
        // Estimate capacity: base env + t/scene_width/scene_height + variable tracks +
        // ~35 properties per actor (only if modifiers are present).
        let has_modifiers = !self.modifier_programs.is_empty() || !self.modifiers.is_empty();
        let estimated_capacity = if has_modifiers {
            self.env.len() + 3 + self.variable_tracks.len() + self.tracks.len() * 35
        } else {
            self.env.len() + 3 + self.variable_tracks.len()
        };
        let mut env = Environment::with_capacity(estimated_capacity);
        env.extend_from(&self.env);
        env.set("t", Value::Num(time_ms as f64 / 1000.0));
        env.set("scene_width", Value::Num(scene_dimensions.width as f64));
        env.set("scene_height", Value::Num(scene_dimensions.height as f64));
        // Inject keyframe-scoped variable tracks into the frame environment.
        for (name, track) in &self.variable_tracks {
            if let Some(value) = track.evaluate(time_ms) {
                env.set(name, value);
            }
        }
        // Fast path: no modifiers means no property lookups at frame time.
        // Skip the per-track property evaluation entirely.
        if self.modifier_programs.is_empty() && self.modifiers.is_empty() {
            return env;
        }
        self.inject_runtime_lookup_values(
            &mut env,
            time_ms,
            Some(scene_dimensions),
            Some(overrides),
        );
        env
    }

    pub fn frame(
        &self,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Environment {
        self.frame_eval_env(time_ms, scene_dimensions, overrides)
    }

    pub(super) fn inject_runtime_lookup_values(
        &self,
        env: &mut Environment,
        time_ms: u64,
        scene_dimensions: Option<SceneDimensions>,
        overrides: Option<
            &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
        >,
    ) {
        let background_color = overrides
            .and_then(|map| map.get("scene"))
            .and_then(|props| props.get("background_color"))
            .and_then(|value| match value {
                Value::Color(c) => Some(*c),
                Value::Vec4(c) => Some(*c),
                _ => None,
            })
            .unwrap_or_else(|| {
                let [r, g, b, a] = self.background_color.evaluate(time_ms);
                [r as f64, g as f64, b as f64, a as f64]
            });
        set_lookup_color(env, "scene.background_color", background_color);

        if let Some(dimensions) = scene_dimensions {
            for (suffix, anchor) in [
                ("top_left", SceneAnchor::TopLeft),
                ("top", SceneAnchor::Top),
                ("top_right", SceneAnchor::TopRight),
                ("left", SceneAnchor::Left),
                ("center", SceneAnchor::Center),
                ("right", SceneAnchor::Right),
                ("bottom_left", SceneAnchor::BottomLeft),
                ("bottom", SceneAnchor::Bottom),
                ("bottom_right", SceneAnchor::BottomRight),
            ] {
                let point = scene_anchor_point(anchor, dimensions);
                set_lookup_vec2(env, &format!("scene.{}", suffix), [point.x, point.y]);
            }
        }

        for (label, track) in &self.tracks {
            // Use the centralized injector for base track values.
            crate::timeline::property_engine::inject_property_into_env(env, label, track, time_ms);

            // Apply overrides on top (from `always` blocks or modifiers).
            let node_overrides = overrides.and_then(|map| map.get(label));
            if let Some(overrides) = node_overrides {
                for (key, val) in overrides {
                    env.set(&format!("{label}.{key}"), val.clone());
                    // Also inject typed sub-keys for known properties
                    match val {
                        Value::Vec2([x, y]) => {
                            env.set(&format!("{label}.{key}.x"), Value::Num(*x));
                            env.set(&format!("{label}.{key}.y"), Value::Num(*y));
                        }
                        Value::Color([r, g, b, a]) => {
                            env.set(&format!("{label}.{key}.r"), Value::Num(*r));
                            env.set(&format!("{label}.{key}.g"), Value::Num(*g));
                            env.set(&format!("{label}.{key}.b"), Value::Num(*b));
                            env.set(&format!("{label}.{key}.a"), Value::Num(*a));
                        }
                        _ => {}
                    }
                }
            }

            // Recalculate derived values after overrides
            if node_overrides.is_some() {
                let size_val = env.get(&format!("{label}.size"));
                if let Some(Value::Vec2([w, h])) = size_val {
                    let r = w.min(h) / 2.0;
                    env.set(&format!("{label}.radius"), Value::Num(r));
                    env.set(&format!("{label}.radius_x"), Value::Num(w / 2.0));
                    env.set(&format!("{label}.radius_y"), Value::Num(h / 2.0));
                }
            }
        }
    }

    pub(super) fn apply_modifier_stmt(
        &self,
        stmt: &Stmt,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        frame_env: &mut Environment,
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
                    apply_override_incremental(frame_env, &label, property, val);
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
                        self.apply_modifier_stmt(
                            stmt,
                            time_ms,
                            scene_dimensions,
                            frame_env,
                            overrides,
                        );
                    }
                } else if let Some(else_branch) = else_branch {
                    for stmt in else_branch {
                        self.apply_modifier_stmt(
                            stmt,
                            time_ms,
                            scene_dimensions,
                            frame_env,
                            overrides,
                        );
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
                            self.apply_modifier_stmt(
                                stmt,
                                time_ms,
                                scene_dimensions,
                                frame_env,
                                overrides,
                            );
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn apply_modifier_stmt_for_test(
        &self,
        stmt: &Stmt,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        frame_env: &mut Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) {
        self.apply_modifier_stmt(stmt, time_ms, scene_dimensions, frame_env, overrides)
    }

    pub fn apply_modifier_ir_program(
        &self,
        program: &ir::ModifierIrProgram,
        _time_ms: u64,
        _scene_dimensions: SceneDimensions,
        frame_env: &mut Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Result<(), EvalError> {
        ir::execute_modifier_ir(program, frame_env, overrides)
    }

    pub fn apply_modifier_bytecode_program(
        &self,
        program: &vm::ModifierBytecodeProgram,
        _time_ms: u64,
        _scene_dimensions: SceneDimensions,
        frame_env: &mut Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Result<(), EvalError> {
        vm::execute_modifier_bytecode(program, frame_env, overrides)
    }
}
