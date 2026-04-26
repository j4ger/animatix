use super::{
    AnimationTrack, Diagnostic, Easing, ModifierHost, ParsedTimingModifiers, PositionBinding,
    ShapeType, Timeline, Value, VectorShapeState, VectorShapeStyle, assignment_target_key, best_path_suggestion,
    build_shape_vello_path, build_vector_shape_vello_path, evaluate_expr_with_lookup_diagnostic,
    mark_track_manual_position, parse_color_in_env_with_lookup_diagnostic, parse_numeric_vec2,
    parse_point_list_expr, parse_timing_modifiers, preserve_discrete_position_state_before,
    preserve_instant_delayed_value, push_unknown_target_path_diagnostic,
    resolve_position_binding_with_lookup_diagnostic, set_track_position_binding,
    vector_shape_uses_custom_path,
};
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use crate::timeline::track::TrackAccessor;

fn push_unsupported_assignment_property_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
    target_key: &str,
    property: &str,
) {
    diagnostics.push(
        Diagnostic::warning(
            DiagnosticCode::UnsupportedAssignmentProperty,
            DiagnosticPhase::Build,
            format!(
                "Assignment property '{property}' on '{target_key}' is not part of the current runtime assignment surface; ignoring this assignment."
            ),
        )
        .with_subject(subject),
    );
}

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
                    // Inline preserve logic for non-Option PropertyTrack
                    if t_start_ms > 0 && !self.background_color.keyframes.contains_key(&(t_start_ms - 1)) {
                        let prev_val = self.background_color.evaluate(t_start_ms - 1);
                        self.background_color.add_keyframe(t_start_ms - 1, prev_val, Easing::Linear);
                    }
                }
                self.background_color
                    .add_keyframe(t_end_ms, target_color, easing);
            }
            return;
        }

        let target_key = assignment_target_key(target);

        if target.len() > 1 && !self.tracks.contains_key(&target_key) {
            let suggestion =
                best_path_suggestion(&target_key, self.tracks.keys().map(String::as_str));
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
                    let start_val = track.color.get(t_start_ms, [1.0, 1.0, 1.0, 1.0]);
                    track
                        .color
                        .ensure([1.0, 1.0, 1.0, 1.0])
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.color, t_start_ms);
                }
                track.color.ensure([1.0, 1.0, 1.0, 1.0]).add_keyframe(t_end_ms, target_color, easing);
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
                    let start_val = track.stroke_width.get(t_start_ms, 2.0);
                    track
                        .stroke_width
                        .ensure(2.0)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.stroke_width, t_start_ms);
                }
                track
                    .stroke_width
                    .ensure(2.0)
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
                    let start_val = track.stroke_color.get(t_start_ms, [1.0, 1.0, 1.0, 1.0]);
                    track
                        .stroke_color
                        .ensure([1.0, 1.0, 1.0, 1.0])
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.stroke_color, t_start_ms);
                }
                track
                    .stroke_color
                    .ensure([1.0, 1.0, 1.0, 1.0])
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
                    let start_val = track.stroke_progress.get(t_start_ms, 1.0);
                    track
                        .stroke_progress
                        .ensure(1.0)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.stroke_progress, t_start_ms);
                }
                track
                    .stroke_progress
                    .ensure(1.0)
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
                    let start_val = track.fill_opacity.get(t_start_ms, 1.0);
                    track
                        .fill_opacity
                        .ensure(1.0)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.fill_opacity, t_start_ms);
                }
                track
                    .fill_opacity
                    .ensure(1.0)
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
                let default_size = crate::timeline::track::DEFAULT_LAYOUT_HALF_SIZE;
                let target_size = if let Value::Vec2([w, h]) = size_val {
                    [w as f32 / 2.0, h as f32 / 2.0]
                } else {
                    track.size.last(default_size)
                };
                if duration_ms > 0.0 {
                    let start_val = track.size.get(t_start_ms, default_size);
                    track
                        .size
                        .ensure(default_size)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.size, t_start_ms);
                }
                track.size.ensure(default_size).add_keyframe(t_end_ms, target_size, easing);
            }
            "tip_length" => {
                let default_size = crate::timeline::track::DEFAULT_LAYOUT_HALF_SIZE;
                let target_tip_length = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.size.last(default_size)[0] as f64))
                .as_num() as f32;
                if duration_ms > 0.0 {
                    let start_val = track.size.get(t_start_ms, default_size);
                    track
                        .size
                        .ensure(default_size)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.size, t_start_ms);
                }
                let mut target_size = track.size.get(t_end_ms, default_size);
                target_size[0] = target_tip_length;
                track.size.ensure(default_size).add_keyframe(t_end_ms, target_size, easing);
            }
            "tip_width" => {
                let default_size = crate::timeline::track::DEFAULT_LAYOUT_HALF_SIZE;
                let target_tip_width = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.size.last(default_size)[1] as f64))
                .as_num() as f32;
                if duration_ms > 0.0 {
                    let start_val = track.size.get(t_start_ms, default_size);
                    track
                        .size
                        .ensure(default_size)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.size, t_start_ms);
                }
                let mut target_size = track.size.get(t_end_ms, default_size);
                target_size[1] = target_tip_width;
                track.size.ensure(default_size).add_keyframe(t_end_ms, target_size, easing);
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
                    if !track.svg_paths.is_empty() && track.image.get(t_start_ms, None).is_none() {
                        diagnostics.push(
                            Diagnostic::warning(
                                DiagnosticCode::UnsupportedMediaAssignment,
                                DiagnosticPhase::Build,
                                "Svg url assignments are not supported yet; redeclare the Svg actor at a keyframe instead.".to_string(),
                            )
                            .with_subject(&assignment_subject)
                            .with_path(&target_url),
                        );
                        return;
                    }

                    match crate::timeline::image::load_image(&target_url) {
                        Ok(target_image) => {
                            if duration_ms > 0.0 {
                                let start_val = track.image.get(t_start_ms, None);
                                track
                                    .image
                                    .ensure(None)
                                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
                            } else if instant_delayed {
                                preserve_instant_delayed_value(&mut track.image, t_start_ms);
                            }
                            track
                                .image
                                .ensure(None)
                                .add_keyframe(t_end_ms, Some(target_image), easing);
                        }
                        Err(error) => {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::MediaLoadFailure,
                                    DiagnosticPhase::Build,
                                    format!("Failed to load image file '{target_url}': {error}"),
                                )
                                .with_subject(&assignment_subject)
                                .with_path(&target_url),
                            );
                        }
                    }
                }
            }
            "position" | "at" => {
                let default_pos = [0.0, 0.0];
                let default_binding = PositionBinding::Absolute;
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
                        let start_binding = track.position_binding.get(t_start_ms, default_binding);
                        track.position_binding.ensure(default_binding).add_keyframe(
                            t_start_ms,
                            start_binding,
                            Easing::Linear,
                        );
                        track
                            .position_binding
                            .ensure(default_binding)
                            .add_keyframe(t_end_ms, binding, easing);
                    } else {
                        set_track_position_binding(track, t_start_ms, binding);
                    }

                    position.unwrap_or_else(|| track.position.last(default_pos))
                } else {
                    track.position.last(default_pos)
                };
                if duration_ms > 0.0 {
                    let start_val = track.position.get(t_start_ms, default_pos);
                    track
                        .position
                        .ensure(default_pos)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.position, t_start_ms);
                }
                track.position.ensure(default_pos).add_keyframe(t_end_ms, target_pos, easing);
            }
            "rotation" => {
                let target_rotation = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.rotation.last(0.0) as f64))
                .as_num() as f32;
                if duration_ms > 0.0 {
                    let start_val = track.rotation.get(t_start_ms, 0.0);
                    track
                        .rotation
                        .ensure(0.0)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.rotation, t_start_ms);
                }
                track
                    .rotation
                    .ensure(0.0)
                    .add_keyframe(t_end_ms, target_rotation, easing);
            }
            "scale" => {
                let target_scale = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.scale.last(1.0) as f64))
                .as_num() as f32;
                if duration_ms > 0.0 {
                    let start_val = track.scale.get(t_start_ms, 1.0);
                    track
                        .scale
                        .ensure(1.0)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.scale, t_start_ms);
                }
                track.scale.ensure(1.0).add_keyframe(t_end_ms, target_scale, easing);
            }
            "radius" => {
                let default_size = crate::timeline::track::DEFAULT_LAYOUT_HALF_SIZE;
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
                    let start_val = track.size.get(t_start_ms, default_size);
                    track
                        .size
                        .ensure(default_size)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.size, t_start_ms);
                }
                track.size.ensure(default_size).add_keyframe(t_end_ms, target_size, easing);
            }
            "radius_x" => {
                let default_size = crate::timeline::track::DEFAULT_LAYOUT_HALF_SIZE;
                let target_radius = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.size.last(default_size)[0] as f64))
                .as_num() as f32;
                let mut target_size = track.size.last(default_size);
                target_size[0] = target_radius;
                if duration_ms > 0.0 {
                    let start_val = track.size.get(t_start_ms, default_size);
                    track
                        .size
                        .ensure(default_size)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.size, t_start_ms);
                }
                track.size.ensure(default_size).add_keyframe(t_end_ms, target_size, easing);
            }
            "radius_y" => {
                let default_size = crate::timeline::track::DEFAULT_LAYOUT_HALF_SIZE;
                let target_radius = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.size.last(default_size)[1] as f64))
                .as_num() as f32;
                let mut target_size = track.size.last(default_size);
                target_size[1] = target_radius;
                if duration_ms > 0.0 {
                    let start_val = track.size.get(t_start_ms, default_size);
                    track
                        .size
                        .ensure(default_size)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.size, t_start_ms);
                }
                track.size.ensure(default_size).add_keyframe(t_end_ms, target_size, easing);
            }
            "start_angle" => {
                let default_arc = [0.0, std::f32::consts::PI];
                let target_angle = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.arc_angles.last(default_arc)[0] as f64))
                .as_num() as f32;
                let mut target_angles = track.arc_angles.last(default_arc);
                target_angles[0] = target_angle;
                if duration_ms > 0.0 {
                    let start_val = track.arc_angles.get(t_start_ms, default_arc);
                    track
                        .arc_angles
                        .ensure(default_arc)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.arc_angles, t_start_ms);
                }
                track
                    .arc_angles
                    .ensure(default_arc)
                    .add_keyframe(t_end_ms, target_angles, easing);
            }
            "sweep_angle" => {
                let default_arc = [0.0, std::f32::consts::PI];
                let target_angle = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.arc_angles.last(default_arc)[1] as f64))
                .as_num() as f32;
                let mut target_angles = track.arc_angles.last(default_arc);
                target_angles[1] = target_angle;
                if duration_ms > 0.0 {
                    let start_val = track.arc_angles.get(t_start_ms, default_arc);
                    track
                        .arc_angles
                        .ensure(default_arc)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.arc_angles, t_start_ms);
                }
                track
                    .arc_angles
                    .ensure(default_arc)
                    .add_keyframe(t_end_ms, target_angles, easing);
            }
            "angle" => {
                let target_angle = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Num(track.rotation.last(0.0) as f64))
                .as_num() as f32;
                if duration_ms > 0.0 {
                    let start_val = track.rotation.get(t_start_ms, 0.0);
                    track
                        .rotation
                        .ensure(0.0)
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.rotation, t_start_ms);
                }
                track.rotation.ensure(0.0).add_keyframe(t_end_ms, target_angle, easing);
            }
            "from" => {
                let default_line_from = [-50.0, 0.0];
                if let Some(target_from) = parse_numeric_vec2(value, &eval_env) {
                    if duration_ms > 0.0 {
                        let start_val = track.line_from.get(t_start_ms, default_line_from);
                        track
                            .line_from
                            .ensure(default_line_from)
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                    } else if instant_delayed {
                        preserve_instant_delayed_value(&mut track.line_from, t_start_ms);
                    }
                    track.line_from.ensure(default_line_from).add_keyframe(t_end_ms, target_from, easing);
                }
            }
            "to" => {
                let default_line_to = [50.0, 0.0];
                if let Some(target_to) = parse_numeric_vec2(value, &eval_env) {
                    if duration_ms > 0.0 {
                        let start_val = track.line_to.get(t_start_ms, default_line_to);
                        track
                            .line_to
                            .ensure(default_line_to)
                            .add_keyframe(t_start_ms, start_val, Easing::Linear);
                    } else if instant_delayed {
                        preserve_instant_delayed_value(&mut track.line_to, t_start_ms);
                    }
                    track.line_to.ensure(default_line_to).add_keyframe(t_end_ms, target_to, easing);
                }
            }
            "text" | "latex" | "math" | "code" => {
                let target_text = evaluate_expr_with_lookup_diagnostic(
                    value,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                )
                .unwrap_or(Value::Str(String::new()))
                .as_str()
                .to_string();
                if duration_ms > 0.0 {
                    let start_val = track.text_content.get(t_start_ms, String::new());
                    track
                        .text_content
                        .ensure(String::new())
                        .add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.text_content, t_start_ms);
                }
                track.text_content.ensure(String::new()).add_keyframe(t_end_ms, target_text, easing);
            }
            "points" => {
                let target_points = parse_point_list_expr(value, &eval_env)
                    .map(|points| {
                        points
                            .into_iter()
                            .map(|p| [p.x as f32, p.y as f32])
                            .collect()
                    })
                    .unwrap_or_default();

                if duration_ms > 0.0 {
                    let start_val = track.points.get(t_start_ms, Vec::new());
                    track.points.ensure(Vec::new()).add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut track.points, t_start_ms);
                }
                track.points.ensure(Vec::new()).add_keyframe(t_end_ms, target_points, easing);
            }
            _ => {
                push_unsupported_assignment_property_diagnostic(
                    diagnostics,
                    &assignment_subject,
                    &target_key,
                    property,
                );
                return;
            }
        }

        if track.vector_paths.as_ref().map(|t| !t.default_value.is_empty() || !t.keyframes.is_empty()).unwrap_or(false)
        {
            let default_size = crate::timeline::track::DEFAULT_LAYOUT_HALF_SIZE;
            let default_arc = [0.0, std::f32::consts::PI];
            let shape_type = track.shape_type.last(ShapeType::Rect);
            let size = track.size.last(default_size);
            let line_from = track.line_from.last([-50.0, 0.0]);
            let line_to = track.line_to.last([50.0, 0.0]);
            let arc_angles = track.arc_angles.last(default_arc);
            let color = track.color.last([1.0, 1.0, 1.0, 1.0]);
            let stroke_width = track.stroke_width.last(2.0);
            let stroke_color = track.stroke_color.last([1.0, 1.0, 1.0, 1.0]);
            let fill_opacity = track.fill_opacity.last(1.0);

            let mut vector_shape_state =
                VectorShapeState::new(size, line_from, line_to, arc_angles);
            if vector_shape_uses_custom_path(shape_type) {
                vector_shape_state.custom_path = track
                    .vector_paths
                    .last(Vec::new())
                    .first()
                    .map(|vp| vp.path.clone());
            }
            let target_vello_path = build_vector_shape_vello_path(
                shape_type,
                &vector_shape_state,
                VectorShapeStyle {
                    color,
                    stroke_width,
                    stroke_color,
                    fill_opacity,
                },
            )
            .unwrap_or_else(|| {
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
            });

            if duration_ms > 0.0 {
                let start_val = track.evaluate_vector_paths(t_start_ms);
                track
                    .vector_paths
                    .ensure(Vec::new())
                    .add_keyframe(t_start_ms, start_val, Easing::Linear);
            } else if instant_delayed {
                preserve_instant_delayed_value(&mut track.vector_paths, t_start_ms);
            }
            track
                .vector_paths
                .ensure(Vec::new())
                .add_keyframe(t_end_ms, vec![target_vello_path], easing);
        }
    }
}
