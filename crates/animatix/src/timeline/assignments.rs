use super::{
    AnimationTrack, Diagnostic, Easing, ModifierHost, ParsedTimingModifiers, SHAPE_PATH,
    SHAPE_POLYGON, Timeline, Value, assignment_target_key, best_path_suggestion,
    build_shape_vello_path, evaluate_expr_with_lookup_diagnostic, mark_track_manual_position,
    parse_color_in_env_with_lookup_diagnostic, parse_numeric_vec2, parse_timing_modifiers,
    preserve_discrete_position_state_before, preserve_instant_delayed_value,
    push_unknown_target_path_diagnostic, resolve_position_binding_with_lookup_diagnostic,
    set_track_position_binding, styled_vello_path,
};

impl Timeline {
    pub(super) fn process_assignment_statement(
        &mut self,
        target: &[String],
        property: &str,
        value: &super::Expr,
        modifiers: &[super::Modifier],
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let eval_env = self.build_eval_env(time_ms as u64);
        let assignment_subject = format!("{}.{}", target.join("."), property);
        let ParsedTimingModifiers {
            duration_ms,
            delay_ms,
            easing,
            ..
        } = parse_timing_modifiers(
            modifiers,
            ModifierHost::Assignment,
            Some(&assignment_subject),
            diagnostics,
        );

        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
        let instant_delayed = delay_ms > 0.0 && duration_ms == 0.0;

        if target.len() == 1 && target[0] == "scene" {
            if property == "background_color" {
                let Some(target_color) = parse_color_in_env_with_lookup_diagnostic(
                    "scene",
                    "background_color",
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                ) else {
                    return;
                };
                if duration_ms > 0.0 {
                    let start_val = self.background_color.evaluate(t_start_ms);
                    self.background_color
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut self.background_color, t_start_ms);
                }
                self.background_color
                    .add_keyframe(t_end_ms, target_color, easing);
            }
            return;
        }

        let target_key = assignment_target_key(target);

        if target.len() > 1 && !self.nodes.contains_key(&target_key) {
            let suggestion =
                best_path_suggestion(&target_key, self.nodes.keys().map(String::as_str));
            push_unknown_target_path_diagnostic(
                diagnostics,
                &assignment_subject,
                &target_key,
                suggestion,
            );
            return;
        }

        let track = self
            .tracks
            .entry(target_key.clone())
            .or_insert_with(|| AnimationTrack::new(target_key.clone()));

        match property {
            "color" => {
                let Some(target_color) = parse_color_in_env_with_lookup_diagnostic(
                    &target_key,
                    "color",
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                ) else {
                    return;
                };
                if duration_ms > 0.0 {
                    let start_val = track.color.evaluate(t_start_ms);
                    track
                        .color
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.color, t_start_ms);
                }
                track.color.add_keyframe(t_end_ms, target_color, easing);
            }
            "stroke_width" => {
                let target_width = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(0.0))
                .as_num() as f32;
                if duration_ms > 0.0 {
                    let start_val = track.stroke_width.evaluate(t_start_ms);
                    track
                        .stroke_width
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.stroke_width, t_start_ms);
                }
                track
                    .stroke_width
                    .add_keyframe(t_end_ms, target_width, easing);
            }
            "stroke_color" => {
                let Some(target_color) = parse_color_in_env_with_lookup_diagnostic(
                    &target_key,
                    "stroke_color",
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                ) else {
                    return;
                };
                if duration_ms > 0.0 {
                    let start_val = track.stroke_color.evaluate(t_start_ms);
                    track
                        .stroke_color
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.stroke_color, t_start_ms);
                }
                track
                    .stroke_color
                    .add_keyframe(t_end_ms, target_color, easing);
            }
            "stroke_progress" => {
                let target_val = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(0.0))
                .as_num() as f32;
                if duration_ms > 0.0 {
                    let start_val = track.stroke_progress.evaluate(t_start_ms);
                    track
                        .stroke_progress
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.stroke_progress, t_start_ms);
                }
                track
                    .stroke_progress
                    .add_keyframe(t_end_ms, target_val, easing);
            }
            "fill_opacity" => {
                let target_val = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(0.0))
                .as_num() as f32;
                if duration_ms > 0.0 {
                    let start_val = track.fill_opacity.evaluate(t_start_ms);
                    track
                        .fill_opacity
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.fill_opacity, t_start_ms);
                }
                track
                    .fill_opacity
                    .add_keyframe(t_end_ms, target_val, easing);
            }
            "size" => {
                let size_val = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(0.0));
                let target_size = if let Value::Vec2([w, h]) = size_val {
                    [w as f32 / 2.0, h as f32 / 2.0]
                } else {
                    track.size.last_value()
                };
                if duration_ms > 0.0 {
                    let start_val = track.size.evaluate(t_start_ms);
                    track
                        .size
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.size, t_start_ms);
                }
                track.size.add_keyframe(t_end_ms, target_size, easing);
            }
            "tip_length" => {
                let target_tip_length = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.size.last_value()[0] as f64))
                .as_num() as f32;
                if duration_ms > 0.0 {
                    let start_val = track.size.evaluate(t_start_ms);
                    track
                        .size
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.size, t_start_ms);
                }
                let mut target_size = track.size.evaluate(t_end_ms);
                target_size[0] = target_tip_length;
                track.size.add_keyframe(t_end_ms, target_size, easing);
            }
            "tip_width" => {
                let target_tip_width = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.size.last_value()[1] as f64))
                .as_num() as f32;
                if duration_ms > 0.0 {
                    let start_val = track.size.evaluate(t_start_ms);
                    track
                        .size
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.size, t_start_ms);
                }
                let mut target_size = track.size.evaluate(t_end_ms);
                target_size[1] = target_tip_width;
                track.size.add_keyframe(t_end_ms, target_size, easing);
            }
            "url" => {
                let target_url = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Str(String::new()))
                .as_str();
                if !target_url.is_empty() {
                    if let Some(target_image) = crate::timeline::image::load_image(&target_url) {
                        if duration_ms > 0.0 {
                            let start_val = track.image.evaluate(t_start_ms);
                            track
                                .image
                                .add_keyframe(t_start_ms, start_val, Easing::Linear);
                        } else if instant_delayed {
                            preserve_instant_delayed_value(&mut track.image, t_start_ms);
                        }
                        track
                            .image
                            .add_keyframe(t_end_ms, Some(target_image), easing);
                    } else {
                        eprintln!("Failed to load image file {}", target_url);
                    }
                }
            }
            "position" | "at" => {
                let target_pos = if let Some((binding, position)) =
                    resolve_position_binding_with_lookup_diagnostic(
                        Some(value),
                        None,
                        None,
                        &eval_env,
                        diagnostics,
                        &assignment_subject,
                    ) {
                    preserve_discrete_position_state_before(track, t_start_ms);
                    if instant_delayed {
                        preserve_instant_delayed_value(&mut track.position, t_start_ms);
                    }
                    mark_track_manual_position(track, t_start_ms);

                    if duration_ms > 0.0 {
                        let start_binding = track.position_binding.evaluate(t_start_ms);
                        track.position_binding.add_keyframe(
                            t_start_ms,
                            start_binding,
                            Easing::Linear,
                        );
                        track
                            .position_binding
                            .add_keyframe(t_end_ms, binding, easing);
                    } else {
                        set_track_position_binding(track, t_start_ms, binding);
                    }

                    position.unwrap_or_else(|| track.position.last_value())
                } else {
                    track.position.last_value()
                };
                if duration_ms > 0.0 {
                    let start_val = track.position.evaluate(t_start_ms);
                    track
                        .position
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.position, t_start_ms);
                }
                track.position.add_keyframe(t_end_ms, target_pos, easing);
            }
            "rotation" => {
                let target_rotation = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.rotation.last_value() as f64))
                .as_num() as f32;
                if duration_ms > 0.0 {
                    let start_val = track.rotation.evaluate(t_start_ms);
                    track
                        .rotation
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.rotation, t_start_ms);
                }
                track
                    .rotation
                    .add_keyframe(t_end_ms, target_rotation, easing);
            }
            "scale" => {
                let target_scale = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.scale.last_value() as f64))
                .as_num() as f32;
                if duration_ms > 0.0 {
                    let start_val = track.scale.evaluate(t_start_ms);
                    track
                        .scale
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.scale, t_start_ms);
                }
                track.scale.add_keyframe(t_end_ms, target_scale, easing);
            }
            "radius" => {
                let radius = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(0.0))
                .as_num() as f32;
                let target_size = [radius, radius];
                if duration_ms > 0.0 {
                    let start_val = track.size.evaluate(t_start_ms);
                    track
                        .size
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.size, t_start_ms);
                }
                track.size.add_keyframe(t_end_ms, target_size, easing);
            }
            "radius_x" => {
                let target_radius = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.size.last_value()[0] as f64))
                .as_num() as f32;
                let mut target_size = track.size.last_value();
                target_size[0] = target_radius;
                if duration_ms > 0.0 {
                    let start_val = track.size.evaluate(t_start_ms);
                    track
                        .size
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.size, t_start_ms);
                }
                track.size.add_keyframe(t_end_ms, target_size, easing);
            }
            "radius_y" => {
                let target_radius = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.size.last_value()[1] as f64))
                .as_num() as f32;
                let mut target_size = track.size.last_value();
                target_size[1] = target_radius;
                if duration_ms > 0.0 {
                    let start_val = track.size.evaluate(t_start_ms);
                    track
                        .size
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.size, t_start_ms);
                }
                track.size.add_keyframe(t_end_ms, target_size, easing);
            }
            "start_angle" => {
                let target_angle = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.arc_angles.last_value()[0] as f64))
                .as_num() as f32;
                let mut target_angles = track.arc_angles.last_value();
                target_angles[0] = target_angle;
                if duration_ms > 0.0 {
                    let start_val = track.arc_angles.evaluate(t_start_ms);
                    track
                        .arc_angles
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.arc_angles, t_start_ms);
                }
                track
                    .arc_angles
                    .add_keyframe(t_end_ms, target_angles, easing);
            }
            "sweep_angle" => {
                let target_angle = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.arc_angles.last_value()[1] as f64))
                .as_num() as f32;
                let mut target_angles = track.arc_angles.last_value();
                target_angles[1] = target_angle;
                if duration_ms > 0.0 {
                    let start_val = track.arc_angles.evaluate(t_start_ms);
                    track
                        .arc_angles
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.arc_angles, t_start_ms);
                }
                track
                    .arc_angles
                    .add_keyframe(t_end_ms, target_angles, easing);
            }
            "from" => {
                if let Some(target_from) = parse_numeric_vec2(value, &eval_env) {
                    if duration_ms > 0.0 {
                        let start_val = track.line_from.evaluate(t_start_ms);
                        track
                            .line_from
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                    } else if instant_delayed {
                        preserve_instant_delayed_value(&mut track.line_from, t_start_ms);
                    }
                    track.line_from.add_keyframe(t_end_ms, target_from, easing);
                }
            }
            "to" => {
                if let Some(target_to) = parse_numeric_vec2(value, &eval_env) {
                    if duration_ms > 0.0 {
                        let start_val = track.line_to.evaluate(t_start_ms);
                        track
                            .line_to
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                    } else if instant_delayed {
                        preserve_instant_delayed_value(&mut track.line_to, t_start_ms);
                    }
                    track.line_to.add_keyframe(t_end_ms, target_to, easing);
                }
            }
            _ => {}
        }

        if !track.vector_paths.default_value.is_empty() || !track.vector_paths.keyframes.is_empty()
        {
            let shape_type = track.shape_type.last_value();
            let size = track.size.last_value();
            let line_from = track.line_from.last_value();
            let line_to = track.line_to.last_value();
            let arc_angles = track.arc_angles.last_value();
            let color = track.color.last_value();
            let stroke_width = track.stroke_width.last_value();
            let stroke_color = track.stroke_color.last_value();
            let fill_opacity = track.fill_opacity.last_value();

            let target_vello_path = if matches!(shape_type, SHAPE_POLYGON | SHAPE_PATH) {
                let existing_path = track
                    .vector_paths
                    .last_value()
                    .first()
                    .map(|vp| vp.path.clone())
                    .unwrap_or_else(kurbo::BezPath::new);
                styled_vello_path(
                    existing_path,
                    shape_type,
                    color,
                    stroke_width,
                    stroke_color,
                    fill_opacity,
                )
            } else {
                build_shape_vello_path(
                    shape_type,
                    size,
                    line_from,
                    line_to,
                    arc_angles,
                    color,
                    stroke_width,
                    stroke_color,
                    fill_opacity,
                )
            };

            if duration_ms > 0.0 {
                let start_val = track.evaluate_vector_paths(t_start_ms);
                track
                    .vector_paths
                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
            } else if instant_delayed {
                preserve_instant_delayed_value(&mut track.vector_paths, t_start_ms);
            }
            track
                .vector_paths
                .add_keyframe(t_end_ms, vec![target_vello_path], easing);
        }
    }
}
