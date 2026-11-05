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
use crate::timeline::property_engine::parse_property_value;
use crate::timeline::property_registry::{PropertyFlags, lookup_property};
use crate::timeline::property_track::TrackAccessor;

mod effects;
mod position;
mod rebuild;

use effects::write_effect_stage_param;
use rebuild::{
    affects_shape_geometry, handle_size_assignment,
    push_unsupported_assignment_property_diagnostic, rebuild_vector_paths, scale_plot_curve_paths,
};

/// Build-fixed inputs of one assignment statement, resolved once before any
/// mutable component borrow by [`Timeline::prepare_assignment`].
struct AssignmentPlan {
    target_key: String,
    subject: String,
    eval_env: Environment,
    /// Build time of the statement in milliseconds (the un-offset clock the
    /// primitive context carries alongside `t_start_ms`).
    time_ms: u64,
    func_blend_mode: crate::timeline::plot::FuncBlendMode,
    t_start_ms: u64,
    t_end_ms: u64,
    duration_ms: f64,
    instant_delayed: bool,
    easing: Easing,
    is_size_property: bool,
    child_labels: Vec<String>,
    coordinate_frame: Option<crate::timeline::CoordinateFrameComponent>,
    caps: animatix_core::caps::ActorCaps,
    actor_type: String,
    track_id: crate::timeline::EntityId,
}

impl Timeline {
    /// Resolve an assignment target path that may traverse the scene hierarchy.
    ///
    /// Delegates to [`crate::timeline::World::resolve_path`], the single
    /// dotted-path resolver shared with action targeting: a direct track named
    /// `"g.vec"` wins, otherwise the path walks the scene hierarchy.
    fn resolve_hierarchical_target(&self, target: &[TargetSegment]) -> Option<String> {
        let segments: Vec<&str> = target.iter().map(TargetSegment::label_str).collect();
        self.world.resolve_path(&segments)
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
        // Resolve the target, timing and the special-case dispatch
        // (effect/scene, variable/camera) that must precede track resolution.
        let Some(plan) = self.prepare_assignment(
            target,
            property,
            value,
            modifiers,
            explicit_easing,
            time_ms,
            diagnostics,
        ) else {
            return;
        };

        // Position/offset need compound-binding handling, then the actor's
        // primitive gets first refusal on the property, and the registry engine
        // handles everything else.
        if matches!(property, "position" | "at") {
            self.assign_position(&plan, value, diagnostics);
            return;
        }
        if property == "offset" {
            self.assign_offset(&plan, value, diagnostics);
            return;
        }
        if self.assign_primitive(&plan, property, value, diagnostics) {
            return;
        }
        self.assign_generic(&plan, property, value, diagnostics);
    }

    /// Resolve everything the per-property steps need before a mutable track
    /// borrow: normalized target segments, timing, the special-case dispatch,
    /// the hierarchy-resolved track key, and the actor context (caps, type,
    /// child labels, coordinate frame).
    ///
    /// `None` means the statement was consumed by a special case or rejected
    /// with a diagnostic.
    fn prepare_assignment(
        &mut self,
        target: &[TargetSegment],
        property: &str,
        value: &super::Expr,
        modifiers: &[super::Modifier],
        explicit_easing: Option<super::Easing>,
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<AssignmentPlan> {
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
            return None;
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
            return None;
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
                return None;
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
        // Resolve coordinate frame and capabilities before mutable track borrows.
        let coordinate_frame = (is_size_property || affects_shape_geometry(property))
            .then(|| self.coordinate_frame_for(&target_key).cloned())
            .flatten();
        let caps = self.world.get_id(&target_key).map(|id| self.world.caps(id)).unwrap_or_default();
        let actor_type = self
            .world
            .get_id(&target_key)
            .map(|id| self.world.actor_type(id).to_string())
            .unwrap_or_default();
        let Some(track_id) = self.world.get_id(&target_key) else {
            tracing::warn!("assignment target '{target_key}' has no track; skipping");
            return None;
        };

        Some(AssignmentPlan {
            target_key,
            subject: assignment_subject,
            eval_env,
            time_ms: time_ms as u64,
            func_blend_mode,
            t_start_ms,
            t_end_ms,
            duration_ms,
            instant_delayed,
            easing,
            is_size_property,
            child_labels,
            coordinate_frame,
            caps,
            actor_type,
            track_id,
        })
    }

    /// Let the actor's primitive handle its own special cases. `true` when the
    /// primitive recognized and consumed the property.
    fn assign_primitive(
        &mut self,
        plan: &AssignmentPlan,
        property: &str,
        value: &super::Expr,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> bool {
        let primitive = self.primitive_registry.find(&plan.actor_type);
        let Some(primitive) = primitive else {
            return false;
        };
        let mut payload = self
            .world
            .payload(plan.track_id)
            .unwrap_or(crate::primitives::PrimitivePayload::None);
        let mut ctx = AssignmentCtx {
            t_start_ms: plan.t_start_ms,
            time_ms: plan.time_ms,
            func_blend_mode: Some(plan.func_blend_mode),
            label: &plan.target_key,
            t_end_ms: plan.t_end_ms,
            easing: plan.easing,
            instant_delayed: plan.instant_delayed,
            duration_ms: plan.duration_ms,
            font_context: self.font_context.as_ref(),
            text_compiler: &mut self.text_compiler.borrow_mut(),
            asset_cache: std::sync::Arc::make_mut(&mut self.asset_cache),
            highlight: self.world.highlight.get_mut(plan.track_id),
            style: self.world.style.try_get_mut(plan.track_id),
            caps: plan.caps,
            geometry: self.world.geometry.try_get_mut(plan.track_id),
            shape: self.world.shape.get_mut(plan.track_id),
            text: self.world.text.get_mut(plan.track_id),
            property_plan: Some(self.world.property_plan.get_mut(plan.track_id)),
            payload: Some(&mut payload),
        };
        if primitive.handle_assignment(
            property,
            value,
            &mut ctx,
            &plan.eval_env,
            diagnostics,
            &plan.subject,
        ) {
            if !matches!(payload, crate::primitives::PrimitivePayload::None) {
                self.world.set_payload(plan.track_id, payload);
            }
            return true;
        }
        false
    }

    /// Schema-registry dispatch: size/geometry special cases, the standard
    /// property-field write, then extension properties and the unknown-property
    /// diagnostic.
    fn assign_generic(
        &mut self,
        plan: &AssignmentPlan,
        property: &str,
        value: &super::Expr,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let schema = lookup_property(property);
        if let Some(schema) = schema {
            // Check if the property is assignable
            if !schema.flags.contains(PropertyFlags::ASSIGNABLE) {
                push_unsupported_assignment_property_diagnostic(
                    diagnostics,
                    &plan.subject,
                    &plan.target_key,
                    property,
                );
                return;
            }
            // Special handling for properties that write to size + layout_size together
            if plan.is_size_property {
                self.assign_size_property(plan, property, value, diagnostics);
                return;
            }
            self.assign_standard_property(plan, property, value, schema, diagnostics);
        } else {
            self.assign_extension_property(plan, property, value, diagnostics);
        }
    }

    /// `size` / `radius_x` / `radius_y`: write both the geometry and shape
    /// halves, then rescale a `Graph`'s plot-curve and tick-label children.
    fn assign_size_property(
        &mut self,
        plan: &AssignmentPlan,
        property: &str,
        value: &super::Expr,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Save parent's old size before assignment (for scaling children)
        let old_half_size =
            self.world.geometry.get(plan.track_id).size.last(DEFAULT_LAYOUT_HALF_SIZE);

        self.world.local_bounds.reset_at(plan.track_id);
        self.world.ensure_shape(plan.track_id);
        let new_half_size = handle_size_assignment(
            &plan.actor_type,
            self.world.style.get(plan.track_id),
            self.world.geometry.get_mut(plan.track_id),
            self.world.shape.get_mut(plan.track_id).expect("shape component"),
            property,
            value,
            &plan.eval_env,
            &plan.subject,
            plan.t_start_ms,
            plan.t_end_ms,
            plan.easing,
            plan.instant_delayed,
            diagnostics,
            plan.coordinate_frame.as_ref(),
        );

        // For Graph actors, also rebuild PlotCurve children whose paths
        // depend on the parent's size (p_size), and scale tick label positions
        // to match the new axis positions.
        let shape_type =
            self.world.shape_or_default(plan.track_id).shape_type.last(ShapeType::Rect);
        if shape_type != ShapeType::Graph || old_half_size == new_half_size {
            return;
        }
        let scale_x = new_half_size[0] / old_half_size[0];
        let scale_y = new_half_size[1] / old_half_size[1];
        for child_label in &plan.child_labels {
            // Capabilities are a `World` projection, not a track field.
            let Some(child_id) = self.world.get_id(child_label) else {
                continue;
            };
            let child_caps = self.world.caps(child_id);
            // Scale PlotCurve paths
            if child_caps.plot_geometry {
                self.world.ensure_shape(child_id);
                scale_plot_curve_paths(
                    self.world.style.get(child_id),
                    self.world.shape.get_mut(child_id).expect("shape component"),
                    scale_x,
                    scale_y,
                    plan.t_start_ms,
                    plan.t_end_ms,
                    plan.easing,
                );
            }
            // Scale tick label positions (Text children named {label}_tick_x_N /
            // _tick_y_N)
            if child_caps.text.is_some()
                && (child_label.contains("_tick_x_") || child_label.contains("_tick_y_"))
            {
                let child_geometry = self.world.geometry.get_mut(child_id);
                let old_pos = child_geometry.position.last([0.0, 0.0]);
                let new_pos = [old_pos[0] * scale_x, old_pos[1] * scale_y];
                let child_has_duration = plan.t_end_ms > plan.t_start_ms;
                if child_has_duration {
                    let start_pos = child_geometry.position.get(plan.t_start_ms, [0.0, 0.0]);
                    child_geometry.position.ensure([0.0, 0.0]).add_keyframe(
                        plan.t_start_ms,
                        start_pos,
                        Easing::Linear,
                    );
                } else if plan.instant_delayed {
                    preserve_instant_delayed_value(&mut child_geometry.position, plan.t_start_ms);
                }
                child_geometry.position.ensure([0.0, 0.0]).add_keyframe(
                    plan.t_end_ms,
                    new_pos,
                    plan.easing,
                );
            }
        }
    }

    /// Standard property write through the registry field table, including the
    /// stroke-only `color` mirror and the shape-geometry rebuild.
    fn assign_standard_property(
        &mut self,
        plan: &AssignmentPlan,
        property: &str,
        value: &super::Expr,
        schema: &crate::timeline::property_registry::PropertySchema,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let stroke_only = plan.caps.shape.is_some_and(|s| s.is_stroke_only());
        let pv = parse_property_value(
            schema.value_type,
            value,
            &plan.eval_env,
            diagnostics,
            &plan.subject,
        );
        let Some(pv) = pv else {
            return;
        };
        let field = schema.field_for_actor(&plan.caps, &plan.actor_type);
        self.world.write_property_field(
            plan.track_id,
            field,
            pv,
            plan.t_start_ms,
            plan.t_end_ms,
            plan.easing,
            diagnostics,
        );
        // For stroke-only shapes, a `color` assignment also sets
        // `stroke_color` — they paint from it and nothing else.
        if property == "color"
            && stroke_only
            && let Some(spv) = parse_property_value(
                schema.value_type,
                value,
                &plan.eval_env,
                diagnostics,
                &plan.subject,
            )
        {
            self.world.write_property_field(
                plan.track_id,
                crate::timeline::ActorField::StrokeColor,
                spv,
                plan.t_start_ms,
                plan.t_end_ms,
                plan.easing,
                diagnostics,
            );
        }

        // If this property affects shape geometry, rebuild vector paths
        if affects_shape_geometry(property) {
            self.world.local_bounds.reset_at(plan.track_id);
            self.world.ensure_shape(plan.track_id);
            rebuild_vector_paths(
                plan.actor_type.as_str(),
                self.world.style.get(plan.track_id),
                self.world.geometry.get(plan.track_id),
                self.world.shape.get_mut(plan.track_id).expect("shape component"),
                plan.t_start_ms,
                plan.t_end_ms,
                plan.easing,
                diagnostics,
                plan.coordinate_frame.as_ref(),
            );
        }
    }

    /// Extension properties registered on this actor type, or the unknown
    /// property diagnostic when neither the registry nor an extension owns it.
    fn assign_extension_property(
        &mut self,
        plan: &AssignmentPlan,
        property: &str,
        value: &super::Expr,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if let Some(ctx) = self.extensions.clone() {
            let spec = ctx.property_spec(&plan.actor_type, property).cloned();
            if let Some(spec) = spec {
                if let Some(pv) = crate::timeline::property_engine::parse_extension_property_value(
                    spec.kind,
                    spec.ty.as_ref(),
                    value,
                    &plan.eval_env,
                    diagnostics,
                    &plan.subject,
                ) {
                    crate::timeline::property_engine::write_extension_property_slot(
                        self.world.property_plan.get_mut(plan.track_id),
                        &ctx,
                        &spec.actor_type,
                        property,
                        pv,
                        plan.t_start_ms,
                        plan.t_end_ms,
                        plan.easing,
                    );
                }
                return;
            }
        }

        // Unknown property — report diagnostic
        push_unsupported_assignment_property_diagnostic(
            diagnostics,
            &plan.subject,
            &plan.target_key,
            property,
        );
    }
}

// ─────────────────────────────────────────────────────────────
// Helper: text / math / code assignments → text paths
// ─────────────────────────────────────────────────────────────

mod special;
mod text;
pub(crate) use text::recompile_text_at_assignment;
