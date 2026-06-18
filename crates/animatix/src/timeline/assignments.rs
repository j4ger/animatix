use super::{
    AnimationTrack, DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE, Diagnostic, Easing, Environment,
    ModifierHost, ParsedTimingModifiers, PositionBinding, ShapeType, Timeline, Value,
    VectorShapeState, VectorShapeStyle, assignment_target_key, best_path_suggestion,
    build_shape_vello_path, build_vector_shape_vello_path, evaluate_expr,
    evaluate_expr_with_lookup_diagnostic, mark_track_manual_position,
    parse_color_in_env_with_lookup_diagnostic, parse_timing_modifiers,
    preserve_discrete_position_state_before, preserve_instant_delayed_value,
    push_unknown_target_path_diagnostic, resolve_position_binding_with_lookup_diagnostic,
    set_track_position_binding,
};
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use crate::primitives::{AssignmentCtx, find_primitive};
use crate::renderer::error::RenderError;
use crate::timeline::VelloPath;
use crate::timeline::build::build_graph_axis_paths;
use crate::timeline::property_engine::{parse_property_value, write_property_field};
use crate::timeline::property_registry::{PropertyFlags, lookup_property};
use crate::timeline::track::TrackAccessor;

impl Timeline {
    /// Resolve an assignment target path that may traverse the scene hierarchy.
    ///
    /// For a path like `["g", "vec"]`:
    /// - First checks if a track named `"g.vec"` exists directly.
    /// - If not, walks the hierarchy: finds `"g"`, checks if `"vec"` is its child,
    ///   and returns `"vec"` as the resolved track key.
    /// - Returns `None` if the path cannot be resolved.
    fn resolve_hierarchical_target(&self, target: &[String]) -> Option<String> {
        if target.is_empty() {
            return None;
        }

        let direct_key = assignment_target_key(target);
        if self.tracks.contains_key(&direct_key) {
            return Some(direct_key);
        }

        if target.len() == 1 {
            return None;
        }

        // Walk the hierarchy
        let mut current = target[0].clone();
        for segment in &target[1..] {
            let track = self.tracks.get(&current)?;
            if track.children.contains(segment) {
                current = segment.clone();
            } else {
                return None;
            }
        }

        Some(current)
    }
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
                format!("Assignment '{property} = ...' must include an actor label.",),
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
                    self.background_color.add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed
                    && t_start_ms > 0
                    && !self.background_color.keyframes.contains_key(&(t_start_ms - 1))
                {
                    let prev_val = self.background_color.evaluate(t_start_ms - 1);
                    self.background_color.add_keyframe(t_start_ms - 1, prev_val, Easing::Linear);
                }
                self.background_color.add_keyframe(t_end_ms, target_color, easing);
            }
            return;
        }

        // ── Resolve target track ──
        // ── Variable field assignment (e.g., `p.x = 30` where `p` is a variable holding an Object)
        if target.len() == 1 {
            let var_name = &target[0];
            // Check if this variable exists as a variable track holding an Object value
            if let Some(current) = self
                .variable_tracks
                .get(var_name)
                .and_then(|track| track.evaluate(time_ms as u64))
            {
                if matches!(current, Value::Object(_, _)) {
                    // Evaluate the assignment value
                    let eval_val = match evaluate_expr(value, &eval_env) {
                        Ok(v) => v,
                        Err(e) => {
                            diagnostics.push(Diagnostic::error(
                                DiagnosticCode::InvalidAssignmentTarget,
                                DiagnosticPhase::Build,
                                format!(
                                    "Failed to evaluate value for '{}.{}': {}",
                                    var_name, property, e
                                ),
                            ));
                            return;
                        },
                    };
                    // Update the Object's field (immutable update via with_field)
                    let new_obj = current.with_field(property, eval_val);
                    self.variable_tracks
                        .entry(var_name.clone())
                        .or_default()
                        .keyframes
                        .insert(time_ms as u64, new_obj);
                    return;
                }
            }
        }

        let target_key = match self.resolve_hierarchical_target(target) {
            Some(key) => key,
            None => {
                let suggestion = best_path_suggestion(
                    &assignment_target_key(target),
                    self.tracks.keys().map(String::as_str),
                );
                push_unknown_target_path_diagnostic(
                    diagnostics,
                    &assignment_subject,
                    &assignment_target_key(target),
                    suggestion,
                );
                return;
            },
        };

        let track = self
            .tracks
            .entry(target_key.clone())
            .or_insert_with(|| AnimationTrack::new(target_key.clone()));

        // ── Special cases that can't go through the generic engine ──

        // Position / at — uses position binding resolution (compound property)
        if matches!(property, "position" | "at") {
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
                track.position.ensure(default_pos).add_keyframe(
                    t_start_ms,
                    start_val,
                    Easing::Linear,
                );
            } else if instant_delayed {
                preserve_instant_delayed_value(&mut track.position, t_start_ms);
            }
            track.position.ensure(default_pos).add_keyframe(t_end_ms, target_pos, easing);
            return;
        }

        // ── Primitive dispatch: let each primitive handle its own special cases ──
        let type_name = super::actor_kind_meta(track.kind).map(|m| m.type_name);
        let primitive = type_name.and_then(find_primitive);
        if let Some(primitive) = primitive {
            let mut ctx = AssignmentCtx {
                t_start_ms,
                t_end_ms,
                easing,
                instant_delayed,
                duration_ms,
                font_context: self.font_context.as_ref(),
                text_compiler: &mut self.text_compiler.borrow_mut(),
            };
            if primitive.handle_assignment(
                track,
                property,
                value,
                &mut ctx,
                &eval_env,
                diagnostics,
                &assignment_subject,
            ) {
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
                    diagnostics,
                    &assignment_subject,
                    &target_key,
                    property,
                );
                return;
            }

            // Special handling for properties that write to size + layout_size together
            if is_size_property {
                // Save parent's old size before assignment (for scaling children)
                let old_half_size = track.size.last(DEFAULT_LAYOUT_HALF_SIZE);

                let new_half_size = handle_size_assignment(
                    track,
                    property,
                    value,
                    &eval_env,
                    &assignment_subject,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    instant_delayed,
                    diagnostics,
                );

                // For Graph actors, also rebuild PlotCurve children whose
                // paths depend on the parent's size (p_size), and scale
                // tick label positions to match the new axis positions.
                let shape_type = track.shape.shape_type.last(ShapeType::Rect);
                if shape_type == ShapeType::Graph && old_half_size != new_half_size {
                    let scale_x = new_half_size[0] / old_half_size[0];
                    let scale_y = new_half_size[1] / old_half_size[1];
                    let children: Vec<String> = track.children.clone();
                    for child_label in &children {
                        if let Some(child_track) = self.tracks.get_mut(child_label) {
                            // Scale PlotCurve paths
                            if child_track.kind == super::ActorKindId::PlotCurve {
                                scale_plot_curve_paths(
                                    child_track,
                                    scale_x,
                                    scale_y,
                                    t_start_ms,
                                    t_end_ms,
                                    easing,
                                );
                            }
                            // Scale tick label positions (Text children named {label}_tick_x_N / _tick_y_N)
                            if child_track.kind == super::ActorKindId::Text
                                && (child_label.contains("_tick_x_")
                                    || child_label.contains("_tick_y_"))
                            {
                                let old_pos = child_track.position.last([0.0, 0.0]);
                                let new_pos = [old_pos[0] * scale_x, old_pos[1] * scale_y];
                                let child_has_duration = t_end_ms > t_start_ms;
                                if child_has_duration {
                                    let start_pos =
                                        child_track.position.get(t_start_ms, [0.0, 0.0]);
                                    child_track.position.ensure([0.0, 0.0]).add_keyframe(
                                        t_start_ms,
                                        start_pos,
                                        Easing::Linear,
                                    );
                                } else if instant_delayed {
                                    preserve_instant_delayed_value(
                                        &mut child_track.position,
                                        t_start_ms,
                                    );
                                }
                                child_track
                                    .position
                                    .ensure([0.0, 0.0])
                                    .add_keyframe(t_end_ms, new_pos, easing);
                            }
                        }
                    }
                }

                return;
            }

            // Standard engine dispatch for everything else
            if let Some(pv) = parse_property_value(
                schema.value_type,
                value,
                &eval_env,
                diagnostics,
                &assignment_subject,
            ) {
                write_property_field(
                    track,
                    schema.field,
                    pv,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    diagnostics,
                );

                // For Line actors, `color` assignment also sets `stroke_color` (Line is stroke-only)
                if property == "color"
                    && track.kind == super::ActorKindId::Shape(super::ShapeKind::Line)
                {
                    if let Some(spv) = parse_property_value(
                        schema.value_type,
                        value,
                        &eval_env,
                        diagnostics,
                        &assignment_subject,
                    ) {
                        write_property_field(
                            track,
                            crate::timeline::ActorField::StrokeColor,
                            spv,
                            t_start_ms,
                            t_end_ms,
                            easing,
                            diagnostics,
                        );
                    }
                }

                // If this property affects shape geometry, rebuild vector paths
                if affects_shape_geometry(property) {
                    rebuild_vector_paths(
                        track,
                        t_start_ms,
                        t_end_ms,
                        easing,
                        diagnostics,
                        Some(&eval_env),
                    );
                }
            }
        } else {
            // Check if this is a plot parameter assignment
            let is_plot_param = track
                .procedural_plot
                .as_ref()
                .map(|p| p.param_names.iter().any(|n| n == property))
                .unwrap_or(false);

            if is_plot_param {
                let target_val = match evaluate_expr(value, &eval_env) {
                    Ok(Value::Num(n)) => n,
                    Ok(_) => {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode::InvalidPropertyValue,
                                DiagnosticPhase::Build,
                                format!(
                                    "Plot parameter '{}' on '{}' must be numeric",
                                    property, target_key
                                ),
                            )
                            .with_subject(&assignment_subject),
                        );
                        return;
                    },
                    Err(e) => {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode::InvalidPropertyValue,
                                DiagnosticPhase::Build,
                                format!(
                                    "Failed to evaluate plot parameter '{}.{}': {}",
                                    target_key, property, e
                                ),
                            )
                            .with_subject(&assignment_subject),
                        );
                        return;
                    },
                };

                let has_duration = t_end_ms > t_start_ms;
                let prop_name = property.to_string();
                let param_track = track
                    .plot_param_tracks
                    .entry(prop_name)
                    .or_insert_with(|| super::PropertyTrack::new(target_val));

                if has_duration {
                    let start_val = param_track.evaluate(t_start_ms);
                    param_track.add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed && t_start_ms > 0 {
                    let prev_val = param_track.evaluate(t_start_ms - 1);
                    param_track.add_keyframe(t_start_ms - 1, prev_val, Easing::Linear);
                }
                param_track.add_keyframe(t_end_ms, target_val, easing);
            } else {
                // Unknown property — report diagnostic
                push_unsupported_assignment_property_diagnostic(
                    diagnostics,
                    &assignment_subject,
                    &target_key,
                    property,
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
) -> [f32; 2] {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let has_duration = t_end_ms > t_start_ms;

    let target_size = match property {
        "size" => {
            if let Some(Value::Vec2([w, h])) =
                evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
            {
                [w as f32 / 2.0, h as f32 / 2.0]
            } else {
                track.size.last(default_size)
            }
        },
        "radius_x" => {
            let mut s = track.size.last(default_size);
            s[0] = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .map(|v| v.as_num() as f32)
                .unwrap_or(s[0]);
            s
        },
        "radius_y" => {
            let mut s = track.size.last(default_size);
            s[1] = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .map(|v| v.as_num() as f32)
                .unwrap_or(s[1]);
            s
        },
        _ => track.size.last(default_size),
    };

    if has_duration {
        let start_val = track.size.get(t_start_ms, default_size);
        track
            .size
            .ensure(default_size)
            .add_keyframe(t_start_ms, start_val, Easing::Linear);
        if let Some(layout_start) = track.layout_size_get(t_start_ms) {
            track.ensure_layout_size(default_size).add_keyframe(
                t_start_ms,
                layout_start,
                Easing::Linear,
            );
        }
    } else if instant_delayed {
        preserve_instant_delayed_value(&mut track.size, t_start_ms);
        preserve_instant_delayed_value(&mut track.layout_size, t_start_ms);
    }
    track.size.ensure(default_size).add_keyframe(t_end_ms, target_size, easing);
    track
        .ensure_layout_size(default_size)
        .add_keyframe(t_end_ms, target_size, easing);

    // Rebuild vector paths after size change
    rebuild_vector_paths(track, t_start_ms, t_end_ms, easing, diagnostics, Some(env));

    target_size
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
) -> Result<(), RenderError> {
    if duration_ms > 0.0 {
        let start_val = track.text.text_content.get(t_start_ms, String::new());
        track.text.text_content.ensure(String::new()).add_keyframe(
            t_start_ms,
            start_val,
            Easing::Linear,
        );
    } else if instant_delayed {
        preserve_instant_delayed_value(&mut track.text.text_content, t_start_ms);
    }
    track
        .text
        .text_content
        .ensure(String::new())
        .add_keyframe(t_end_ms, target_text.clone(), easing);

    let text_kind = match track.kind {
        super::ActorKindId::Text => crate::renderer::text::TextKind::Text,
        super::ActorKindId::Code => crate::renderer::text::TextKind::Code,
        super::ActorKindId::Typst => crate::renderer::text::TextKind::Typst,
        _ => return Ok(()),
    };

    let font_family = track.text.font_family.get(t_end_ms, String::new());
    let font_size = track.text.font_size.get(t_end_ms, 48.0);
    let font_weight = track.text.font_weight.get(t_end_ms, 400.0);
    let font_style = track.text.font_style.get(t_end_ms, "normal".to_string());
    let line_height = track.text.line_height.get(t_end_ms, 1.2);
    let letter_spacing = track.text.letter_spacing.get(t_end_ms, 0.0);
    let word_spacing = track.text.word_spacing.get(t_end_ms, 0.0);
    let color = track.color.get(t_end_ms, [1.0, 1.0, 1.0, 1.0]);

    let new_paths = text_compiler.compile(
        &target_text,
        &font_family,
        font_size,
        font_weight,
        &font_style,
        line_height,
        letter_spacing,
        word_spacing,
        color,
        text_kind,
        font_ctx,
        0.0,
        "left",
        "visible",
    )?;
    let new_half_size = crate::renderer::text::measure_text_paths(&new_paths);

    if duration_ms > 0.0 {
        let start_val = track.evaluate_text_paths(t_start_ms);
        track
            .text
            .text_paths
            .ensure(Vec::new())
            .add_keyframe(t_start_ms, start_val, Easing::Linear);
        let start_size = track.size.get(t_start_ms, DEFAULT_LAYOUT_HALF_SIZE);
        let start_layout_size =
            track.layout_size_get(t_start_ms).unwrap_or(DEFAULT_LAYOUT_HALF_SIZE);
        track.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
            t_start_ms,
            start_size,
            Easing::Linear,
        );
        track.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
            t_start_ms,
            start_layout_size,
            Easing::Linear,
        );
    } else if instant_delayed {
        preserve_instant_delayed_value(&mut track.text.text_paths, t_start_ms);
        preserve_instant_delayed_value(&mut track.size, t_start_ms);
        preserve_instant_delayed_value(&mut track.layout_size, t_start_ms);
    }

    // Compute font metrics for baseline alignment
    // Re-compile the text to extract metrics (separate from cached paths)
    let typst_color_for_metrics = typst::visualize::Color::from_u8(
        (color[0] * 255.0) as u8,
        (color[1] * 255.0) as u8,
        (color[2] * 255.0) as u8,
        (color[3] * 255.0) as u8,
    );
    let (ascent, descent, baseline_offset) = match text_kind {
        crate::renderer::text::TextKind::Text => {
            match crate::renderer::text::compile_text(
                &target_text, font_size, typst_color_for_metrics, &font_family,
                font_ctx, font_weight, &font_style, line_height,
                letter_spacing, word_spacing, 0.0, "left", "visible",
            ) {
                Ok(frame) => {
                    let m = crate::renderer::text::extract_glyphs_with_metrics(&frame);
                    (m.ascent, m.descent, m.baseline_offset)
                },
                Err(_) => (0.0, 0.0, 0.0),
            }
        },
        crate::renderer::text::TextKind::Typst => {
            match crate::renderer::text::compile_typst(
                &target_text, font_size, typst_color_for_metrics, &font_family,
                font_ctx, font_weight, &font_style, line_height,
                letter_spacing, word_spacing, 0.0, "left", "visible",
            ) {
                Ok(frame) => {
                    let m = crate::renderer::text::extract_glyphs_with_metrics(&frame);
                    (m.ascent, m.descent, m.baseline_offset)
                },
                Err(_) => (0.0, 0.0, 0.0),
            }
        },
        crate::renderer::text::TextKind::Code => {
            match crate::renderer::text::compile_code(
                &target_text, font_size, typst_color_for_metrics, &font_family,
                font_ctx, font_weight, &font_style, line_height,
                letter_spacing, word_spacing, 0.0, "left", "visible",
            ) {
                Ok(frame) => {
                    let m = crate::renderer::text::extract_glyphs_with_metrics(&frame);
                    (m.ascent, m.descent, m.baseline_offset)
                },
                Err(_) => (0.0, 0.0, 0.0),
            }
        },
        crate::renderer::text::TextKind::Math => {
            match crate::renderer::text::compile_math(
                &target_text, font_size, typst_color_for_metrics, &font_family,
                font_ctx, 0.0, "left", "visible",
            ) {
                Ok(frame) => {
                    let m = crate::renderer::text::extract_glyphs_with_metrics(&frame);
                    (m.ascent, m.descent, m.baseline_offset)
                },
                Err(_) => (0.0, 0.0, 0.0),
            }
        },
    };

    track
        .text
        .text_paths
        .ensure(Vec::new())
        .add_keyframe(t_end_ms, new_paths.to_vec(), easing);
    track
        .size
        .ensure(DEFAULT_LAYOUT_HALF_SIZE)
        .add_keyframe(t_end_ms, new_half_size, easing);
    track.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
        t_end_ms,
        new_half_size,
        easing,
    );
    track.set_metrics(t_end_ms, ascent, descent, baseline_offset);
    Ok(())
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
    env: Option<&Environment>,
) {
    let default_size = DEFAULT_LAYOUT_HALF_SIZE;
    let default_arc = [0.0, 0.0];
    let has_duration = t_end_ms > t_start_ms;
    let size = track.size.last(default_size);
    let shape_type = track.shape.shape_type.last(ShapeType::Rect);

    // ── Special case: Graph actors rebuild axis/grid/tick paths ──
    if shape_type == ShapeType::Graph {
        if let Some(env) = env {
            let label = &track.label;
            let x_domain = env
                .get(&format!("{}_x_domain", label))
                .and_then(|v| {
                    if let Value::Vec2(d) = v {
                        Some(d)
                    } else {
                        None
                    }
                })
                .unwrap_or([-10.0, 10.0]);
            let y_domain = env
                .get(&format!("{}_y_domain", label))
                .and_then(|v| {
                    if let Value::Vec2(d) = v {
                        Some(d)
                    } else {
                        None
                    }
                })
                .unwrap_or([-10.0, 10.0]);
            let stroke_color = track.stroke_color.last(DEFAULT_WHITE);

            // Read graph axis settings from env (stored during build)
            let grid = env
                .get(&format!("{}_grid", label))
                .and_then(|v| {
                    if let Value::Bool(b) = v {
                        Some(b)
                    } else {
                        None
                    }
                })
                .unwrap_or(false);
            let ticks = env
                .get(&format!("{}_ticks", label))
                .and_then(|v| {
                    if let Value::Bool(b) = v {
                        Some(b)
                    } else {
                        None
                    }
                })
                .unwrap_or(false);
            let tick_labels_str = env
                .get(&format!("{}_tick_labels", label))
                .and_then(|v| {
                    if let Value::Str(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "auto".to_string());
            let has_labels = matches!(tick_labels_str.as_str(), "auto" | "true" | "both");

            let new_paths = build_graph_axis_paths(
                size,
                x_domain,
                y_domain,
                stroke_color,
                grid,
                ticks,
                has_labels,
            );

            if has_duration {
                let start_paths = track.evaluate_vector_paths(t_start_ms);
                track.shape.vector_paths.ensure(Vec::new()).add_keyframe(
                    t_start_ms,
                    start_paths,
                    Easing::Linear,
                );
            } else if t_end_ms > 0 {
                preserve_instant_delayed_value(&mut track.shape.vector_paths, t_end_ms);
            }
            track.shape.vector_paths.ensure(Vec::new()).add_keyframe(t_end_ms, new_paths, easing);
            return;
        }
    }

    let line_from = track.shape.line_from.last([-50.0, 0.0]);
    let line_to = track.shape.line_to.last([50.0, 0.0]);
    let arc_angles = track.shape.arc_angles.last(default_arc);
    let color = track.color.last(DEFAULT_WHITE);
    let stroke_width = track.stroke_width.last(2.0);
    let stroke_color = track.stroke_color.last(DEFAULT_WHITE);
    let fill_opacity = track.fill_opacity.last(1.0);

    // Build vector shape state and compute paths
    let mut vector_shape_state = VectorShapeState::new(shape_type, size);
    // Restore shape-specific fields from track data
    match &mut vector_shape_state {
        VectorShapeState::Line(line) => {
            line.line_from = track.shape.line_from.last([-50.0, 0.0]);
            line.line_to = track.shape.line_to.last([50.0, 0.0]);
        },
        VectorShapeState::Arrow(arrow) => {
            arrow.from = track.shape.line_from.last([-50.0, 0.0]);
            arrow.to = track.shape.line_to.last([50.0, 0.0]);
            arrow.head_size = track.shape.head_size.last(10.0);
        },
        VectorShapeState::Polygon(poly) => {
            // Restore points for Polygon actors
            poly.points = track.shape.points.last(Vec::new());
            if !poly.points.is_empty() {
                use crate::timeline::KurboShape;
                let pts: Vec<kurbo::Point> = poly
                    .points
                    .iter()
                    .map(|&[x, y]| kurbo::Point::new(x as f64, y as f64))
                    .collect();
                poly.custom_path = Some(KurboShape::Polygon { points: pts }.to_path_default());
            }
        },
        VectorShapeState::Path(path_state) => {
            // Restore commands for Path actors
            let commands_svg = track.shape.commands.last(String::new());
            if !commands_svg.is_empty() {
                if let Ok(path) = kurbo::BezPath::from_svg(&commands_svg) {
                    path_state.custom_path = Some(path);
                }
            }
        },
        _ => {},
    }
    let target_vello_path = build_vector_shape_vello_path(
        shape_type,
        &vector_shape_state,
        VectorShapeStyle {
            color,
            stroke_width,
            stroke_color,
            fill_opacity,
            line_cap: 0,
            line_join: 0,
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

    if has_duration {
        let start_paths = track.evaluate_vector_paths(t_start_ms);
        track
            .shape
            .vector_paths
            .ensure(Vec::new())
            .add_keyframe(t_start_ms, start_paths, Easing::Linear);
    } else if t_end_ms > 0 {
        preserve_instant_delayed_value(&mut track.shape.vector_paths, t_end_ms);
    }
    track
        .shape
        .vector_paths
        .ensure(Vec::new())
        .add_keyframe(t_end_ms, vec![target_vello_path], easing);
}

// ─────────────────────────────────────────────────────────────
// Helper: scale PlotCurve paths when parent Graph resizes
// ─────────────────────────────────────────────────────────────

fn scale_plot_curve_paths(
    track: &mut AnimationTrack,
    scale_x: f32,
    scale_y: f32,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
) {
    // Skip if scale is identity
    if (scale_x - 1.0).abs() < f32::EPSILON && (scale_y - 1.0).abs() < f32::EPSILON {
        return;
    }

    let has_duration = t_end_ms > t_start_ms;
    let current_paths = track.evaluate_vector_paths(t_end_ms);
    if current_paths.is_empty() {
        return;
    }

    let scale_transform = kurbo::Affine::scale_non_uniform(scale_x as f64, scale_y as f64);
    let scaled_paths: Vec<VelloPath> = current_paths
        .into_iter()
        .map(|mut vp| {
            vp.path = scale_transform * vp.path;
            vp
        })
        .collect();

    if has_duration {
        let start_paths = track.evaluate_vector_paths(t_start_ms);
        track
            .shape
            .vector_paths
            .ensure(Vec::new())
            .add_keyframe(t_start_ms, start_paths, Easing::Linear);
    } else if t_end_ms > 0 {
        preserve_instant_delayed_value(&mut track.shape.vector_paths, t_end_ms);
    }
    track
        .shape
        .vector_paths
        .ensure(Vec::new())
        .add_keyframe(t_end_ms, scaled_paths, easing);
}

// ─────────────────────────────────────────────────────────────
// Helper: does this property affect shape geometry?
// ─────────────────────────────────────────────────────────────

fn affects_shape_geometry(property: &str) -> bool {
    matches!(
        property,
        "from" | "to" | "radius_x" | "radius_y" | "size" | "points" | "commands" | "shape_type"
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
