use super::{
    AnimationTrack, DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE, Diagnostic, Easing, Environment,
    ModifierHost, ParsedTimingModifiers, PositionBinding, ShapeType, Timeline, Value,
    VectorShapeState, VectorShapeStyle, assignment_target_key, best_path_suggestion,
    build_shape_vello_path, build_vector_shape_vello_path, default_stroke_width, evaluate_expr,
    evaluate_expr_with_lookup_diagnostic, mark_track_manual_position,
    parse_color_in_env_with_lookup_diagnostic, parse_timing_modifiers,
    preserve_discrete_position_state_before, preserve_instant_delayed_value,
    push_unknown_target_path_diagnostic, resolve_position_binding_with_lookup_diagnostic,
    set_track_position_binding,
};
use crate::ast::TargetSegment;
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use crate::primitives::AssignmentCtx;
use crate::renderer::error::RenderError;
use crate::timeline::VelloPath;
use crate::timeline::actor_kind::ActorKindId;
use crate::timeline::build::build_graph_axis_paths;
use crate::timeline::plot::FuncSource;
use crate::timeline::property_engine::{parse_property_value, write_property_field};
use crate::timeline::property_registry::{PropertyFlags, lookup_property};
use crate::timeline::property_track::TrackAccessor;

mod rebuild;

use rebuild::{
    affects_shape_geometry, handle_size_assignment,
    push_unsupported_assignment_property_diagnostic, rebuild_vector_paths, scale_plot_curve_paths,
};

impl Timeline {
    /// Resolve an assignment target path that may traverse the scene hierarchy.
    ///
    /// For a path like `["g", "vec"]`:
    /// - First checks if a track named `"g.vec"` exists directly.
    /// - If not, walks the hierarchy: finds `"g"`, checks if `"vec"` is its child, and returns
    ///   `"vec"` as the resolved track key.
    /// - Returns `None` if the path cannot be resolved.
    fn resolve_hierarchical_target(&self, target: &[TargetSegment]) -> Option<String> {
        if target.is_empty() {
            return None;
        }

        let direct_key = assignment_target_key(&target);
        if self.tracks.contains_key(&direct_key) {
            return Some(direct_key);
        }

        if target.len() == 1 {
            return None;
        }

        // Walk the hierarchy — all segments must be static at build time.
        let mut current = target[0].label_str().to_string();
        for segment in &target[1..] {
            let seg_str = segment.label_str();
            let track = self.tracks.get(&current)?;
            if track.children.contains(&seg_str.to_string()) {
                current = seg_str.to_string();
            } else {
                return None;
            }
        }

        Some(current)
    }
    pub(super) fn process_assignment_statement(
        &mut self,
        target: &[TargetSegment],
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
        // Resolve `name[expr]` segments against the build environment before
        // walking the scene hierarchy (loop variables and `let` bindings are
        // constants at build time). Frame-time `always` assignments never pass
        // through here — they are lowered to `AssignIndexed` IR instead.
        let target = if target.iter().any(|s| matches!(s, TargetSegment::Indexed { .. })) {
            target
                .iter()
                .map(|segment| match segment {
                    TargetSegment::Static(s) => TargetSegment::Static(s.clone()),
                    TargetSegment::Indexed { base, index } => {
                        match evaluate_expr(index, &eval_env) {
                            Ok(super::Value::Num(n)) if n >= 0.0 && n == n.floor() => {
                                TargetSegment::Static(crate::ast::array_actor_label(
                                    base, n as usize,
                                ))
                            },
                            Ok(super::Value::Num(n)) => {
                                diagnostics.push(Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    format!(
                                        "Assignment target index for '{}' must be a non-negative integer, got {}",
                                        base, n
                                    ),
                                ));
                                TargetSegment::Static(base.clone())
                            },
                            _ => {
                                diagnostics.push(Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    format!(
                                        "Failed to evaluate assignment target index for '{}' at build time",
                                        base
                                    ),
                                ));
                                TargetSegment::Static(base.clone())
                            },
                        }
                    },
                })
                .collect::<Vec<_>>()
        } else {
            target.to_vec()
        };
        let assignment_subject = format!("{}.{}", assignment_target_key(&target), property);
        let ParsedTimingModifiers {
            duration_ms,
            delay_ms,
            easing: modifier_easing,
            func_blend_mode,
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
        if target.len() == 1 && target[0].label_str() == "scene" {
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
            let var_name = target[0].label_str();
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
                        .entry(var_name.to_string())
                        .or_default()
                        .keyframes
                        .insert(time_ms as u64, new_obj);
                    return;
                }
            }
        }

        let target_key = match self.resolve_hierarchical_target(&target) {
            Some(key) => key,
            None => {
                let suggestion = best_path_suggestion(
                    &assignment_target_key(&target),
                    self.tracks.keys().map(String::as_str),
                );
                push_unknown_target_path_diagnostic(
                    diagnostics,
                    &assignment_subject,
                    &assignment_target_key(&target),
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
                    preserve_instant_delayed_value(&mut track.geometry.position, t_start_ms);
                }
                mark_track_manual_position(track, t_start_ms);
                if duration_ms > 0.0 {
                    let start_binding =
                        track.geometry.position_binding.get(t_start_ms, default_binding);
                    track.geometry.position_binding.ensure(default_binding).add_keyframe(
                        t_start_ms,
                        start_binding,
                        Easing::Linear,
                    );
                    track
                        .geometry
                        .position_binding
                        .ensure(default_binding)
                        .add_keyframe(t_end_ms, binding, easing);
                } else {
                    set_track_position_binding(track, t_start_ms, binding);
                }
                position.unwrap_or_else(|| track.geometry.position.last(default_pos))
            } else {
                track.geometry.position.last(default_pos)
            };
            if duration_ms > 0.0 {
                let start_val = track.geometry.position.get(t_start_ms, default_pos);
                track.geometry.position.ensure(default_pos).add_keyframe(
                    t_start_ms,
                    start_val,
                    Easing::Linear,
                );
            } else if instant_delayed {
                preserve_instant_delayed_value(&mut track.geometry.position, t_start_ms);
            }
            track
                .geometry
                .position
                .ensure(default_pos)
                .add_keyframe(t_end_ms, target_pos, easing);
            return;
        }

        // ── Primitive dispatch: let each primitive handle its own special cases ──
        let type_name = track
            .actor_type
            .as_deref()
            .or_else(|| super::actor_kind_meta(track.kind).map(|m| m.type_name));
        let primitive = type_name.and_then(|ty| self.primitive_registry.find(ty));
        if let Some(primitive) = primitive {
            let mut ctx = AssignmentCtx {
                t_start_ms,
                t_end_ms,
                easing,
                instant_delayed,
                duration_ms,
                font_context: self.font_context.as_ref(),
                text_compiler: &mut self.text_compiler.borrow_mut(),
                asset_cache: std::sync::Arc::make_mut(&mut self.asset_cache),
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

        // ── Func assignment on supported plot actors (special case) ──
        // `func` is a build-time-only AST node, not a registry property.
        // We handle it here so `curve.func = (x) => cos(x) [1s]` creates a
        // FuncTransition that blends function outputs at frame time.
        let is_plot_actor = primitive
            .is_some_and(|primitive| primitive.capabilities().plot_geometry)
            || matches!(
                track.kind,
                ActorKindId::VectorField
                    | ActorKindId::Heatmap
                    | ActorKindId::ContourSet
                    | ActorKindId::PlotCurve
            );
        if property == "func" && is_plot_actor {
            // Evaluate RHS to a closure.
            let closure_val = match evaluate_expr(value, &eval_env) {
                Ok(v) => v,
                Err(e) => {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::InvalidPlotFunc,
                            DiagnosticPhase::Build,
                            format!(
                                "Failed to evaluate func assignment on '{}': {}",
                                target_key, e
                            ),
                        )
                        .with_subject(&assignment_subject),
                    );
                    return;
                },
            };
            let (to_args, to_body, to_captures) = match closure_val {
                Value::Closure(args, body, captures) => (args, *body, captures),
                _ => {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::InvalidPlotFunc,
                            DiagnosticPhase::Build,
                            format!(
                                "func assignment on '{}' must be a closure (e.g. `(x) => expr`)",
                                target_key
                            ),
                        )
                        .with_subject(&assignment_subject),
                    );
                    return;
                },
            };

            // Determine "from" source:
            //   - If there's an active transition, record-and-chain: freeze current blend
            //   - Else use last completed transition's `to`, or the declaration func
            let from_source = if let Some(active) = track.func_transitions.last() {
                if time_ms as u64 >= active.start_ms && time_ms as u64 <= active.end_ms {
                    // Record-and-chain: freeze current blend state
                    let progress = if active.end_ms > active.start_ms {
                        ((time_ms as u64 - active.start_ms) as f64
                            / (active.end_ms - active.start_ms) as f64)
                            .clamp(0.0, 1.0)
                    } else {
                        1.0
                    };
                    FuncSource::Blend {
                        from: Box::new(active.from.clone()),
                        to: Box::new(active.to.clone()),
                        frozen_progress: progress,
                    }
                } else {
                    // Last transition completed, use its `to`
                    active.to.clone()
                }
            } else if let Some(plot) = track.procedural_plot.as_ref() {
                // No prior transitions, use declaration func
                FuncSource::Compiled(
                    plot.func_args.clone(),
                    Box::new(plot.func_body.clone()),
                    plot.extra_captures.clone(),
                )
            } else {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::InvalidPlotFunc,
                        DiagnosticPhase::Build,
                        format!(
                            "Cannot assign func on '{}': no func declared on this plot actor",
                            target_key
                        ),
                    )
                    .with_subject(&assignment_subject),
                );
                return;
            };

            // Validate same arity
            if from_source.arity() != to_args.len() {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::InvalidPlotFunc,
                        DiagnosticPhase::Build,
                        format!("func transition on '{}' must keep the same arity ", target_key),
                    )
                    .with_subject(&assignment_subject),
                );
                return;
            }

            // Push FuncTransition
            track.func_transitions.push(crate::timeline::plot::FuncTransition {
                start_ms: t_start_ms,
                end_ms: t_end_ms,
                easing,
                from: from_source,
                to: FuncSource::Compiled(to_args, Box::new(to_body), to_captures),
                blend_mode: func_blend_mode,
            });
            return; // func is not a registry property; do not fall through
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
                let old_half_size = track.geometry.size.last(DEFAULT_LAYOUT_HALF_SIZE);

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
                            // Scale tick label positions (Text children named {label}_tick_x_N /
                            // _tick_y_N)
                            if child_track.kind == super::ActorKindId::Text
                                && (child_label.contains("_tick_x_")
                                    || child_label.contains("_tick_y_"))
                            {
                                let old_pos = child_track.geometry.position.last([0.0, 0.0]);
                                let new_pos = [old_pos[0] * scale_x, old_pos[1] * scale_y];
                                let child_has_duration = t_end_ms > t_start_ms;
                                if child_has_duration {
                                    let start_pos =
                                        child_track.geometry.position.get(t_start_ms, [0.0, 0.0]);
                                    child_track.geometry.position.ensure([0.0, 0.0]).add_keyframe(
                                        t_start_ms,
                                        start_pos,
                                        Easing::Linear,
                                    );
                                } else if instant_delayed {
                                    preserve_instant_delayed_value(
                                        &mut child_track.geometry.position,
                                        t_start_ms,
                                    );
                                }
                                child_track
                                    .geometry
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

                // For Line actors, `color` assignment also sets `stroke_color` (Line is
                // stroke-only)
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
            // Extension properties registered on this actor type.
            if let Some(ctx) = self.extensions.clone() {
                let actor_type = track.actor_type.clone();
                let spec = actor_type
                    .as_deref()
                    .and_then(|actor_type| ctx.property_spec(actor_type, property))
                    .cloned();
                if let Some(spec) = spec {
                    if let Some(pv) =
                        crate::timeline::property_engine::parse_extension_property_value(
                            spec.kind,
                            value,
                            &eval_env,
                            diagnostics,
                            &assignment_subject,
                        )
                    {
                        crate::timeline::property_engine::write_extension_property_slot(
                            track,
                            &ctx,
                            &spec.actor_type,
                            property,
                            pv,
                            t_start_ms,
                            t_end_ms,
                            easing,
                        );
                    }
                    return;
                }
            }

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
    track.text.text_content.ensure(String::new()).add_keyframe(
        t_end_ms,
        target_text.clone(),
        easing,
    );

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
    let color = track.style.color.get(t_end_ms, [1.0, 1.0, 1.0, 1.0]);

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
        track.text.text_paths.ensure(Vec::new()).add_keyframe(
            t_start_ms,
            start_val,
            Easing::Linear,
        );
        let start_size = track.geometry.size.get(t_start_ms, DEFAULT_LAYOUT_HALF_SIZE);
        let start_layout_size =
            track.layout_size_get(t_start_ms).unwrap_or(DEFAULT_LAYOUT_HALF_SIZE);
        track.geometry.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
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
        preserve_instant_delayed_value(&mut track.geometry.size, t_start_ms);
        preserve_instant_delayed_value(&mut track.geometry.layout_size, t_start_ms);
    }

    // Content swaps cross-fade the source and target glyph sets instead of
    // morphing arbitrary strings through a midpoint string snapshot.
    if duration_ms > 0.0 {
        track
            .style
            .morph_options
            .ensure(crate::timeline::MorphOptions::default())
            .add_keyframe(
                t_end_ms,
                crate::timeline::MorphOptions {
                    strategy: crate::timeline::MorphStrategy::Fade,
                    ..Default::default()
                },
                easing,
            );
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
                &target_text,
                font_size,
                typst_color_for_metrics,
                &font_family,
                font_ctx,
                font_weight,
                &font_style,
                line_height,
                letter_spacing,
                word_spacing,
                0.0,
                "left",
                "visible",
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
                &target_text,
                font_size,
                typst_color_for_metrics,
                &font_family,
                font_ctx,
                font_weight,
                &font_style,
                line_height,
                letter_spacing,
                word_spacing,
                0.0,
                "left",
                "visible",
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
                &target_text,
                font_size,
                typst_color_for_metrics,
                &font_family,
                font_ctx,
                font_weight,
                &font_style,
                line_height,
                letter_spacing,
                word_spacing,
                0.0,
                "left",
                "visible",
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
                &target_text,
                font_size,
                typst_color_for_metrics,
                &font_family,
                font_ctx,
                0.0,
                "left",
                "visible",
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
    track.geometry.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
        t_end_ms,
        new_half_size,
        easing,
    );
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
