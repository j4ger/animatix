//! Mount inert timeline plans into the host timeline's existing ECS stores.
//!
//! Plan values are expanded at build time. This module owns their placement
//! clock, private actor identity remapping, and conversion to the existing
//! property/action writers; it adds no runtime clock or timeline entity.

use std::collections::HashMap;

use animatix_syntax::AstFolder;
use animatix_syntax::ast::{BinaryOp, Expr, InlineItem, Modifier, Span, Stmt, TargetSegment, Time};
use animatix_syntax::visit::{walk_fold_expr, walk_fold_stmt};

use super::*;
use crate::timeline::plan_value::{ActorRef, BoundBuiltinCall, PlanDuration, PlanOp, TimelinePlan};

mod baseline;
mod depth;
mod helpers;
mod statements;
pub(crate) use depth::{MAX_PLAN_EXPANSION_DEPTH, PlanExpansionGuard};
use helpers::{
    builtin_extent_ms, builtin_invocation, collect_plan_actor_roots, contains_private_actor,
    localize_always_body, plan_contains_audio, plan_timing_error, remap_builtin_call,
    remap_capture_values, remap_instance_methods, remap_statements, remap_value,
    restore_plan_scope,
};
pub(crate) use helpers::{estimate_statements_extent_ms, scale_modifiers, statement_span_ms};

static NEXT_MOUNT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Build-only placement context; actor storage stays in the host World.
struct PlanPlacement<'a> {
    plan: &'a TimelinePlan,
    mount_id: u64,
    qualified_labels: &'a HashMap<String, String>,
    origin_ms: f64,
    parent_label: Option<&'a str>,
    inherited_rate: f64,
    span: Option<Span>,
}

impl Timeline {
    /// Evaluate an expression in the build environment and mount a plan result.
    pub(crate) fn mount_plan_expression(
        &mut self,
        expr: &Expr,
        span: Option<Span>,
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let env = self.build_eval_env(time_ms.max(0.0) as u64);
        match evaluate_expr(expr, &env) {
            Ok(Value::TimelinePlan(plan)) => {
                self.mount_plan(
                    &plan,
                    time_ms,
                    parent_label,
                    self.plan_clock_rate,
                    span,
                    diagnostics,
                );
            },
            Ok(Value::Unit) => {},
            Ok(value) => diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("timeline position expects a plan or Unit, got {value:?}"),
                )
                .with_ast_span(span),
            ),
            Err(EvalError::InvalidActorIndex(index)) => diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::CallableEvaluationError,
                    DiagnosticPhase::Build,
                    format!("actor array index must be a non-negative integer, got {index}"),
                )
                .with_ast_span(span),
            ),
            Err(error) => diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::CallableEvaluationError,
                    DiagnosticPhase::Build,
                    format!("failed to evaluate timeline expression: {error}"),
                )
                .with_ast_span(span),
            ),
        }
    }

    /// Record the resolved time of an authored host keyframe marker.
    pub(crate) fn record_resolved_marker(&mut self, span: Option<Span>, time_ms: f64) {
        if let Some(span) = span
            && time_ms.is_finite()
        {
            self.resolved_markers.push((span, time_ms));
        }
    }

    /// Look up a marker time retained during the build walk.
    /// Resolve a marker from the build's recorded source spans, in milliseconds.
    pub fn marker_time(&self, span: Span) -> Option<f64> {
        self.resolved_markers
            .iter()
            .rev()
            .find_map(|(candidate, time_ms)| (*candidate == span).then_some(*time_ms))
    }

    /// Mount one plan value at a concrete scene time. Local actor references
    /// receive a fresh label map for this placement; caller references survive.
    pub(crate) fn mount_plan(
        &mut self,
        plan: &TimelinePlan,
        origin_ms: f64,
        parent_label: Option<&str>,
        inherited_rate: f64,
        span: Option<Span>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<f64> {
        let Some(_depth) = PlanExpansionGuard::enter() else {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::CallableEvaluationError,
                    DiagnosticPhase::Build,
                    format!(
                        "timeline plan expansion exceeded the maximum nesting depth of {MAX_PLAN_EXPANSION_DEPTH}"
                    ),
                )
                .with_ast_span(span),
            );
            return None;
        };
        if !origin_ms.is_finite() || !inherited_rate.is_finite() || inherited_rate <= 0.0 {
            diagnostics.push(plan_timing_error(
                "plan placement requires finite time and a positive scale",
                span,
            ));
            return None;
        }
        if (inherited_rate - 1.0).abs() > f64::EPSILON && plan_contains_audio(plan) {
            diagnostics.push(plan_timing_error(
                "audio segments cannot be stretched or fitted to a different clock rate",
                span,
            ));
            return None;
        }

        let mount_id = NEXT_MOUNT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let roots = collect_plan_actor_roots(plan);
        let labels = roots
            .iter()
            .map(|root| (root.clone(), self.fresh_plan_actor_label(mount_id, root, parent_label)))
            .collect::<HashMap<_, _>>();
        let qualified_labels = labels
            .iter()
            .map(|(source, target)| {
                let target = parent_label
                    .map(|parent| format!("{parent}.{target}"))
                    .unwrap_or_else(|| target.clone());
                (source.clone(), target)
            })
            .collect::<HashMap<_, _>>();
        let captures =
            remap_capture_values(plan.captures(), plan.id(), mount_id, &qualified_labels);
        for label in qualified_labels.values() {
            self.env.set(label, Value::ActorRef(ActorRef::mounted(mount_id, label.clone())));
        }
        let previous_component_methods = self.env.component_methods_snapshot();
        if !plan.instance_methods().is_empty() {
            let methods = remap_instance_methods(
                plan.instance_methods(),
                &qualified_labels,
                &qualified_labels,
            );
            crate::timeline::build::extend_input_methods(
                &mut self.env,
                &methods,
                plan.function_context().map(AsRef::as_ref),
            );
        }
        let previous_rate = self.plan_clock_rate;
        let previous_authored_t = self.env.has_authored_t_binding();
        let previous_source_path = self.env.source_path.clone();
        let previous_document_dir =
            self.asset_cache.document_dir().map(std::path::Path::to_path_buf);
        self.plan_clock_rate = inherited_rate;
        self.env.set_authored_t_binding(plan.has_authored_t_capture());
        if let Some(source_path) = &plan.source_id {
            self.env.set_source_path(Some(std::sync::Arc::clone(source_path)));
            std::sync::Arc::make_mut(&mut self.asset_cache).set_document_dir(source_path.parent());
        }
        self.block_scope.push(HashMap::new());
        for (name, value) in &captures {
            if let Some(scope) = self.block_scope.last_mut() {
                scope
                    .entry(name.clone())
                    .or_insert_with(|| self.env.overrides.get(name).cloned());
            }
            self.env.set(name, value.clone());
        }

        let ownership = crate::timeline::world::PlanOwnership::new(mount_id);
        self.build_plan_ownership.push(ownership);
        let local_end = self.mount_plan_operations(
            PlanPlacement {
                plan,
                mount_id,
                qualified_labels: &qualified_labels,
                origin_ms,
                parent_label,
                inherited_rate,
                span,
            },
            diagnostics,
        );
        let previous_owner = self.build_plan_ownership.pop();
        debug_assert_eq!(previous_owner, Some(ownership));

        restore_plan_scope(self);
        self.env.restore_component_methods(previous_component_methods);
        self.plan_clock_rate = previous_rate;
        self.env.set_authored_t_binding(previous_authored_t);
        self.env.set_source_path(previous_source_path);
        if plan.source_id.is_some() {
            std::sync::Arc::make_mut(&mut self.asset_cache)
                .set_document_dir(previous_document_dir.as_deref());
        }
        let mounted_extent = local_end.map(|end| end * inherited_rate);
        if let Some(end) = mounted_extent
            .map(|extent| origin_ms + extent)
            .filter(|end| end.is_finite() && *end > 0.0)
        {
            self.local_time_scope_end_ms = self.local_time_scope_end_ms.max(end.ceil() as u64);
        }
        mounted_extent
    }

    /// Mark entities spawned while lowering one statement as owned by the
    /// active plan. Nested mounts set their own component first, so the outer
    /// plan only claims remaining unowned entities.
    pub(super) fn mark_new_plan_actors(&mut self, first_new_entity_index: usize) {
        let Some(owner) = self.build_plan_ownership.last().copied() else {
            return;
        };
        let end = self.world.len();
        for index in first_new_entity_index..end {
            let id = crate::timeline::EntityId(index as u32);
            if self.world.plan_ownership(id).is_some() {
                continue;
            }
            if let Some(mut entity) = self.entity_by_id_mut(id) {
                entity.set_plan_ownership(owner);
            }
        }
    }

    /// Allocate a private label in a namespace that source identifiers cannot
    /// spell, checking the final parent-qualified identity against the World.
    fn fresh_plan_actor_label(
        &self,
        mount_id: u64,
        root: &str,
        parent_label: Option<&str>,
    ) -> String {
        let mut label = format!("@animatix:plan:{mount_id}:{root}");
        let mut collision = 0u64;
        loop {
            let qualified = parent_label
                .map(|parent| format!("{parent}.{label}"))
                .unwrap_or_else(|| label.clone());
            if self.world.get_id(&qualified).is_none() {
                return label;
            }
            collision = collision.saturating_add(1);
            label = format!("@animatix:plan:{mount_id}:{collision}:{root}");
        }
    }

    /// Lower operations while the placement's captures and methods are installed.
    fn mount_plan_operations(
        &mut self,
        placement: PlanPlacement<'_>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<f64> {
        let PlanPlacement {
            plan,
            mount_id,
            qualified_labels,
            origin_ms,
            parent_label,
            inherited_rate,
            span,
        } = placement;
        let mut local_end = plan.extent().map(PlanDuration::milliseconds);
        for op in plan.ops() {
            let end = match op {
                PlanOp::Builtin(call) => {
                    let remapped = remap_builtin_call(call, plan.id(), mount_id, qualified_labels);
                    if remapped.name == "persist"
                        && remapped.args.iter().any(|(_, value)| {
                            matches!(value, Value::ActorRef(actor)
                            if !actor.is_caller()
                                || self.world.get_id(actor.label()).is_some_and(|entity| {
                                    self.world.plan_ownership(entity).is_some()
                                }))
                        })
                    {
                        diagnostics.push(plan_timing_error(
                            "persist cannot target a plan-local actor owned by a reusable plan",
                            span,
                        ));
                        continue;
                    }
                    self.lower_builtin_call(
                        &remapped,
                        origin_ms,
                        inherited_rate,
                        span,
                        diagnostics,
                    );
                    Some(builtin_extent_ms(call))
                },
                PlanOp::Property {
                    actor,
                    property,
                    value,
                    start,
                    duration,
                    easing,
                } => {
                    let actor = remap_value(
                        &Value::ActorRef(actor.clone()),
                        plan.id(),
                        mount_id,
                        qualified_labels,
                    );
                    let Value::ActorRef(actor) = actor else {
                        diagnostics.push(plan_timing_error(
                            "property plan lost its actor reference while mounting",
                            span,
                        ));
                        continue;
                    };
                    self.lower_property_op(
                        &actor,
                        property,
                        value,
                        *start,
                        *duration,
                        *easing,
                        origin_ms,
                        inherited_rate,
                        span,
                        diagnostics,
                    );
                    Some(start.milliseconds() + duration.milliseconds())
                },
                PlanOp::Statements(body) => {
                    let remapped = remap_statements(body, qualified_labels, qualified_labels);
                    self.merge_plan_references(&remapped);
                    baseline::anchor_external_property_writes(
                        self,
                        &remapped,
                        origin_ms,
                        qualified_labels,
                        span,
                        diagnostics,
                    );
                    self.mount_statements(
                        &remapped,
                        origin_ms,
                        inherited_rate,
                        local_end,
                        parent_label,
                        diagnostics,
                    )
                },
                PlanOp::Sequence(children) => {
                    let mut local_cursor = 0.0;
                    let mut complete = true;
                    for child in children.iter() {
                        let extent = child.extent().map(PlanDuration::milliseconds);
                        self.mount_plan(
                            child,
                            origin_ms + local_cursor * inherited_rate,
                            parent_label,
                            inherited_rate,
                            span,
                            diagnostics,
                        );
                        if let Some(extent) = extent {
                            local_cursor += extent;
                        } else {
                            complete = false;
                            diagnostics.push(plan_timing_error(
                                "sequence child has no finite build-time extent",
                                span,
                            ));
                        }
                    }
                    complete.then_some(local_cursor)
                },
                PlanOp::Stagger { interval, children } => {
                    let mut end = 0.0f64;
                    for (index, child) in children.iter().enumerate() {
                        let offset = interval.milliseconds() * index as f64;
                        self.mount_plan(
                            child,
                            origin_ms + offset * inherited_rate,
                            parent_label,
                            inherited_rate,
                            span,
                            diagnostics,
                        );
                        if let Some(extent) = child.extent() {
                            end = end.max(offset + extent.milliseconds());
                        }
                    }
                    Some(end)
                },
                PlanOp::Retimed {
                    plan: child,
                    factor,
                    fit_duration,
                } => {
                    let factor = if let Some(requested) = fit_duration {
                        match child.extent() {
                            Some(extent) if extent.seconds() > 0.0 => {
                                requested.seconds() / extent.seconds()
                            },
                            _ => {
                                diagnostics.push(plan_timing_error(
                                    "fit_duration needs a child plan with a positive finite extent",
                                    span,
                                ));
                                continue;
                            },
                        }
                    } else {
                        *factor
                    };
                    self.mount_plan(
                        child,
                        origin_ms,
                        parent_label,
                        inherited_rate * factor,
                        span,
                        diagnostics,
                    );
                    child.extent().map(|extent| extent.milliseconds() * factor)
                },
            };
            if let Some(end) = end {
                local_end = Some(local_end.unwrap_or(0.0).max(end));
            }
        }

        local_end
    }

    fn mount_statements(
        &mut self,
        body: &[Stmt],
        origin_ms: f64,
        rate: f64,
        local_extent_ms: Option<f64>,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<f64> {
        self.mount_statements_from(
            body,
            origin_ms,
            rate,
            local_extent_ms,
            parent_label,
            0.0,
            diagnostics,
        )
    }

    fn merge_plan_references(&mut self, body: &[Stmt]) {
        let scan = crate::timeline::build::referenced_roots::scan_references(body);
        self.merge_reference_scan(scan);
    }

    pub(crate) fn resolve_marker_time(
        &self,
        time: &Time,
        span: Option<Span>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<f64> {
        let value = match time {
            Time::Expression(expr) => match evaluate_expr(expr, &self.env) {
                Ok(Value::Duration(seconds)) if seconds.is_finite() && seconds >= 0.0 => {
                    seconds * 1000.0
                },
                Ok(value) => {
                    diagnostics.push(plan_timing_error(
                        &format!("computed marker expects a non-negative Duration, got {value:?}"),
                        span,
                    ));
                    return None;
                },
                Err(error) => {
                    diagnostics
                        .push(plan_timing_error(&format!("computed marker failed: {error}"), span));
                    return None;
                },
            },
            Time::SpeechMarker { .. } | Time::SpeechWord { .. } => {
                diagnostics.push(plan_timing_error(
                    "speech markers inside reusable plans must be lowered before mounting",
                    span,
                ));
                return None;
            },
            _ => match crate::timeline::time_to_ms_at_beat(time, self.beat_duration_ms()) {
                Ok(value) => value,
                Err(error) => {
                    diagnostics.push(plan_timing_error(&error.to_string(), span));
                    return None;
                },
            },
        };
        if value.is_finite() && value >= 0.0 {
            Some(value)
        } else {
            diagnostics
                .push(plan_timing_error("plan marker must be finite and non-negative", span));
            None
        }
    }

    fn lower_property_op(
        &mut self,
        actor: &ActorRef,
        property: &str,
        value: &Value,
        start: PlanDuration,
        duration: PlanDuration,
        easing: Option<crate::easing::Easing>,
        origin_ms: f64,
        rate: f64,
        span: Option<Span>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(value) = crate::timeline::value_parser::value_to_expr(value) else {
            diagnostics.push(plan_timing_error(
                &format!(
                    "property plan value for '{property}' cannot be lowered to the property backend"
                ),
                span,
            ));
            return;
        };
        let target = [TargetSegment::Static(actor.label().to_string())];
        self.process_assignment_statement(
            &target,
            property,
            &value,
            &[
                Modifier {
                    name: Some("delay".into()),
                    value: Expr::Duration(start.seconds() * rate),
                },
                Modifier {
                    name: None,
                    value: Expr::Duration(duration.seconds() * rate),
                },
            ],
            easing,
            origin_ms,
            span,
            diagnostics,
        );
    }

    fn lower_builtin_call(
        &mut self,
        call: &BoundBuiltinCall,
        time_ms: f64,
        rate: f64,
        span: Option<Span>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(action) = builtin_invocation(call, rate, diagnostics, span) else {
            return;
        };
        let targets_plan_owned = action.targets.iter().any(|label| {
            self.world
                .get_id(label)
                .is_some_and(|entity| self.world.plan_ownership(entity).is_some())
        });
        if action.verb == "persist"
            && (targets_plan_owned
                || call.args.iter().any(|(_, value)| contains_private_actor(value)))
        {
            diagnostics
                .push(plan_timing_error("plan-local actors cannot escape through `persist`", span));
            return;
        }
        let timing = parse_timing_modifiers(
            &action.modifiers,
            ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let duration_ms = timing.duration_ms as u64;
        let descriptor = animatix_std::callable::callable(&action.verb);
        let category = match descriptor.map(|entry| entry.category.as_str()) {
            Some("Entrance") => ActionCategory::Entrance,
            Some("Exit") => ActionCategory::Exit,
            Some("Effects") => ActionCategory::Effect,
            Some("Reorder") => ActionCategory::Reorder,
            Some("Reveal") => ActionCategory::Reveal,
            _ => ActionCategory::Motion,
        };
        for target in &action.targets {
            self.action_events.push(ActionEvent {
                verb: action.verb.clone(),
                targets: vec![target.clone()],
                start_time_ms: time_ms.max(0.0) as u64,
                duration_ms,
                easing: timing.easing,
                category,
            });
        }
        crate::timeline::actions::process_action(&action, time_ms, self, diagnostics, span);
    }
}

#[cfg(test)]
#[path = "plan_lowerer/ownership_tests.rs"]
mod ownership_tests;
