use super::{
    AnimationTrack, Diagnostic, Easing, ModifierHost, ParsedTimingModifiers, PositionBinding,
    ShapeType, Timeline, Value, VectorShapeState, VectorShapeStyle, assignment_target_key, best_path_suggestion,
    build_shape_vello_path, build_vector_shape_vello_path,
    evaluate_expr_with_lookup_diagnostic,
    mark_track_manual_position, parse_color_in_env_with_lookup_diagnostic,
    parse_timing_modifiers, preserve_discrete_position_state_before,
    preserve_instant_delayed_value, push_unknown_target_path_diagnostic,
    resolve_position_binding_with_lookup_diagnostic, set_track_position_binding,
    DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE,
};
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use crate::primitives::{AssignmentCtx, find_primitive};
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
        explicit_easing: Option<super::Easing>,
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if target.is_empty() {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::InvalidAssignmentTarget,
                DiagnosticPhase::Build,
                format!(
                    "Assignment '{property} = ...' must include an actor label, or be placed inside a 'drive' block",
                ),
            ));
            return;
        }
        let eval_env = self.build_eval_env(time_ms as u64);
        let assignment_subject = format!("{}.{}", target.join("."), property);
        let ParsedTimingModifiers {
            duration_ms,
            delay_ms,
            easing: modifier_easing,
            ..
        } = parse_timing_modifiers(
            modifiers,
            ModifierHost::Assignment,
            Some(&assignment_subject),
            diagnostics,
        );
        let easing = explicit_easing.unwrap_or(modifier_easing);

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

        // ── Primitive dispatch: let each primitive handle its own special cases ──
        let type_name = super::actor_kind_meta(track.kind).type_name;
        let primitive = find_primitive(type_name);
        if let Some(primitive) = primitive {
            let mut ctx = AssignmentCtx {
                t_start_ms,
                t_end_ms,
                easing,
                instant_delayed,
                duration_ms,
                font_context: &self.font_context,
                text_compiler: &mut self.text_compiler.borrow_mut(),
            };
            if primitive.handle_assignment(track, property, value, &mut ctx, &eval_env, diagnostics, &assignment_subject) {
                return;
            }
        }

        // ── Generic engine for all other properties ──

        let _track_label = &track.label.clone();

        // Special handling for size-like properties (also write to layout_size)
        let is_size_property = matches!(property, "size" | "radius_x" | "radius_y");

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

            // Standard engine dispatch for everything else
            if let Some(pv) = parse_property_value(schema.value_type, value, &eval_env, diagnostics, &assignment_subject) {
                write_property_field(track, schema.field, pv, t_start_ms, t_end_ms, easing, diagnostics);

                // If this property affects shape geometry, rebuild vector paths
                if affects_shape_geometry(property) {
                    rebuild_vector_paths(track, t_start_ms, t_end_ms, easing, diagnostics);
                }
            }
        } else {
            // Unknown property — report diagnostic
            push_unsupported_assignment_property_diagnostic(
                diagnostics, &assignment_subject, &target_key, property,
            );
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
    rebuild_vector_paths(track, t_start_ms, t_end_ms, easing, diagnostics);
}

// ─────────────────────────────────────────────────────────────
// Helper: text / math / code assignments → text paths
// ─────────────────────────────────────────────────────────────

pub(crate) fn recompile_text_at_assignment(
    track: &mut AnimationTrack,
    target_text: String,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    instant_delayed: bool,
    duration_ms: f64,
    font_ctx: &crate::renderer::text::FontContext,
    text_compiler: &mut crate::renderer::text::TextCompiler,
) {
    if duration_ms > 0.0 {
        let start_val = track.text_content.get(t_start_ms, String::new());
        track.text_content.ensure(String::new()).add_keyframe(t_start_ms, start_val, Easing::Linear);
    } else if instant_delayed {
        preserve_instant_delayed_value(&mut track.text_content, t_start_ms);
    }
    track.text_content.ensure(String::new()).add_keyframe(t_end_ms, target_text.clone(), easing);

    let text_kind = match track.kind {
        super::ActorKindId::Text => crate::renderer::text::TextKind::Text,
        super::ActorKindId::Math => crate::renderer::text::TextKind::Math,
        super::ActorKindId::Code => crate::renderer::text::TextKind::Code,
        super::ActorKindId::Typst => crate::renderer::text::TextKind::Typst,
        _ => return,
    };

    let font_family = track.font_family.get(t_end_ms, String::new());
    let font_size = track.font_size.get(t_end_ms, 48.0);
    let color = track.color.get(t_end_ms, [1.0, 1.0, 1.0, 1.0]);

    let new_paths = text_compiler.compile(&target_text, &font_family, font_size, color, text_kind, font_ctx);
    let new_half_size = crate::renderer::text::measure_text_paths(&new_paths);

    if duration_ms > 0.0 {
        let start_val = track.evaluate_text_paths(t_start_ms);
        track.text_paths.ensure(Vec::new()).add_keyframe(t_start_ms, start_val, Easing::Linear);
        let start_size = track.size.get(t_start_ms, DEFAULT_LAYOUT_HALF_SIZE);
        let start_layout_size = track.layout_size_get(t_start_ms).unwrap_or(DEFAULT_LAYOUT_HALF_SIZE);
        track.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(t_start_ms, start_size, Easing::Linear);
        track.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(t_start_ms, start_layout_size, Easing::Linear);
    } else if instant_delayed {
        preserve_instant_delayed_value(&mut track.text_paths, t_start_ms);
        preserve_instant_delayed_value(&mut track.size, t_start_ms);
        preserve_instant_delayed_value(&mut track.layout_size, t_start_ms);
    }

    track.text_paths.ensure(Vec::new()).add_keyframe(t_end_ms, new_paths, easing);
    track.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(t_end_ms, new_half_size, easing);
    track.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(t_end_ms, new_half_size, easing);
}

// ─────────────────────────────────────────────────────────────
// Helper: rebuild vector paths
// ─────────────────────────────────────────────────────────────

fn rebuild_vector_paths(
    track: &mut AnimationTrack,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
    _diagnostics: &mut Vec<Diagnostic>,
) {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let default_arc = [0.0, 0.0];
    let has_duration = t_end_ms > t_start_ms;
    let size = track.size.last(default_size);
    let line_from = track.line_from.last([-50.0, 0.0]);
    let line_to = track.line_to.last([50.0, 0.0]);
    let arc_angles = track.arc_angles.last(default_arc);
    let color = track.color.last(DEFAULT_WHITE);
    let stroke_width = track.stroke_width.last(2.0);
    let stroke_color = track.stroke_color.last(DEFAULT_WHITE);
    let fill_opacity = track.fill_opacity.last(1.0);
    let _shape_type = track.shape_type.last(ShapeType::Rect);

    // Build vector shape state and compute paths
    let shape_type = track.shape_type.last(ShapeType::Rect);
    let mut vector_shape_state = VectorShapeState::new(shape_type, size);
    // Restore shape-specific fields from track data
    match &mut vector_shape_state {
        VectorShapeState::Line(line) => {
            line.line_from = track.line_from.last([-50.0, 0.0]);
            line.line_to = track.line_to.last([50.0, 0.0]);
        }
        VectorShapeState::Polygon(poly) => {
            // Restore points for Polygon actors
            poly.points = track.points.last(Vec::new());
            if !poly.points.is_empty() {
                use crate::timeline::KurboShape;
                let pts: Vec<kurbo::Point> = poly.points.iter().map(|&[x, y]| kurbo::Point::new(x as f64, y as f64)).collect();
                poly.custom_path = Some(KurboShape::Polygon {
                    points: pts,
                }.to_path_default());
            }
        }
        VectorShapeState::Path(path_state) => {
            // Restore commands for Path actors
            let commands_svg = track.commands.last(String::new());
            if !commands_svg.is_empty() {
                if let Ok(path) = kurbo::BezPath::from_svg(&commands_svg) {
                    path_state.custom_path = Some(path);
                }
            }
        }
        _ => {}
    }
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

    if has_duration {
        let start_paths = track.evaluate_vector_paths(t_start_ms);
        track.vector_paths.ensure(Vec::new()).add_keyframe(t_start_ms, start_paths, Easing::Linear);
    } else if t_end_ms > 0 {
        preserve_instant_delayed_value(&mut track.vector_paths, t_end_ms);
    }
    track.vector_paths.ensure(Vec::new()).add_keyframe(t_end_ms, vec![target_vello_path], easing);
}

// ─────────────────────────────────────────────────────────────
// Helper: does this property affect shape geometry?
// ─────────────────────────────────────────────────────────────

fn affects_shape_geometry(property: &str) -> bool {
    matches!(property,
        "from" | "to" | "radius_x" | "radius_y" | "size"
        | "points" | "commands"
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
        Diagnostic::error(
            DiagnosticCode::UnsupportedAssignmentProperty,
            DiagnosticPhase::Build,
            format!(
                "Assignment property '{property}' on '{target_key}' is not part of the current runtime assignment surface; ignoring this assignment."
            ),
        )
        .with_subject(subject),
    );
}
