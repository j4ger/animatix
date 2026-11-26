//! Preserve caller-owned interpolated property values before a reusable plan.
//!
//! A later instant assignment would otherwise become a property's first
//! keyframe and be sampled before the plan is mounted. This build-only pass
//! resolves the plan's concrete caller targets, snapshots every affected field
//! before writing any of them, and adds a same-value keyframe at the mount
//! clock. Plan-local actors are declared by the plan itself and are excluded.

use std::collections::HashSet;

use animatix_syntax::ast::{Expr, Span, Stmt, TargetSegment, Time};

use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::timeline::env::{Environment, Value};
use crate::timeline::property_engine::PropertyValue;
use crate::timeline::property_registry::{ActorField, PropertyFlags, PropertySchema};
use crate::timeline::utils::seconds_to_millis;
use crate::timeline::{Timeline, evaluate_expr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FieldKey {
    entity: crate::timeline::EntityId,
    field: ActorField,
}

struct PendingField {
    key: FieldKey,
    label: String,
    property: String,
    span: Option<Span>,
    schema: &'static PropertySchema,
}

struct Snapshot {
    pending: PendingField,
    value: PropertyValue,
}

/// Add a mount-time hold for interpolated properties assigned by remapped
/// statements. The complete snapshot is collected before the first write so
/// one property's baseline cannot affect another property's captured value.
pub(super) fn anchor_external_property_writes(
    timeline: &mut Timeline,
    statements: &[Stmt],
    origin_ms: f64,
    qualified_labels: &std::collections::HashMap<String, String>,
    plan_span: Option<Span>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !origin_ms.is_finite() {
        return;
    }
    let time_ms = origin_ms.max(0.0) as u64;
    let local_roots = qualified_labels.values().map(String::as_str).collect::<Vec<_>>();
    let mut env = timeline.build_eval_env(time_ms);
    let mut pending = Vec::new();
    let mut seen = HashSet::new();
    let mut fenced_fields = HashSet::new();

    collect_statement_fields(
        timeline,
        statements,
        &mut env,
        time_ms,
        0.0,
        &local_roots,
        plan_span,
        &mut seen,
        &mut fenced_fields,
        &mut pending,
    );

    let snapshots = pending
        .into_iter()
        .filter(|pending| !fenced_fields.contains(&pending.key))
        .filter_map(|pending| snapshot_field(timeline, pending, time_ms, diagnostics))
        .collect::<Vec<_>>();

    let mut wrote_any = false;
    for snapshot in snapshots {
        let PendingField {
            key,
            label,
            property,
            span,
            schema: _,
        } = snapshot.pending;
        if timeline.world.property_has_keyframe_at(key.entity, key.field, time_ms) {
            continue;
        }

        let mut write_diagnostics = Vec::new();
        timeline.world.write_property_field(
            key.entity,
            key.field,
            snapshot.value,
            time_ms,
            time_ms,
            Easing::Linear,
            &mut write_diagnostics,
        );
        if !write_diagnostics.is_empty() {
            diagnostics.extend(write_diagnostics);
        }
        if !timeline.world.property_has_keyframe_at(key.entity, key.field, time_ms) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("could not preserve {label}.{property} at reusable-plan mount time"),
                )
                .with_ast_span(span),
            );
        } else {
            wrote_any = true;
        }
    }
    if wrote_any {
        timeline.invalidate_frame_cache();
    }
}

#[allow(clippy::too_many_arguments)] // Scanner state is explicit so the pass stays build-only.
fn collect_statement_fields(
    timeline: &Timeline,
    statements: &[Stmt],
    env: &mut Environment,
    time_ms: u64,
    local_clock_ms: f64,
    local_roots: &[&str],
    plan_span: Option<Span>,
    seen: &mut HashSet<FieldKey>,
    fenced_fields: &mut HashSet<FieldKey>,
    pending: &mut Vec<PendingField>,
) {
    let mut scanner = FieldScanner {
        timeline,
        time_ms,
        local_roots,
        plan_span,
        seen,
        fenced_fields,
        pending,
    };
    scanner.collect(statements, env, local_clock_ms);
}

struct FieldScanner<'a, 'roots> {
    timeline: &'a Timeline,
    time_ms: u64,
    local_roots: &'roots [&'roots str],
    plan_span: Option<Span>,
    seen: &'a mut HashSet<FieldKey>,
    fenced_fields: &'a mut HashSet<FieldKey>,
    pending: &'a mut Vec<PendingField>,
}

impl FieldScanner<'_, '_> {
    fn collect(&mut self, statements: &[Stmt], env: &mut Environment, local_clock_ms: f64) {
        for statement in statements {
            self.collect_statement(statement, env, local_clock_ms);
        }
    }

    fn collect_statement(&mut self, statement: &Stmt, env: &mut Environment, local_clock_ms: f64) {
        match statement {
            Stmt::Assignment {
                target,
                property,
                modifiers,
                span,
                ..
            } => collect_target_fields(
                self.timeline,
                target,
                property,
                *span,
                self.plan_span,
                assignment_has_start_fence(modifiers, env, local_clock_ms == 0.0),
                env,
                self.time_ms,
                self.local_roots,
                self.seen,
                self.fenced_fields,
                self.pending,
            ),
            Stmt::Keyframe { time, body, .. } => {
                self.collect_marker(time, body, env, local_clock_ms, false)
            },
            Stmt::RelativeKeyframe { offset, body, .. } => {
                self.collect_marker(offset, body, env, local_clock_ms, true)
            },
            Stmt::Sequence { body, .. } => self.collect_sequence(body, env, local_clock_ms),
            Stmt::Stagger {
                modifiers, body, ..
            } => self.collect_stagger(modifiers, body, env, local_clock_ms),
            // Reactive bindings and always blocks compile to modifier IR and
            // do not write World keyframes. Adding a mount baseline for them
            // would alter the host's pre-existing interpolated track.
            Stmt::Always { .. } | Stmt::ReactiveBinding { .. } => {},
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.collect_conditional(
                condition,
                then_branch,
                else_branch.as_deref(),
                env,
                local_clock_ms,
            ),
            Stmt::Match {
                scrutinee, arms, ..
            } => self.collect_match(scrutinee, arms, env, local_clock_ms),
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                modifiers,
                ..
            } => self.collect_loop(
                var,
                index_var.as_deref(),
                iterable,
                body,
                modifiers,
                env,
                local_clock_ms,
            ),
            Stmt::LetDecl { name, value, .. } => {
                if let Ok(value) = evaluate_expr(value, env) {
                    env.set(name, value);
                }
            },
            Stmt::Block { body, .. } => {
                let mut block_env = env.clone();
                self.collect(body, &mut block_env, local_clock_ms);
            },
            Stmt::Expr(expression, _) if local_clock_ms == 0.0 => {
                if let Ok(Value::TimelinePlan(plan)) = evaluate_expr(expression, env) {
                    collect_plan_start_fences(
                        self.timeline,
                        &plan,
                        env,
                        self.time_ms,
                        self.fenced_fields,
                    );
                }
            },
            // Nested expression plans mount themselves when the expression
            // statement is lowered; their own Statements operations run this
            // pass with their own actor map and mount clock. This start-only
            // walk prevents an outer end-only write from masking a nested
            // child's authored local-zero keyframe.
            _ => {},
        }
    }

    fn collect_marker(
        &mut self,
        marker: &Time,
        body: &[Stmt],
        env: &mut Environment,
        local_clock_ms: f64,
        relative: bool,
    ) {
        let Some(marker_ms) = time_to_ms(marker, env, self.timeline) else {
            return;
        };
        let clock_ms = if relative {
            local_clock_ms + marker_ms
        } else {
            marker_ms
        };
        self.collect(body, env, clock_ms);
    }

    fn collect_sequence(&mut self, body: &[Stmt], env: &mut Environment, local_clock_ms: f64) {
        let mut cursor_ms = local_clock_ms;
        for statement in body {
            self.collect(std::slice::from_ref(statement), env, cursor_ms);
            cursor_ms += super::statement_span_ms(statement, env).unwrap_or(0.0);
        }
    }

    fn collect_stagger(
        &mut self,
        modifiers: &[animatix_syntax::ast::Modifier],
        body: &[Stmt],
        env: &mut Environment,
        local_clock_ms: f64,
    ) {
        let mut stagger_diagnostics = Vec::new();
        let Some(parsed) =
            crate::timeline::parse_stagger_modifiers(modifiers, &mut stagger_diagnostics)
        else {
            return;
        };
        let global_time_ms = self.time_ms as f64 + local_clock_ms;
        let delays = self.timeline.compute_stagger_delays(&parsed, body, global_time_ms);
        for (statement, delay_ms) in body.iter().zip(delays) {
            self.collect(std::slice::from_ref(statement), env, local_clock_ms + delay_ms);
        }
    }

    fn collect_conditional(
        &mut self,
        condition: &Expr,
        then_branch: &[Stmt],
        else_branch: Option<&[Stmt]>,
        env: &mut Environment,
        local_clock_ms: f64,
    ) {
        let branch = match evaluate_expr(condition, env) {
            Ok(value) if value.is_truthy() => Some(then_branch),
            Ok(_) => else_branch,
            // The normal build walk diagnoses this condition and skips both branches.
            Err(_) => None,
        };
        if let Some(branch) = branch {
            self.collect(branch, env, local_clock_ms);
        }
    }

    fn collect_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[(animatix_syntax::ast::MatchPattern, Vec<Stmt>)],
        env: &mut Environment,
        local_clock_ms: f64,
    ) {
        let Ok(value) = evaluate_expr(scrutinee, env) else {
            return;
        };
        if let Some((_, body)) = arms
            .iter()
            .find(|(pattern, _)| crate::timeline::build::pattern_matches(pattern, &value))
        {
            self.collect(body, env, local_clock_ms);
        }
    }

    #[allow(clippy::too_many_arguments)] // Loop iteration context is passed unchanged to the scanner.
    fn collect_loop(
        &mut self,
        var: &animatix_syntax::ast::LoopPattern,
        index_var: Option<&str>,
        iterable: &Expr,
        body: &[Stmt],
        modifiers: &[animatix_syntax::ast::Modifier],
        env: &Environment,
        local_clock_ms: f64,
    ) {
        let step_ms = crate::timeline::build::parse_for_loop_step(
            modifiers,
            env,
            self.time_ms as f64,
            &mut Vec::new(),
        );
        let values = crate::timeline::lookup::for_iter_values(iterable, env);
        for (index, value) in values.into_iter().enumerate() {
            let mut iteration_env = env.clone();
            let mut loop_diagnostics = Vec::new();
            crate::timeline::build::bind_loop_var(
                &mut iteration_env,
                var,
                value,
                index,
                &mut loop_diagnostics,
            );
            if let Some(index_var) = index_var {
                iteration_env.set(index_var, Value::Num(index as f64));
                if index_var == "t" {
                    iteration_env.set_authored_t_binding(true);
                }
            }
            self.collect(body, &mut iteration_env, local_clock_ms + step_ms * index as f64);
        }
    }
}

fn collect_plan_start_fences(
    timeline: &Timeline,
    plan: &crate::timeline::plan_value::TimelinePlan,
    env: &Environment,
    time_ms: u64,
    fenced_fields: &mut HashSet<FieldKey>,
) {
    let Some(_depth) = super::PlanExpansionGuard::enter() else {
        return; // Mounting reports the overflow; this pass only finds optional baselines.
    };
    let mut plan_env = env.clone();
    for (name, value) in &plan.captures().0 {
        plan_env.set(name, value.clone());
    }
    plan_env.set_function_context(plan.function_context().cloned());
    if let Some(source_path) = &plan.source_id {
        plan_env.set_source_path(Some(std::sync::Arc::clone(source_path)));
    }
    if !plan.instance_methods().is_empty() {
        crate::timeline::build::extend_input_methods(
            &mut plan_env,
            plan.instance_methods(),
            plan.function_context().map(AsRef::as_ref),
        );
    }

    let local_roots = plan.local_actor_roots().iter().map(String::as_str).collect::<Vec<_>>();
    let mut seen = HashSet::new();
    let mut ignored_pending = Vec::new();
    for operation in plan.ops() {
        match operation {
            crate::timeline::plan_value::PlanOp::Statements(body) => collect_statement_fields(
                timeline,
                body,
                &mut plan_env,
                time_ms,
                0.0,
                &local_roots,
                None,
                &mut seen,
                fenced_fields,
                &mut ignored_pending,
            ),
            crate::timeline::plan_value::PlanOp::Property {
                actor, property, ..
            } => {
                let target = [TargetSegment::Static(actor.label().to_owned())];
                collect_target_fields(
                    timeline,
                    &target,
                    property,
                    None,
                    None,
                    true,
                    &plan_env,
                    time_ms,
                    &local_roots,
                    &mut seen,
                    fenced_fields,
                    &mut ignored_pending,
                );
            },
            crate::timeline::plan_value::PlanOp::Builtin(call) => {
                collect_builtin_start_fences(
                    timeline,
                    call,
                    &plan_env,
                    time_ms,
                    &local_roots,
                    fenced_fields,
                    &mut seen,
                    &mut ignored_pending,
                );
            },
            crate::timeline::plan_value::PlanOp::Sequence(children) => {
                let mut cursor_ms = 0.0;
                for child in children.iter() {
                    if cursor_ms == 0.0 {
                        collect_plan_start_fences(
                            timeline,
                            child,
                            &plan_env,
                            time_ms,
                            fenced_fields,
                        );
                    }
                    let Some(extent) = child.extent() else {
                        break;
                    };
                    cursor_ms += extent.milliseconds();
                }
            },
            crate::timeline::plan_value::PlanOp::Stagger { interval, children } => {
                for (index, child) in children.iter().enumerate() {
                    if index == 0 || interval.seconds() == 0.0 {
                        collect_plan_start_fences(
                            timeline,
                            child,
                            &plan_env,
                            time_ms,
                            fenced_fields,
                        );
                    }
                }
            },
            crate::timeline::plan_value::PlanOp::Retimed { plan, .. } => {
                collect_plan_start_fences(timeline, plan, &plan_env, time_ms, fenced_fields);
            },
        }
    }
}

fn collect_builtin_start_fences(
    timeline: &Timeline,
    call: &crate::timeline::plan_value::BoundBuiltinCall,
    env: &Environment,
    time_ms: u64,
    local_roots: &[&str],
    fenced_fields: &mut HashSet<FieldKey>,
    seen: &mut HashSet<FieldKey>,
    ignored_pending: &mut Vec<PendingField>,
) {
    let Some(kind) = animatix_core::builtins::callable::TimelineBuiltin::from_name(&call.name)
    else {
        return;
    };
    let properties: &[&str] = match kind {
        animatix_core::builtins::callable::TimelineBuiltin::FadeIn
        | animatix_core::builtins::callable::TimelineBuiltin::FadeOut
        | animatix_core::builtins::callable::TimelineBuiltin::Remove => &["opacity"],
        animatix_core::builtins::callable::TimelineBuiltin::WipeIn
        | animatix_core::builtins::callable::TimelineBuiltin::WipeOut => {
            &["stroke_progress", "fill_opacity", "opacity"]
        },
        animatix_core::builtins::callable::TimelineBuiltin::DrawIn
        | animatix_core::builtins::callable::TimelineBuiltin::DrawOut => {
            &["stroke_progress", "fill_opacity", "opacity"]
        },
        animatix_core::builtins::callable::TimelineBuiltin::RevealIn
        | animatix_core::builtins::callable::TimelineBuiltin::RevealOut => &[
            "char_progress",
            "stroke_progress",
            "fill_opacity",
            "opacity",
        ],
        animatix_core::builtins::callable::TimelineBuiltin::Move => {
            if call.args.iter().any(|(name, _)| name == "along") {
                &["at", "rotation"]
            } else {
                &["shift"]
            }
        },
        animatix_core::builtins::callable::TimelineBuiltin::Shift
        | animatix_core::builtins::callable::TimelineBuiltin::Shake
        | animatix_core::builtins::callable::TimelineBuiltin::Bounce => &["shift"],
        animatix_core::builtins::callable::TimelineBuiltin::Rotate => &["rotation"],
        animatix_core::builtins::callable::TimelineBuiltin::Scale
        | animatix_core::builtins::callable::TimelineBuiltin::Pulse
        | animatix_core::builtins::callable::TimelineBuiltin::GrowIn
        | animatix_core::builtins::callable::TimelineBuiltin::ShrinkOut => &["scale"],
        animatix_core::builtins::callable::TimelineBuiltin::SettleIn
        | animatix_core::builtins::callable::TimelineBuiltin::PopIn => &["opacity", "scale"],
        animatix_core::builtins::callable::TimelineBuiltin::Highlight
        | animatix_core::builtins::callable::TimelineBuiltin::Unhighlight => {
            &["highlight_opacity", "highlight_padding", "highlight_radius"]
        },
        animatix_core::builtins::callable::TimelineBuiltin::Persist
        | animatix_core::builtins::callable::TimelineBuiltin::Swap
        | animatix_core::builtins::callable::TimelineBuiltin::Reorder
        // Handoff fences its visibility writes after all authored tracks exist.
        | animatix_core::builtins::callable::TimelineBuiltin::Handoff => &[],
    };
    if properties.is_empty() {
        return;
    }

    let mut actor_labels = Vec::new();
    for (name, value) in &call.args {
        if matches!(name.as_str(), "target" | "targets" | "a" | "b") {
            collect_caller_actor_labels(value, &mut actor_labels);
        }
    }
    for label in actor_labels {
        let target = [TargetSegment::Static(label)];
        for property in properties {
            collect_target_fields(
                timeline,
                &target,
                property,
                None,
                None,
                true,
                env,
                time_ms,
                local_roots,
                seen,
                fenced_fields,
                ignored_pending,
            );
        }
    }
}

fn collect_caller_actor_labels(value: &Value, labels: &mut Vec<String>) {
    match value {
        Value::ActorRef(actor) if actor.is_caller() => labels.push(actor.label().to_owned()),
        Value::List(items) => {
            for item in items.iter() {
                collect_caller_actor_labels(item, labels);
            }
        },
        _ => {},
    }
}

#[allow(clippy::too_many_arguments)] // Scanner context is explicit to keep the walk build-only.
fn collect_target_fields(
    timeline: &Timeline,
    target: &[TargetSegment],
    property: &str,
    span: Option<Span>,
    plan_span: Option<Span>,
    at_origin: bool,
    env: &Environment,
    time_ms: u64,
    local_roots: &[&str],
    seen: &mut HashSet<FieldKey>,
    fenced_fields: &mut HashSet<FieldKey>,
    pending: &mut Vec<PendingField>,
) {
    let Some(schema) = crate::timeline::property_registry::lookup_property(property) else {
        // Extension properties and unknown properties remain the responsibility
        // of the regular assignment writer, which retains their diagnostics.
        return;
    };
    if !schema.flags.contains(PropertyFlags::ANIMATED)
        || !schema.flags.contains(PropertyFlags::ASSIGNABLE)
    {
        return;
    }

    let Some(label) = resolve_target_label(timeline, target, env) else {
        // The normal assignment walk reports unresolved or invalid targets.
        return;
    };
    if is_plan_local_label(&label, local_roots) {
        return;
    }
    let Some(entity) = timeline.world.get_id(&label) else {
        return;
    };
    if timeline.world.first_seen(entity) > time_ms {
        return;
    }

    let field =
        schema.field_for_actor(&timeline.world.caps(entity), timeline.world.actor_type(entity));
    let field = match field {
        ActorField::PositionBindingGroup
        | ActorField::VectorShapeGroup
        | ActorField::PlotDomainGroup
        | ActorField::ContainerLayoutGroup
        | ActorField::NoStorage => match schema.read_source.storage_field() {
            Some(field) => field,
            None => return,
        },
        field => field,
    };

    add_pending_field(
        entity,
        field,
        label.clone(),
        property,
        span.or(plan_span),
        schema,
        seen,
        pending,
    );
    if at_origin {
        fenced_fields.insert(FieldKey { entity, field });
    }

    // `size` writes both the authored geometry size and the layout size.
    // Anchoring each storage field independently preserves an existing size
    // override or layout result without converting through source dimensions.
    if field == ActorField::Size && matches!(property, "size" | "radius_x" | "radius_y") {
        add_pending_field(
            entity,
            ActorField::LayoutSize,
            label.clone(),
            property,
            span.or(plan_span),
            schema,
            seen,
            pending,
        );
        if at_origin {
            fenced_fields.insert(FieldKey {
                entity,
                field: ActorField::LayoutSize,
            });
        }
    }

    // Stroke-only primitives mirror `color` into their stroke track.
    if property == "color"
        && timeline.world.caps(entity).shape.is_some_and(|shape| shape.is_stroke_only())
    {
        add_pending_field(
            entity,
            ActorField::StrokeColor,
            label,
            property,
            span.or(plan_span),
            schema,
            seen,
            pending,
        );
        if at_origin {
            fenced_fields.insert(FieldKey {
                entity,
                field: ActorField::StrokeColor,
            });
        }
    }
}

fn assignment_has_start_fence(
    modifiers: &[animatix_syntax::ast::Modifier],
    env: &Environment,
    at_origin: bool,
) -> bool {
    let mut delay_ms = 0.0;
    let mut duration_ms = 0.0;
    for modifier in modifiers {
        let target = match modifier.name.as_deref() {
            Some("delay") => &mut delay_ms,
            Some("duration") | None => &mut duration_ms,
            _ => continue,
        };
        let milliseconds =
            crate::timeline::timing::duration_expr_to_ms(&modifier.value).or_else(|| {
                match evaluate_expr(&modifier.value, env).ok()? {
                    Value::Duration(seconds) => Some(seconds * 1000.0),
                    _ => None,
                }
            });
        let Some(milliseconds) = milliseconds.filter(|value| value.is_finite() && *value >= 0.0)
        else {
            continue;
        };
        *target = milliseconds;
    }
    (at_origin && delay_ms == 0.0) || delay_ms > 0.0 || duration_ms > 0.0
}

fn time_to_ms(time: &Time, env: &Environment, timeline: &Timeline) -> Option<f64> {
    let milliseconds = match time {
        Time::Seconds(seconds) => seconds_to_millis(*seconds),
        Time::Milliseconds(milliseconds) => *milliseconds as f64,
        Time::Beats(beats) => beats * timeline.beat_duration_ms(),
        Time::Expression(expression) => match evaluate_expr(expression, env).ok()? {
            Value::Duration(seconds) => seconds_to_millis(seconds),
            _ => return None,
        },
        Time::SpeechMarker { .. } | Time::SpeechWord { .. } => return None,
    };
    (milliseconds.is_finite() && milliseconds >= 0.0).then_some(milliseconds)
}

fn add_pending_field(
    entity: crate::timeline::EntityId,
    field: ActorField,
    label: String,
    property: &str,
    span: Option<Span>,
    schema: &'static PropertySchema,
    seen: &mut HashSet<FieldKey>,
    pending: &mut Vec<PendingField>,
) {
    let key = FieldKey { entity, field };
    if seen.insert(key) {
        pending.push(PendingField {
            key,
            label,
            property: property.to_string(),
            span,
            schema,
        });
    }
}

fn snapshot_field(
    timeline: &Timeline,
    pending: PendingField,
    time_ms: u64,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Snapshot> {
    if timeline
        .world
        .property_has_keyframe_at(pending.key.entity, pending.key.field, time_ms)
    {
        // An explicit keyframe already fences the mount clock. This commonly
        // occurs for a native property/action writer's authored delay start.
        return None;
    }

    let value = timeline
        .world
        .read_property_value(pending.key.entity, pending.key.field, time_ms)
        .or_else(|| {
            // These two fields have actor-specific defaults. Other field
            // defaults are needed for aliases such as a numeric width backed
            // by a two-component size track.
            match pending.key.field {
                ActorField::StrokeWidth | ActorField::FontSize => None,
                field => field.default_value(),
            }
        })
        .or_else(|| Some((pending.schema.default_value)(&timeline.world.caps(pending.key.entity))));
    let Some(value) = value else {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::InvalidPropertyValue,
                DiagnosticPhase::Build,
                format!(
                    "no current value is available to preserve {}.{} before reusable-plan writes",
                    pending.label, pending.property
                ),
            )
            .with_ast_span(pending.span),
        );
        return None;
    };
    if !is_interpolated_value(&value) {
        // Discrete strings, booleans, enums and resource payloads take effect
        // at their own authored keyframe and do not need a numeric hold.
        return None;
    }
    Some(Snapshot { pending, value })
}

fn is_interpolated_value(value: &PropertyValue) -> bool {
    matches!(
        value,
        PropertyValue::F32(_)
            | PropertyValue::Vec2(_)
            | PropertyValue::Vec4(_)
            | PropertyValue::Color(_)
            | PropertyValue::Transform(_)
            | PropertyValue::PointList(_)
            | PropertyValue::F32List(_)
            | PropertyValue::Gradient(_)
    )
}

fn resolve_target_label(
    timeline: &Timeline,
    target: &[TargetSegment],
    env: &Environment,
) -> Option<String> {
    let mut resolved = Vec::with_capacity(target.len());
    for segment in target {
        match segment {
            TargetSegment::Static(label) => resolved.push(label.clone()),
            TargetSegment::Indexed { base, index } => {
                let Ok(Value::Num(index)) = evaluate_expr(index, env) else {
                    // The ordinary build path diagnoses a non-constant or
                    // invalid index; a dynamic modifier target is not a
                    // compile-time keyframe and needs no mount hold here.
                    return None;
                };
                if !index.is_finite() || index < 0.0 || index != index.floor() {
                    return None;
                }
                let index = index as usize;
                let indexed = match env.get_path(base) {
                    Some(Value::ActorRef(actor)) => Some(actor.indexed(index)),
                    Some(Value::List(items)) => items.get(index).and_then(|item| match item {
                        Value::ActorRef(actor) => Some(actor.clone()),
                        _ => None,
                    }),
                    _ => None,
                };
                if let Some(actor) = indexed {
                    let authored_prefix = resolved.join(".");
                    if !authored_prefix.is_empty()
                        && actor
                            .label()
                            .strip_prefix(&authored_prefix)
                            .is_some_and(|suffix| suffix.starts_with('.'))
                    {
                        resolved.clear();
                    }
                    resolved.extend(actor.label().split('.').map(str::to_string));
                } else {
                    resolved.push(animatix_syntax::ast::array_actor_label(base, index));
                }
            },
        }
    }

    if resolved.is_empty() {
        return None;
    }
    let authored = resolved.join(".");
    if let Some(Value::ActorRef(actor)) = env.get_path(&authored) {
        return Some(actor.label().to_string());
    }
    let segments = resolved.iter().map(String::as_str).collect::<Vec<_>>();
    timeline.world.resolve_path(&segments).or_else(|| {
        timeline
            .world
            .get_id(&authored)
            .and_then(|id| timeline.world.label(id).map(str::to_string))
    })
}

fn is_plan_local_label(label: &str, local_roots: &[&str]) -> bool {
    local_roots.iter().any(|root| {
        if label == *root {
            return true;
        }
        let Some(suffix) = label.strip_prefix(root) else {
            return false;
        };
        suffix.starts_with('.') || is_array_suffix(suffix)
    })
}

fn is_array_suffix(suffix: &str) -> bool {
    let Some(suffix) = suffix.strip_prefix("__") else {
        return false;
    };
    let digits = suffix.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && (digits == suffix.len() || suffix[digits..].starts_with('.'))
}

#[cfg(test)]
mod tests;
