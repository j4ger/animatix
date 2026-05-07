use super::{
    AnimationTrack, Diagnostic, Easing, ModifierHost, ParsedTimingModifiers, PositionBinding,
    ShapeType, Timeline, Value, VectorShapeState, VectorShapeStyle, assignment_target_key, best_path_suggestion,
    build_shape_vello_path, build_vector_shape_vello_path, evaluate_expr_with_lookup_diagnostic,
    mark_track_manual_position, parse_color_in_env_with_lookup_diagnostic,
    parse_timing_modifiers, preserve_discrete_position_state_before,
    preserve_instant_delayed_value, push_unknown_target_path_diagnostic,
    resolve_position_binding_with_lookup_diagnostic, set_track_position_binding,
    DEFAULT_LAYOUT_HALF_SIZE,
};
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use crate::timeline::property_engine::{
    parse_property_value, write_property_field,
};
use crate::timeline::property_registry::{lookup_property, PropertyFlags};
use crate::timeline::track::TrackAccessor;

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

        // ── Scene-level property (background_color) ──
        if target.len() == 1 && target[0] == "scene" {
            if property == "background_color" {
                let Some(target_color) = parse_color_in_env_with_lookup_diagnostic(
                    "scene", "background_color", value, &eval_env, diagnostics, &assignment_subject,
                ) else { return; };
                if duration_ms > 0.0 {
                    let start_val = self.background_color.evaluate(t_start_ms);
                    self.background_color.add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    if t_start_ms > 0 && !self.background_color.keyframes.contains_key(&(t_start_ms - 1)) {
                        let prev_val = self.background_color.evaluate(t_start_ms - 1);
                        self.background_color.add_keyframe(t_start_ms - 1, prev_val, Easing::Linear);
                    }
                }
                self.background_color.add_keyframe(t_end_ms, target_color, easing);
            }
            return;
        }

        // ── Resolve target track ──
        let target_key = assignment_target_key(target);
        if target.len() > 1 && !self.tracks.contains_key(&target_key) {
            let suggestion = best_path_suggestion(&target_key, self.tracks.keys().map(String::as_str));
            push_unknown_target_path_diagnostic(diagnostics, &assignment_subject, &target_key, suggestion);
            return;
        }

        let track = self.tracks
            .entry(target_key.clone())
            .or_insert_with(|| AnimationTrack::new(target_key.clone()));

        // ── Special cases that can't go through the generic engine ──

        // Position / at — uses position binding resolution (compound property)
        if matches!(property, "position" | "at") {
            let default_pos = [0.0, 0.0];
            let default_binding = PositionBinding::Absolute;
            let target_pos = if let Some((binding, position)) =
                resolve_position_binding_with_lookup_diagnostic(
                    Some(value), None, None, &eval_env, diagnostics, &assignment_subject,
                ) {
                preserve_discrete_position_state_before(track, t_start_ms);
                if instant_delayed { preserve_instant_delayed_value(&mut track.position, t_start_ms); }
                mark_track_manual_position(track, t_start_ms);
                if duration_ms > 0.0 {
                    let start_binding = track.position_binding.get(t_start_ms, default_binding);
                    track.position_binding.ensure(default_binding).add_keyframe(t_start_ms, start_binding, Easing::Linear);
                    track.position_binding.ensure(default_binding).add_keyframe(t_end_ms, binding, easing);
                } else {
                    set_track_position_binding(track, t_start_ms, binding);
                }
                position.unwrap_or_else(|| track.position.last(default_pos))
            } else {
                track.position.last(default_pos)
            };
            if duration_ms > 0.0 {
                let start_val = track.position.get(t_start_ms, default_pos);
                track.position.ensure(default_pos).add_keyframe(t_start_ms, start_val, Easing::Linear);
            } else if instant_delayed {
                preserve_instant_delayed_value(&mut track.position, t_start_ms);
            }
            track.position.ensure(default_pos).add_keyframe(t_end_ms, target_pos, easing);
            return;
        }

        // Text content assignment — handled specially because it can't regenerate paths at runtime
        if matches!(property, "text" | "latex" | "math" | "code") {
            let target_text = evaluate_expr_with_lookup_diagnostic(
                value, &eval_env, diagnostics, &assignment_subject,
            )
            .unwrap_or(Value::Str(String::new()))
            .as_str()
            .to_string();
            if duration_ms > 0.0 {
                let start_val = track.text_content.get(t_start_ms, String::new());
                track.text_content.ensure(String::new()).add_keyframe(t_start_ms, start_val, Easing::Linear);
            } else if instant_delayed {
                preserve_instant_delayed_value(&mut track.text_content, t_start_ms);
            }
            track.text_content.ensure(String::new()).add_keyframe(t_end_ms, target_text, easing);
            return;
        }

        // Image url assignment
        if property == "url" {
            let target_url = evaluate_expr_with_lookup_diagnostic(
                value, &eval_env, diagnostics, &assignment_subject,
            )
            .unwrap_or(Value::Str(String::new()))
            .as_str();
            if !target_url.is_empty() {
                if !track.svg_paths.is_empty() && track.image.get(t_start_ms, None).is_none() {
                    diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::UnsupportedMediaAssignment,
                        DiagnosticPhase::Build,
                        "Svg url assignments are not supported yet; redeclare the Svg actor at a keyframe instead.".to_string(),
                    ).with_subject(&assignment_subject).with_path(&target_url));
                    return;
                }
                match crate::timeline::image::load_image(&target_url) {
                    Ok(target_image) => {
                        if duration_ms > 0.0 {
                            let start_val = track.image.get(t_start_ms, None);
                            track.image.ensure(None).add_keyframe(t_start_ms, start_val, Easing::Linear);
                        } else if instant_delayed {
                            preserve_instant_delayed_value(&mut track.image, t_start_ms);
                        }
                        track.image.ensure(None).add_keyframe(t_end_ms, Some(target_image), easing);
                    }
                    Err(error) => {
                        diagnostics.push(Diagnostic::warning(
                            DiagnosticCode::MediaLoadFailure,
                            DiagnosticPhase::Build,
                            format!("Failed to load image file '{target_url}': {error}"),
                        ).with_subject(&assignment_subject).with_path(&target_url));
                    }
                }
            }
            return;
        }

        // ── Generic engine for all other properties ──

        let track_label = &track.label.clone();

        // Special handling for size-like properties (also write to layout_size)
        let is_size_property = matches!(property, "size" | "radius" | "radius_x" | "radius_y" | "tip_length" | "tip_width");

        let schema = lookup_property(property);

        if let Some(schema) = schema {
            // Check if the property is assignable
            if !schema.flags.contains(PropertyFlags::ASSIGNABLE) {
                push_unsupported_assignment_property_diagnostic(
                    diagnostics, &assignment_subject, &target_key, property,
                );
                return;
            }

            // Special handling for properties that write to size + layout_size together
            if is_size_property {
                handle_size_assignment(track, property, value, &eval_env, &assignment_subject,
                    t_start_ms, t_end_ms, easing, instant_delayed, diagnostics);
                return;
            }

            // Special handling for start_angle / sweep_angle → write to arc_angles
            if matches!(property, "start_angle" | "sweep_angle") {
                handle_arc_angle_assignment(track, property, value, &eval_env, &assignment_subject,
                    t_start_ms, t_end_ms, easing, instant_delayed, diagnostics);
                return;
            }

            // Standard engine dispatch for everything else
            if let Some(pv) = parse_property_value(schema.value_type, value, &eval_env, diagnostics, &assignment_subject) {
                write_property_field(track, schema.field, pv, t_start_ms, t_end_ms, easing, diagnostics);

                // If this property affects shape geometry, rebuild vector paths
                if affects_shape_geometry(property) {
                    rebuild_vector_paths(track, t_end_ms, easing, diagnostics);
                }
            }
        } else {
            // Unknown property — try shape rebuild
            if affects_shape_geometry(property) && false {
                // This path would be for truly unknown shape properties
            } else {
                push_unsupported_assignment_property_diagnostic(
                    diagnostics, &assignment_subject, &target_key, property,
                );
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────
// Helper: size-like assignments (also write to layout_size)
// ─────────────────────────────────────────────────────────────

fn handle_size_assignment(
    track: &mut AnimationTrack,
    property: &str,
    value: &super::Expr,
    env: &super::Environment,
    subject: &str,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    instant_delayed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let has_duration = t_end_ms > t_start_ms;

    let target_size = match property {
        "size" => {
            if let Some(v) = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject) {
                if let Value::Vec2([w, h]) = v {
                    [w as f32 / 2.0, h as f32 / 2.0]
                } else { track.size.last(default_size) }
            } else { track.size.last(default_size) }
        }
        "radius" => {
            let r = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .map(|v| v.as_num() as f32).unwrap_or(track.size.last(default_size)[0]);
            [r, r]
        }
        "radius_x" => {
            let mut s = track.size.last(default_size);
            s[0] = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .map(|v| v.as_num() as f32).unwrap_or(s[0]);
            s
        }
        "radius_y" => {
            let mut s = track.size.last(default_size);
            s[1] = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .map(|v| v.as_num() as f32).unwrap_or(s[1]);
            s
        }
        "tip_length" => {
            let mut s = track.size.last(default_size);
            s[0] = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .map(|v| v.as_num() as f32).unwrap_or(s[0]);
            s
        }
        "tip_width" => {
            let mut s = track.size.last(default_size);
            s[1] = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .map(|v| v.as_num() as f32).unwrap_or(s[1]);
            s
        }
        _ => track.size.last(default_size),
    };

    if has_duration {
        let start_val = track.size.get(t_start_ms, default_size);
        track.size.ensure(default_size).add_keyframe(t_start_ms, start_val, Easing::Linear);
        if let Some(layout_start) = track.layout_size_get(t_start_ms) {
            track.ensure_layout_size(default_size).add_keyframe(t_start_ms, layout_start, Easing::Linear);
        }
    } else if instant_delayed {
        preserve_instant_delayed_value(&mut track.size, t_start_ms);
        preserve_instant_delayed_value(&mut track.layout_size, t_start_ms);
    }
    track.size.ensure(default_size).add_keyframe(t_end_ms, target_size, easing);
    track.ensure_layout_size(default_size).add_keyframe(t_end_ms, target_size, easing);

    // Rebuild vector paths after size change
    rebuild_vector_paths(track, t_end_ms, easing, diagnostics);
}

// ─────────────────────────────────────────────────────────────
// Helper: start_angle / sweep_angle → arc_angles
// ─────────────────────────────────────────────────────────────

fn handle_arc_angle_assignment(
    track: &mut AnimationTrack,
    property: &str,
    value: &super::Expr,
    env: &super::Environment,
    subject: &str,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    instant_delayed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let default_arc = [0.0, std::f32::consts::PI];
    let has_duration = t_end_ms > t_start_ms;

    let target_angle = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
        .map(|v| v.as_num() as f32)
        .unwrap_or(track.arc_angles.last(default_arc)[0]);

    let mut target_angles = track.arc_angles.last(default_arc);
    match property {
        "start_angle" => target_angles[0] = target_angle,
        "sweep_angle" => target_angles[1] = target_angle,
        _ => {}
    }

    if has_duration {
        let start_val = track.arc_angles.get(t_start_ms, default_arc);
        track.arc_angles.ensure(default_arc).add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if instant_delayed {
        preserve_instant_delayed_value(&mut track.arc_angles, t_start_ms);
    }
    track.arc_angles.ensure(default_arc).add_keyframe(t_end_ms, target_angles, easing);

    // Rebuild vector paths after angle change
    rebuild_vector_paths(track, t_end_ms, easing, diagnostics);
}

// ─────────────────────────────────────────────────────────────
// Helper: rebuild vector paths
// ─────────────────────────────────────────────────────────────

fn rebuild_vector_paths(
    track: &mut AnimationTrack,
    t_end_ms: u64,
    easing: Easing,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let default_arc = [0.0, std::f32::consts::PI];
    let size = track.size.last(default_size);
    let line_from = track.line_from.last([-50.0, 0.0]);
    let line_to = track.line_to.last([50.0, 0.0]);
    let arc_angles = track.arc_angles.last(default_arc);
    let color = track.color.last([1.0, 1.0, 1.0, 1.0]);
    let stroke_width = track.stroke_width.last(2.0);
    let stroke_color = track.stroke_color.last([1.0, 1.0, 1.0, 1.0]);
    let fill_opacity = track.fill_opacity.last(1.0);
    let shape_type = track.shape_type.last(ShapeType::Rect);

    // Build vector shape state and compute paths
    let mut vector_shape_state = VectorShapeState::new(size, line_from, line_to, arc_angles);
    // Restore points for Polygon actors
    vector_shape_state.points = track.points.last(Vec::new());
    if !vector_shape_state.points.is_empty() {
        use crate::timeline::KurboShape;
        let pts: Vec<kurbo::Point> = vector_shape_state.points.iter().map(|&[x, y]| kurbo::Point::new(x as f64, y as f64)).collect();
        vector_shape_state.custom_path = Some(KurboShape::Polygon {
            points: pts,
        }.to_path_default());
    }

    let shape_type = track.shape_type.last(ShapeType::Rect);
    let target_vello_path = build_vector_shape_vello_path(
        shape_type,
        &vector_shape_state,
        VectorShapeStyle { color, stroke_width, stroke_color, fill_opacity },
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

    if t_end_ms > 0 {
        preserve_instant_delayed_value(&mut track.vector_paths, t_end_ms);
    }
    track.vector_paths.ensure(Vec::new()).add_keyframe(t_end_ms, vec![target_vello_path], easing);
}

// ─────────────────────────────────────────────────────────────
// Helper: does this property affect shape geometry?
// ─────────────────────────────────────────────────────────────

fn affects_shape_geometry(property: &str) -> bool {
    matches!(property,
        "from" | "to" | "start_angle" | "sweep_angle" | "arc_angles"
        | "radius" | "radius_x" | "radius_y" | "size"
        | "tip_length" | "tip_width" | "points"
        | "shape_type"
    )
}

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
