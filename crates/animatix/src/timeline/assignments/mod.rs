use super::lookup::parse_numeric_vec2_with_lookup_diagnostic;
use super::{
    DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE, Diagnostic, Easing, Environment, ModifierHost,
    ParsedTimingModifiers, PositionBinding, ShapeType, Timeline, Value, VectorShapeState,
    VectorShapeStyle, assignment_target_key, best_path_suggestion, build_shape_vello_path,
    build_vector_shape_vello_path, evaluate_expr, evaluate_expr_with_lookup_diagnostic,
    mark_track_manual_position, parse_color_in_env_with_lookup_diagnostic, parse_timing_modifiers,
    preserve_discrete_position_state_before, preserve_instant_delayed_value,
    push_unknown_target_path_diagnostic, resolve_position_binding_with_lookup_diagnostic,
    set_track_position_binding,
};
use crate::ast::TargetSegment;
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use crate::primitives::AssignmentCtx;
use crate::renderer::error::RenderError;
use crate::timeline::VelloPath;
use crate::timeline::build::build_graph_axis_paths;
use crate::timeline::plot::FuncSource;
use crate::timeline::property_engine::parse_property_value;
use crate::timeline::property_registry::{PropertyFlags, lookup_property};
use crate::timeline::property_track::TrackAccessor;

mod effects;
mod rebuild;

use effects::write_effect_stage_param;
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

        let direct_key = assignment_target_key(target);
        if self.world.tracks.contains_key(&direct_key) {
            return Some(direct_key);
        }

        if target.len() == 1 {
            return None;
        }

        // Walk the hierarchy — all segments must be static at build time.
        let mut current = target[0].label_str().to_string();
        for segment in &target[1..] {
            let seg_str = segment.label_str();
            let current_id = self.world.get_id(&current)?;
            let next = self.world.child_labels(current_id).find(|child| *child == seg_str)?;
            current = next.to_string();
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

        if self.assign_effect_or_scene(
            &target,
            property,
            value,
            &eval_env,
            t_start_ms,
            t_end_ms,
            duration_ms,
            instant_delayed,
            easing,
            diagnostics,
            &assignment_subject,
        ) {
            return;
        }
        if self.assign_variable_or_camera(
            &target,
            property,
            value,
            modifiers,
            &eval_env,
            time_ms,
            t_start_ms,
            t_end_ms,
            easing,
            diagnostics,
            &assignment_subject,
        ) {
            return;
        }
        let target_key = match self.resolve_hierarchical_target(&target) {
            Some(key) => key,
            None => {
                let suggestion = best_path_suggestion(
                    &assignment_target_key(&target),
                    self.world.tracks.keys().map(String::as_str),
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

        // The target was resolved above, so its track must exist; never create
        // one with no primitive identity. Child labels are collected before the
        // mutable borrow because the Graph size path rewrites child tracks.
        let is_size_property = matches!(property, "size" | "radius_x" | "radius_y");
        let child_labels: Vec<String> = if is_size_property {
            self.child_labels(&target_key).map(str::to_string).collect()
        } else {
            Vec::new()
        };
        // Path rebuilds need the host's coordinate frame, and both it and the
        // track live in `World`. Resolve the immutable frame (a small clone)
        // before the mutable track borrow instead of fighting the borrow
        // checker with a second `&World`/`&Timeline`.
        let coordinate_frame = (is_size_property || affects_shape_geometry(property))
            .then(|| self.coordinate_frame_for(&target_key).cloned())
            .flatten();
        // Capabilities live in `World`, so resolve them before the mutable
        // track borrow (the projection is `Copy`).
        let caps = self.world.get_id(&target_key).map(|id| self.world.caps(id)).unwrap_or_default();
        let actor_type = self
            .world
            .get_id(&target_key)
            .map(|id| self.world.actor_type(id).to_string())
            .unwrap_or_default();
        let Some(track_id) = self.world.get_id(&target_key) else {
            tracing::warn!("assignment target '{target_key}' has no track; skipping");
            return;
        };

        // ── Special cases that can't go through the generic engine ──

        // Position / at — uses position binding resolution (compound property)
        if matches!(property, "position" | "at") {
            let default_pos = [0.0, 0.0];
            let default_binding = PositionBinding::Absolute;
            let geometry = self.world.geometry.get_mut(track_id);
            let target_pos = if let Some((binding, position)) =
                resolve_position_binding_with_lookup_diagnostic(
                    Some(value),
                    None,
                    None,
                    &eval_env,
                    diagnostics,
                    &assignment_subject,
                ) {
                preserve_discrete_position_state_before(geometry, t_start_ms);
                if instant_delayed {
                    preserve_instant_delayed_value(&mut geometry.position, t_start_ms);
                }
                mark_track_manual_position(geometry, t_start_ms);
                if duration_ms > 0.0 {
                    let start_binding = geometry.position_binding.get(t_start_ms, default_binding);
                    geometry.position_binding.ensure(default_binding).add_keyframe(
                        t_start_ms,
                        start_binding,
                        Easing::Linear,
                    );
                    geometry
                        .position_binding
                        .ensure(default_binding)
                        .add_keyframe(t_end_ms, binding, easing);
                } else {
                    set_track_position_binding(geometry, t_start_ms, binding);
                }
                position.unwrap_or_else(|| geometry.position.last(default_pos))
            } else {
                geometry.position.last(default_pos)
            };
            if duration_ms > 0.0 {
                let start_val = geometry.position.get(t_start_ms, default_pos);
                geometry.position.ensure(default_pos).add_keyframe(
                    t_start_ms,
                    start_val,
                    Easing::Linear,
                );
            } else if instant_delayed {
                preserve_instant_delayed_value(&mut geometry.position, t_start_ms);
            }
            geometry.position.ensure(default_pos).add_keyframe(t_end_ms, target_pos, easing);
            return;
        }

        // Offset — rewrites the offset component of the *current* position
        // binding, keeping the anchor or percentage base it hangs off. An
        // offset only exists relative to an anchor/percent base, so on an
        // absolute or container-default binding it is ignored with the same
        // `IgnoredOffset` warning the declaration path uses.
        if property == "offset" {
            let Some(new_offset) = parse_numeric_vec2_with_lookup_diagnostic(
                value,
                &eval_env,
                diagnostics,
                &assignment_subject,
            ) else {
                return;
            };
            let default_binding = PositionBinding::Absolute;
            let geometry = self.world.geometry.get_mut(track_id);
            let current = geometry.position_binding.get(t_start_ms, default_binding);
            let rebased = match current {
                PositionBinding::SceneAnchor { anchor, .. } => PositionBinding::SceneAnchor {
                    anchor,
                    offset: new_offset,
                },
                PositionBinding::ScenePercent { x, y, .. } => PositionBinding::ScenePercent {
                    x,
                    y,
                    offset: new_offset,
                },
                PositionBinding::Absolute
                | PositionBinding::ContainerDefault { .. }
                | PositionBinding::ContainerPercent { .. } => {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::IgnoredOffset,
                            DiagnosticPhase::Build,
                            format!(
                                "`offset` has no effect on '{assignment_subject}' without `at`/`anchor`; the value is ignored.",
                            ),
                        )
                        .with_subject(&assignment_subject),
                    );
                    return;
                },
            };
            preserve_discrete_position_state_before(geometry, t_start_ms);
            mark_track_manual_position(geometry, t_start_ms);
            if duration_ms > 0.0 {
                let start_binding = geometry.position_binding.get(t_start_ms, default_binding);
                geometry.position_binding.ensure(default_binding).add_keyframe(
                    t_start_ms,
                    start_binding,
                    Easing::Linear,
                );
                geometry
                    .position_binding
                    .ensure(default_binding)
                    .add_keyframe(t_end_ms, rebased, easing);
            } else {
                set_track_position_binding(geometry, t_start_ms, rebased);
            }
            return;
        }

        // ── Primitive dispatch: let each primitive handle its own special cases ──
        let primitive = self.primitive_registry.find(&actor_type);
        if let Some(primitive) = primitive {
            let mut ctx = AssignmentCtx {
                label: &target_key,
                t_start_ms,
                t_end_ms,
                easing,
                instant_delayed,
                duration_ms,
                font_context: self.font_context.as_ref(),
                text_compiler: &mut self.text_compiler.borrow_mut(),
                asset_cache: std::sync::Arc::make_mut(&mut self.asset_cache),
                highlight: self.world.highlight.try_get_mut(track_id),
                style: self.world.style.try_get_mut(track_id),
                caps,
                geometry: self.world.geometry.try_get_mut(track_id),
                shape: self.world.shape.try_get_mut(track_id),
                text: self.world.text.try_get_mut(track_id),
                property_plan: Some(self.world.property_plan.get_mut(track_id)),
                payload: Some(
                    self.world
                        .payload
                        .get_or_insert_with(track_id, crate::primitives::PrimitivePayload::default),
                ),
            };
            if primitive.handle_assignment(
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
        let is_plot_actor = caps.plot_geometry;
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
            let from_source = if let Some(active) = self.world.func_transitions(track_id).last() {
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
            } else if let Some(plot) = self.world.procedural_plot(track_id) {
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
            self.world
                .func_transitions_mut(track_id)
                .push(crate::timeline::plot::FuncTransition {
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
                let old_half_size =
                    self.world.geometry.get(track_id).size.last(DEFAULT_LAYOUT_HALF_SIZE);

                let new_half_size = handle_size_assignment(
                    &actor_type,
                    self.world.style.get(track_id),
                    self.world.geometry.get_mut(track_id),
                    self.world.shape.get_mut(track_id),
                    property,
                    value,
                    &eval_env,
                    &assignment_subject,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    instant_delayed,
                    diagnostics,
                    coordinate_frame.as_ref(),
                );

                // For Graph actors, also rebuild PlotCurve children whose
                // paths depend on the parent's size (p_size), and scale
                // tick label positions to match the new axis positions.
                let shape_type = self.world.shape.get(track_id).shape_type.last(ShapeType::Rect);
                if shape_type == ShapeType::Graph && old_half_size != new_half_size {
                    let scale_x = new_half_size[0] / old_half_size[0];
                    let scale_y = new_half_size[1] / old_half_size[1];
                    for child_label in &child_labels {
                        // Capabilities are a `World` projection, not a track field.
                        let Some(child_id) = self.world.get_id(child_label) else {
                            continue;
                        };
                        let child_caps = self.world.caps(child_id);
                        // Scope the two child rewrites (the old
                        // `tracks.get_mut` guard is now the id resolution
                        // above); they are independent of each other.
                        {
                            // Scale PlotCurve paths
                            if child_caps.plot_geometry {
                                scale_plot_curve_paths(
                                    self.world.style.get(child_id),
                                    self.world.shape.get_mut(child_id),
                                    scale_x,
                                    scale_y,
                                    t_start_ms,
                                    t_end_ms,
                                    easing,
                                );
                            }
                            // Scale tick label positions (Text children named {label}_tick_x_N /
                            // _tick_y_N)
                            if child_caps.text.is_some()
                                && (child_label.contains("_tick_x_")
                                    || child_label.contains("_tick_y_"))
                            {
                                let child_geometry = self.world.geometry.get_mut(child_id);
                                let old_pos = child_geometry.position.last([0.0, 0.0]);
                                let new_pos = [old_pos[0] * scale_x, old_pos[1] * scale_y];
                                let child_has_duration = t_end_ms > t_start_ms;
                                if child_has_duration {
                                    let start_pos =
                                        child_geometry.position.get(t_start_ms, [0.0, 0.0]);
                                    child_geometry.position.ensure([0.0, 0.0]).add_keyframe(
                                        t_start_ms,
                                        start_pos,
                                        Easing::Linear,
                                    );
                                } else if instant_delayed {
                                    preserve_instant_delayed_value(
                                        &mut child_geometry.position,
                                        t_start_ms,
                                    );
                                }
                                child_geometry
                                    .position
                                    .ensure([0.0, 0.0])
                                    .add_keyframe(t_end_ms, new_pos, easing);
                            }
                        }
                    }
                }

                return;
            }

            // Standard engine dispatch for everything else. Capture what the
            // track borrow still owes us, then end it so the write can go
            // through `World` (which needs the whole world mutably).
            let stroke_only = caps.shape.is_some_and(|s| s.is_stroke_only());
            let pv = parse_property_value(
                schema.value_type,
                value,
                &eval_env,
                diagnostics,
                &assignment_subject,
            );
            if let Some(pv) = pv {
                self.world.write_property_field(
                    track_id,
                    schema.field,
                    pv,
                    t_start_ms,
                    t_end_ms,
                    easing,
                    diagnostics,
                );

                // For stroke-only shapes, a `color` assignment also sets
                // `stroke_color` — they paint from it and nothing else.
                if property == "color" && stroke_only {
                    if let Some(spv) = parse_property_value(
                        schema.value_type,
                        value,
                        &eval_env,
                        diagnostics,
                        &assignment_subject,
                    ) {
                        self.world.write_property_field(
                            track_id,
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
                        actor_type.as_str(),
                        self.world.style.get(track_id),
                        self.world.geometry.get(track_id),
                        self.world.shape.get_mut(track_id),
                        t_start_ms,
                        t_end_ms,
                        easing,
                        diagnostics,
                        coordinate_frame.as_ref(),
                    );
                }
            }
        } else {
            // Extension properties registered on this actor type.
            if let Some(ctx) = self.extensions.clone() {
                let spec = ctx.property_spec(&actor_type, property).cloned();
                if let Some(spec) = spec {
                    if let Some(pv) =
                        crate::timeline::property_engine::parse_extension_property_value(
                            spec.kind,
                            spec.ty.as_ref(),
                            value,
                            &eval_env,
                            diagnostics,
                            &assignment_subject,
                        )
                    {
                        crate::timeline::property_engine::write_extension_property_slot(
                            self.world.property_plan.get_mut(track_id),
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
            let is_plot_param = self
                .world
                .procedural_plot(track_id)
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
                let param_track = self
                    .world
                    .ensure_plot_payload(track_id)
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

mod special;
mod text;

pub(crate) use text::recompile_text_at_assignment;
