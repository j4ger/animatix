use super::{
    assignment_target_key, evaluate_expr, scene_anchor_point, set_lookup_color, set_lookup_scalar,
    set_lookup_vec2, Environment, EvalError, SceneAnchor, SceneDimensions, Stmt, Timeline, Value,
};

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
        let mut env = self.env.clone();
        env.set("t", Value::Num(time_ms as f64 / 1000.0));
        env.set("scene_width", Value::Num(scene_dimensions.width as f64));
        env.set("scene_height", Value::Num(scene_dimensions.height as f64));
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
            let node_overrides = overrides.and_then(|map| map.get(label));
            let motion_offset = track.motion_offset.evaluate(time_ms);
            let rotation = track.rotation.evaluate(time_ms) as f64;
            let scale = track.scale.evaluate(time_ms) as f64;
            let base_position = node_overrides
                .and_then(|props| props.get("at").or_else(|| props.get("position")))
                .and_then(|value| match value {
                    Value::Vec2(v) => Some(*v),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let [x, y] = track.position.evaluate(time_ms);
                    [x as f64, y as f64]
                });
            let position = [
                base_position[0] + motion_offset[0] as f64,
                base_position[1] + motion_offset[1] as f64,
            ];
            set_lookup_vec2(env, &format!("{}.at", label), position);
            env.set(&format!("{}.position", label), Value::Vec2(position));
            set_lookup_vec2(
                env,
                &format!("{}.shift", label),
                [motion_offset[0] as f64, motion_offset[1] as f64],
            );
            set_lookup_scalar(env, &format!("{}.rotation", label), rotation);
            set_lookup_scalar(env, &format!("{}.scale", label), scale);

            let size = node_overrides
                .and_then(|props| props.get("size"))
                .and_then(|value| match value {
                    Value::Vec2(v) => Some(*v),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let [w, h] = track.size.evaluate(time_ms);
                    [w as f64 * 2.0, h as f64 * 2.0]
                });
            set_lookup_vec2(env, &format!("{}.size", label), size);
            set_lookup_scalar(env, &format!("{}.width", label), size[0]);
            set_lookup_scalar(env, &format!("{}.height", label), size[1]);
            if track.shape_type.evaluate(time_ms) == super::SHAPE_ARROW {
                set_lookup_scalar(env, &format!("{}.tip_length", label), size[0] / 2.0);
                set_lookup_scalar(env, &format!("{}.tip_width", label), size[1] / 2.0);
            }

            let radius_x = node_overrides
                .and_then(|props| props.get("radius_x"))
                .map(Value::as_num)
                .unwrap_or(size[0] / 2.0);
            let radius_y = node_overrides
                .and_then(|props| props.get("radius_y"))
                .map(Value::as_num)
                .unwrap_or(size[1] / 2.0);
            let radius = node_overrides
                .and_then(|props| props.get("radius"))
                .map(Value::as_num)
                .unwrap_or(radius_x);
            set_lookup_scalar(env, &format!("{}.radius", label), radius);
            set_lookup_scalar(env, &format!("{}.radius_x", label), radius_x);
            set_lookup_scalar(env, &format!("{}.radius_y", label), radius_y);

            let color = node_overrides
                .and_then(|props| props.get("color"))
                .and_then(|value| match value {
                    Value::Color(c) => Some(*c),
                    Value::Vec4(c) => Some(*c),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let [r, g, b, a] = track.color.evaluate(time_ms);
                    [r as f64, g as f64, b as f64, a as f64]
                });
            set_lookup_color(env, &format!("{}.color", label), color);

            let stroke_color = node_overrides
                .and_then(|props| props.get("stroke_color").or_else(|| props.get("stroke")))
                .and_then(|value| match value {
                    Value::Color(c) => Some(*c),
                    Value::Vec4(c) => Some(*c),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let [r, g, b, a] = track.stroke_color.evaluate(time_ms);
                    [r as f64, g as f64, b as f64, a as f64]
                });
            set_lookup_color(env, &format!("{}.stroke_color", label), stroke_color);

            let opacity = node_overrides
                .and_then(|props| props.get("opacity"))
                .map(Value::as_num)
                .unwrap_or(track.opacity.evaluate(time_ms) as f64);
            set_lookup_scalar(env, &format!("{}.opacity", label), opacity);

            let fill_opacity = node_overrides
                .and_then(|props| props.get("fill_opacity"))
                .map(Value::as_num)
                .unwrap_or(track.fill_opacity.evaluate(time_ms) as f64);
            set_lookup_scalar(env, &format!("{}.fill_opacity", label), fill_opacity);

            let stroke_width = node_overrides
                .and_then(|props| props.get("stroke_width").or_else(|| props.get("width")))
                .map(Value::as_num)
                .unwrap_or(track.stroke_width.evaluate(time_ms) as f64);
            set_lookup_scalar(env, &format!("{}.stroke_width", label), stroke_width);

            let stroke_progress = node_overrides
                .and_then(|props| props.get("stroke_progress"))
                .map(Value::as_num)
                .unwrap_or(track.stroke_progress.evaluate(time_ms) as f64);
            set_lookup_scalar(env, &format!("{}.stroke_progress", label), stroke_progress);

            let from = node_overrides
                .and_then(|props| props.get("from"))
                .and_then(|value| match value {
                    Value::Vec2(v) => Some(*v),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let [x, y] = track.line_from.evaluate(time_ms);
                    [x as f64, y as f64]
                });
            set_lookup_vec2(env, &format!("{}.from", label), from);

            let to = node_overrides
                .and_then(|props| props.get("to"))
                .and_then(|value| match value {
                    Value::Vec2(v) => Some(*v),
                    _ => None,
                })
                .unwrap_or_else(|| {
                    let [x, y] = track.line_to.evaluate(time_ms);
                    [x as f64, y as f64]
                });
            set_lookup_vec2(env, &format!("{}.to", label), to);

            let start_angle = node_overrides
                .and_then(|props| props.get("start_angle"))
                .map(Value::as_num)
                .unwrap_or(track.arc_angles.evaluate(time_ms)[0] as f64);
            let sweep_angle = node_overrides
                .and_then(|props| props.get("sweep_angle"))
                .map(Value::as_num)
                .unwrap_or(track.arc_angles.evaluate(time_ms)[1] as f64);
            set_lookup_scalar(env, &format!("{}.start_angle", label), start_angle);
            set_lookup_scalar(env, &format!("{}.sweep_angle", label), sweep_angle);
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
                    overrides
                        .entry(assignment_target_key(target))
                        .or_default()
                        .insert(property.clone(), val);
                    *frame_env = self.frame_eval_env(time_ms, scene_dimensions, overrides);
                }
            }
            Stmt::LetDecl { name, value } => {
                if let Ok(val) = evaluate_expr(value, frame_env) {
                    frame_env.set(name, val);
                }
            }
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
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
        program: &crate::ir::ModifierIrProgram,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        frame_env: &mut Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Result<(), EvalError> {
        crate::ir::execute_modifier_ir(program, frame_env, overrides, |frame_env, overrides| {
            *frame_env = self.frame_eval_env(time_ms, scene_dimensions, overrides);
        })
    }

    pub fn apply_modifier_bytecode_program(
        &self,
        program: &crate::vm::ModifierBytecodeProgram,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        frame_env: &mut Environment,
        overrides: &mut std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Result<(), EvalError> {
        crate::vm::execute_modifier_bytecode(
            program,
            frame_env,
            overrides,
            |frame_env, overrides| {
                *frame_env = self.frame_eval_env(time_ms, scene_dimensions, overrides);
            },
        )
    }
}
