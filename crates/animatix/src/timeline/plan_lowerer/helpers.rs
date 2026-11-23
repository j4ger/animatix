use super::*;
use animatix_syntax::visit::{AstVisitor, walk_inline_item, walk_stmt};

pub(super) fn restore_plan_scope(timeline: &mut Timeline) {
    if let Some(locals) = timeline.block_scope.pop() {
        for (name, previous) in locals {
            if let Some(value) = previous {
                timeline.env.set(&name, value);
            } else {
                timeline.env.overrides.remove(&name);
                timeline.env.mark_mutated();
            }
        }
    }
}

pub(super) fn remap_capture_values(
    captures: &crate::timeline::env::CapturedEnv,
    plan_id: u64,
    mount_id: u64,
    labels: &HashMap<String, String>,
) -> HashMap<String, Value> {
    captures
        .0
        .iter()
        .map(|(name, value)| (name.clone(), remap_value(value, plan_id, mount_id, labels)))
        .collect()
}

pub(super) fn remap_value(
    value: &Value,
    plan_id: u64,
    mount_id: u64,
    labels: &HashMap<String, String>,
) -> Value {
    match value {
        Value::ActorRef(reference) if reference.local_plan_id() == Some(plan_id) => {
            let label = remap_local_label(reference.label(), labels);
            Value::ActorRef(ActorRef::mounted(mount_id, label))
        },
        Value::List(items) => Value::List(
            items
                .iter()
                .map(|item| remap_value(item, plan_id, mount_id, labels))
                .collect::<Vec<_>>()
                .into(),
        ),
        Value::Object(name, fields) => Value::Object(
            name.clone(),
            fields
                .iter()
                .map(|(name, value)| (name.clone(), remap_value(value, plan_id, mount_id, labels)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn remap_local_label(label: &str, labels: &HashMap<String, String>) -> String {
    labels
        .iter()
        .find_map(|(source, target)| {
            if label == source {
                Some(target.clone())
            } else {
                label
                    .strip_prefix(source)
                    .filter(|suffix| suffix.starts_with('.'))
                    .map(|suffix| format!("{target}{suffix}"))
            }
        })
        .unwrap_or_else(|| label.to_string())
}

pub(super) fn remap_instance_methods(
    methods: &crate::module::InstanceFnRegistry,
    declaration_labels: &HashMap<String, String>,
    reference_labels: &HashMap<String, String>,
) -> crate::module::InstanceFnRegistry {
    let mut remapped = crate::module::InstanceFnRegistry::new();
    for (label, templates) in methods {
        let label = remap_local_label(label, reference_labels);
        let entries = remapped.entry(label).or_default();
        for (name, template) in templates {
            let mut template = template.clone();
            let mut body_references = reference_labels.clone();
            for param in &template.params {
                body_references.remove(&param.name);
            }
            template.body = remap_statements(&template.body, declaration_labels, &body_references);
            if let Some(index) = template.instance_index.take() {
                template.instance_index =
                    Some(remap_expression(index, declaration_labels, reference_labels));
            }
            entries.insert(name.clone(), template);
        }
    }
    remapped
}

fn remap_expression(
    expression: Expr,
    declaration_labels: &HashMap<String, String>,
    reference_labels: &HashMap<String, String>,
) -> Expr {
    let expression = crate::timeline::actor_aliases::resolve_actor_aliases_expr(
        expression,
        reference_labels.clone(),
    );
    ActorMountFolder { declaration_labels }.fold_expr(expression)
}

pub(super) fn remap_statements(
    body: &[Stmt],
    declaration_labels: &HashMap<String, String>,
    reference_labels: &HashMap<String, String>,
) -> Vec<Stmt> {
    let body = crate::timeline::actor_aliases::resolve_actor_aliases(
        body.to_vec(),
        reference_labels.clone(),
    );
    ActorMountFolder { declaration_labels }.fold_stmts(body)
}

fn collect_op_actor_roots(ops: &[PlanOp], plan_id: u64, output: &mut Vec<String>) {
    fn collect_value(value: &Value, plan_id: u64, output: &mut Vec<String>) {
        match value {
            Value::ActorRef(actor) if actor.local_plan_id() == Some(plan_id) => {
                let root = actor.label().split('.').next().unwrap_or(actor.label());
                output.push(root.to_string());
            },
            Value::List(items) => {
                for value in items.iter() {
                    collect_value(value, plan_id, output);
                }
            },
            Value::Object(_, fields) => {
                for value in fields.values() {
                    collect_value(value, plan_id, output);
                }
            },
            _ => {},
        }
    }
    for op in ops {
        match op {
            PlanOp::Builtin(call) => {
                for (_, value) in &call.args {
                    collect_value(value, plan_id, output);
                }
            },
            PlanOp::Property { actor, .. } if actor.local_plan_id() == Some(plan_id) => {
                output.push(actor.label().split('.').next().unwrap_or(actor.label()).to_string());
            },
            _ => {},
        }
    }
}

pub(super) fn remap_builtin_call(
    call: &BoundBuiltinCall,
    plan_id: u64,
    mount_id: u64,
    labels: &HashMap<String, String>,
) -> BoundBuiltinCall {
    BoundBuiltinCall {
        name: call.name.clone(),
        args: call
            .args
            .iter()
            .map(|(name, value)| (name.clone(), remap_value(value, plan_id, mount_id, labels)))
            .collect(),
        byte_span: call.byte_span,
    }
}

struct ActorMountFolder<'a> {
    declaration_labels: &'a HashMap<String, String>,
}

impl AstFolder for ActorMountFolder<'_> {
    fn fold_stmt(&mut self, statement: Stmt) -> Stmt {
        let mut statement = walk_fold_stmt(self, statement);
        if let Stmt::ActorDecl { label, .. } = &mut statement {
            *label = remap_local_label(label, self.declaration_labels);
        }
        statement
    }

    fn fold_inline_item(&mut self, item: InlineItem) -> InlineItem {
        let mut item = animatix_syntax::visit::walk_fold_inline_item(self, item);
        if let InlineItem::Labeled { label, .. } = &mut item {
            *label = remap_local_label(label, self.declaration_labels);
        }
        item
    }
}

pub(super) fn collect_plan_actor_roots(plan: &TimelinePlan) -> Vec<String> {
    let mut roots = plan.local_actor_roots().to_vec();
    collect_op_actor_roots(plan.ops(), plan.id(), &mut roots);
    roots.sort();
    roots.dedup();
    roots
}

pub(super) fn builtin_invocation(
    call: &BoundBuiltinCall,
    rate: f64,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<Span>,
) -> Option<crate::timeline::actions::BuiltinInvocation> {
    let mut targets = Vec::new();
    let mut modifiers = Vec::new();
    for (name, value) in &call.args {
        match name.as_str() {
            "target" | "a" | "b" => {
                if let Value::ActorRef(actor) = value {
                    targets.push(actor.label().to_string());
                } else if !matches!(value, Value::Null) {
                    diagnostics.push(plan_timing_error(
                        &format!("'{name}' expects an actor reference, got {value:?}"),
                        span,
                    ));
                    return None;
                }
            },
            "order" if call.name == "reorder" => {
                let Value::List(items) = value else {
                    diagnostics.push(plan_timing_error("'order' expects a list of actors", span));
                    return None;
                };
                let mut labels = Vec::with_capacity(items.len());
                for item in items.iter() {
                    let Value::ActorRef(actor) = item else {
                        diagnostics
                            .push(plan_timing_error("'order' contains a non-actor value", span));
                        return None;
                    };
                    labels.push(Expr::Str(actor.label().to_string()));
                }
                modifiers.push(Modifier {
                    name: Some(name.clone()),
                    value: Expr::List(labels),
                });
            },
            "targets" => match value {
                Value::List(items) => {
                    for item in items.iter() {
                        if let Value::ActorRef(actor) = item {
                            targets.push(actor.label().to_string());
                        } else {
                            diagnostics.push(plan_timing_error(
                                &format!("'targets' contains a non-actor value {item:?}"),
                                span,
                            ));
                            return None;
                        }
                    }
                },
                Value::Null => {},
                other => {
                    diagnostics.push(plan_timing_error(
                        &format!("'targets' expects a list of actors, got {other:?}"),
                        span,
                    ));
                    return None;
                },
            },
            _ if matches!(value, Value::Null) => {},
            _ => {
                let value = match value {
                    Value::Duration(seconds)
                        if matches!(
                            name.as_str(),
                            "duration" | "delay" | "anticipate" | "stagger"
                        ) =>
                    {
                        Value::Duration(seconds * rate)
                    },
                    other => other.clone(),
                };
                let Some(value) = crate::timeline::value_parser::value_to_expr(&value) else {
                    diagnostics.push(plan_timing_error(
                        &format!("argument '{name}' cannot be represented by the action backend"),
                        span,
                    ));
                    return None;
                };
                modifiers.push(Modifier {
                    name: Some(name.clone()),
                    value,
                });
            },
        }
    }
    if targets.is_empty() {
        diagnostics.push(plan_timing_error(
            &format!("'{}' needs at least one actor target", call.name),
            span,
        ));
        return None;
    }
    Some(crate::timeline::actions::BuiltinInvocation {
        verb: call.name.clone(),
        target_index: vec![None; targets.len()],
        target_index_position: vec![None; targets.len()],
        targets,
        args: Vec::new(),
        modifiers,
        byte_span: call.byte_span,
    })
}

pub(super) fn contains_private_actor(value: &Value) -> bool {
    match value {
        Value::ActorRef(actor) => !actor.is_caller(),
        Value::List(items) => items.iter().any(contains_private_actor),
        Value::Object(_, fields) => fields.values().any(contains_private_actor),
        _ => false,
    }
}

pub(super) fn builtin_extent_ms(call: &BoundBuiltinCall) -> f64 {
    call.args
        .iter()
        .filter(|(name, _)| matches!(name.as_str(), "duration" | "delay"))
        .filter_map(|(_, value)| match value {
            Value::Duration(seconds) => Some(seconds * 1000.0),
            _ => None,
        })
        .sum::<f64>()
}

pub(super) fn statement_list_extent_ms(body: &[Stmt], env: &Environment) -> Option<f64> {
    let scoped_env = env.clone();
    scoped_env.push_let_scope([]);
    let extent = statement_list_extent_in_scope(body, &scoped_env);
    scoped_env.pop_let_scope();
    extent
}

pub(crate) fn estimate_statements_extent_ms(body: &[Stmt], env: &Environment) -> Option<f64> {
    estimate_statements_extent_from(body, env, 0.0)
}

fn estimate_statements_extent_from(
    body: &[Stmt],
    env: &Environment,
    initial_cursor_ms: f64,
) -> Option<f64> {
    let scoped_env = env.clone();
    scoped_env.push_let_scope([]);
    let extent = estimate_statements_extent_in_scope(body, &scoped_env, initial_cursor_ms);
    scoped_env.pop_let_scope();
    extent
}

fn estimate_statements_extent_in_scope(
    body: &[Stmt],
    env: &Environment,
    initial_cursor_ms: f64,
) -> Option<f64> {
    let mut cursor_ms = initial_cursor_ms;
    let mut extent_ms = initial_cursor_ms;
    for statement in body {
        match statement {
            Stmt::LetDecl { name, value, .. } => {
                if let Ok(value) = evaluate_expr(value, env) {
                    env.set_let_binding(name, value);
                }
            },
            Stmt::Keyframe { time, body, .. } => {
                cursor_ms = measure_time_ms(time, env)?;
                extent_ms = extent_ms.max(cursor_ms);
                extent_ms = extent_ms.max(estimate_statements_extent_from(body, env, cursor_ms)?);
            },
            Stmt::RelativeKeyframe { offset, body, .. } => {
                cursor_ms += measure_time_ms(offset, env)?;
                extent_ms = extent_ms.max(cursor_ms);
                extent_ms = extent_ms.max(estimate_statements_extent_from(body, env, cursor_ms)?);
            },
            Stmt::Block { body, .. } => {
                extent_ms = extent_ms.max(estimate_statements_extent_from(body, env, cursor_ms)?);
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let branch = match evaluate_expr(condition, env) {
                    Ok(value) if value.is_truthy() => Some(then_branch.as_slice()),
                    Ok(_) => else_branch.as_deref(),
                    Err(_) => None,
                };
                if let Some(branch) = branch {
                    extent_ms =
                        extent_ms.max(estimate_statements_extent_from(branch, env, cursor_ms)?);
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                let value = evaluate_expr(scrutinee, env).ok()?;
                if let Some((_, branch)) = arms
                    .iter()
                    .find(|(pattern, _)| crate::timeline::build::pattern_matches(pattern, &value))
                {
                    extent_ms =
                        extent_ms.max(estimate_statements_extent_from(branch, env, cursor_ms)?);
                }
            },
            _ => {
                extent_ms = extent_ms.max(cursor_ms + statement_span_ms(statement, env)?);
            },
        }
    }
    Some(extent_ms.max(cursor_ms))
}

fn statement_list_extent_in_scope(body: &[Stmt], env: &Environment) -> Option<f64> {
    let mut extent_ms = 0.0f64;
    for statement in body {
        if let Stmt::LetDecl { name, value, .. } = statement {
            if let Ok(value) = evaluate_expr(value, env) {
                env.set_let_binding(name, value);
            }
            continue;
        }
        extent_ms = extent_ms.max(statement_span_ms(statement, env)?);
    }
    Some(extent_ms)
}

pub(crate) fn statement_span_ms(statement: &Stmt, env: &Environment) -> Option<f64> {
    match statement {
        Stmt::Assignment { modifiers, .. } => modifier_span_ms(modifiers, env),
        Stmt::ActorDecl {
            ty,
            modifiers,
            children,
            ..
        } => actor_declaration_extent_ms(ty, modifiers, children, env),
        Stmt::Expr(expr, _) => match evaluate_expr(expr, env).ok()? {
            Value::TimelinePlan(plan) => plan.extent().map(PlanDuration::milliseconds),
            Value::Unit => Some(0.0),
            _ => Some(0.0),
        },
        Stmt::Sequence { body, .. } => sequence_statement_extent_ms(body, env),
        Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body,
            modifiers,
            ..
        } => for_loop_extent_ms(var, index_var.as_deref(), iterable, body, modifiers, env),
        Stmt::Stagger {
            modifiers, body, ..
        } => {
            let interval = modifier_span_ms(modifiers, env)?;
            let child = statement_list_extent_ms(body, env)?;
            Some(child + interval * body.len().saturating_sub(1) as f64)
        },
        Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
            statement_list_extent_ms(body, env)
        },
        Stmt::LetDecl { .. }
        | Stmt::Always { .. }
        | Stmt::ReactiveBinding { .. }
        | Stmt::Comment(..)
        | Stmt::TypeAlias { .. } => Some(0.0),
        _ => None,
    }
}

fn actor_declaration_extent_ms(
    ty: &str,
    modifiers: &[Modifier],
    children: &[InlineItem],
    env: &Environment,
) -> Option<f64> {
    if ty == "Audio" {
        return None;
    }
    let declaration_ms = modifier_span_ms(modifiers, env)?;
    let children_ms = inline_items_extent_ms(children, env)?;
    Some(declaration_ms.max(children_ms))
}

fn inline_items_extent_ms(items: &[InlineItem], env: &Environment) -> Option<f64> {
    items
        .iter()
        .map(|item| inline_item_extent_ms(item, env))
        .collect::<Option<Vec<_>>>()
        .map(|spans| spans.into_iter().fold(0.0, f64::max))
}

fn inline_item_extent_ms(item: &InlineItem, env: &Environment) -> Option<f64> {
    match item {
        InlineItem::Anonymous {
            ty,
            modifiers,
            children,
            ..
        }
        | InlineItem::Labeled {
            ty,
            modifiers,
            children,
            ..
        } => actor_declaration_extent_ms(ty, modifiers, children, env),
        InlineItem::ForLoop {
            var,
            index_var,
            iterable,
            body,
        } => inline_for_loop_extent_ms(var, index_var.as_deref(), iterable, body, env),
        InlineItem::Expr(expression, _) => match evaluate_expr(expression, env).ok()? {
            Value::TimelinePlan(plan) => plan.extent().map(PlanDuration::milliseconds),
            Value::Unit => Some(0.0),
            _ => Some(0.0),
        },
        InlineItem::SlotFill { items, .. } => inline_items_extent_ms(items, env),
        InlineItem::SlotMarker => Some(0.0),
    }
}

fn inline_for_loop_extent_ms(
    pattern: &crate::ast::LoopPattern,
    index_var: Option<&str>,
    iterable: &Expr,
    body: &[InlineItem],
    env: &Environment,
) -> Option<f64> {
    let mut extent = 0.0f64;
    for (index, value) in
        crate::timeline::lookup::for_iter_values(iterable, env).into_iter().enumerate()
    {
        let iteration_env = scoped_loop_environment(env, pattern, value, index, index_var);
        extent = extent.max(inline_items_extent_ms(body, &iteration_env)?);
    }
    Some(extent)
}

fn sequence_statement_extent_ms(body: &[Stmt], env: &Environment) -> Option<f64> {
    let scoped_env = env.clone();
    scoped_env.push_let_scope([]);
    let mut cursor_ms = 0.0;
    let mut extent_ms = 0.0f64;
    for statement in body {
        if let Stmt::LetDecl { name, value, .. } = statement {
            if let Ok(value) = evaluate_expr(value, &scoped_env) {
                scoped_env.set_let_binding(name, value);
            }
            continue;
        }
        let span_ms = statement_span_ms(statement, &scoped_env)?;
        cursor_ms += span_ms;
        extent_ms = extent_ms.max(cursor_ms);
    }
    scoped_env.pop_let_scope();
    Some(extent_ms)
}

fn scoped_loop_environment(
    env: &Environment,
    pattern: &crate::ast::LoopPattern,
    value: Value,
    index: usize,
    index_var: Option<&str>,
) -> Environment {
    let mut iteration_env = env.clone();
    iteration_env.push_let_scope([]);
    crate::timeline::build::bind_loop_var(
        &mut iteration_env,
        pattern,
        value,
        index,
        &mut Vec::new(),
    );
    let names = match pattern {
        crate::ast::LoopPattern::Single(name) => std::slice::from_ref(name),
        crate::ast::LoopPattern::Tuple(names) => names.as_slice(),
    };
    for name in names {
        if let Some(value) = iteration_env.overrides.get(name).cloned() {
            iteration_env.set_let_binding(name, value);
        }
    }
    if let Some(index_var) = index_var {
        iteration_env.set_let_binding(index_var, Value::Num(index as f64));
        if index_var == "t" {
            iteration_env.set_authored_t_binding(true);
        }
    }
    iteration_env
}

fn for_loop_extent_ms(
    pattern: &crate::ast::LoopPattern,
    index_var: Option<&str>,
    iterable: &Expr,
    body: &[Stmt],
    modifiers: &[Modifier],
    env: &Environment,
) -> Option<f64> {
    let step = match modifiers.iter().find(|modifier| modifier.name.as_deref() == Some("step")) {
        Some(modifier) => duration_ms(&modifier.value, env)?,
        None => 0.0,
    };
    let values = crate::timeline::lookup::for_iter_values(iterable, env);
    let mut extent = 0.0f64;
    for (index, value) in values.into_iter().enumerate() {
        let iteration_env = scoped_loop_environment(env, pattern, value, index, index_var);
        let body_extent = estimate_statements_extent_ms(body, &iteration_env)?;
        extent = extent.max(index as f64 * step + body_extent);
    }
    Some(extent)
}

fn measure_time_ms(time: &Time, env: &Environment) -> Option<f64> {
    let time_ms = match time {
        Time::Seconds(seconds) => seconds * 1000.0,
        Time::Milliseconds(milliseconds) => *milliseconds as f64,
        Time::Beats(beats) => beats * crate::ast::beat_seconds(crate::ast::DEFAULT_BPM) * 1000.0,
        Time::Expression(expr) => match evaluate_expr(expr, env).ok()? {
            Value::Duration(seconds) => seconds * 1000.0,
            _ => return None,
        },
        Time::SpeechMarker { .. } | Time::SpeechWord { .. } => return None,
    };
    (time_ms.is_finite() && time_ms >= 0.0).then_some(time_ms)
}

/// Scale explicit assignment timing values while a plan's affine clock is active.
pub(crate) fn scale_modifiers(modifiers: &[Modifier], rate: f64) -> Vec<Modifier> {
    modifiers
        .iter()
        .cloned()
        .map(|mut modifier| {
            let is_time = modifier
                .name
                .as_deref()
                .is_none_or(|name| matches!(name, "duration" | "delay" | "anticipate"));
            if is_time && (rate - 1.0).abs() > f64::EPSILON {
                if let Expr::Duration(seconds) = &modifier.value {
                    modifier.value = Expr::Duration(*seconds * rate);
                } else {
                    modifier.value = Expr::Binary(
                        Box::new(modifier.value),
                        BinaryOp::Mul,
                        Box::new(Expr::Num(rate)),
                    );
                }
            }
            modifier
        })
        .collect()
}

fn modifier_span_ms(modifiers: &[Modifier], env: &Environment) -> Option<f64> {
    let mut duration = 0.0;
    let mut delay = 0.0;
    for modifier in modifiers {
        let target = match modifier.name.as_deref() {
            Some("duration") => &mut duration,
            Some("delay") => &mut delay,
            None => &mut duration,
            _ => continue,
        };
        *target = duration_ms(&modifier.value, env)?;
    }
    Some(duration + delay)
}

fn duration_ms(expr: &Expr, env: &Environment) -> Option<f64> {
    if let Some(milliseconds) = crate::timeline::timing::duration_expr_to_ms(expr) {
        return Some(milliseconds);
    }
    match evaluate_expr(expr, env).ok()? {
        Value::Duration(seconds) if seconds.is_finite() && seconds >= 0.0 => Some(seconds * 1000.0),
        _ => None,
    }
}

pub(super) fn localize_always_body(
    body: &[Stmt],
    global_start_ms: f64,
    global_end_ms: f64,
    origin_ms: f64,
    rate: f64,
    authored_t_capture: bool,
) -> Vec<Stmt> {
    let mut folder = LocalClockFolder {
        origin_seconds: origin_ms / 1000.0,
        rate,
        bound_t: authored_t_capture,
    };
    let body = folder.fold_stmts(body.to_vec());
    let t = Expr::Ident(crate::timeline::env::FRAME_TIME_IDENT.into());
    let condition = Expr::Binary(
        Box::new(Expr::Binary(
            Box::new(t.clone()),
            BinaryOp::Gte,
            Box::new(Expr::Num(global_start_ms / 1000.0)),
        )),
        BinaryOp::And,
        Box::new(Expr::Binary(
            Box::new(t),
            BinaryOp::Lt,
            Box::new(Expr::Num(global_end_ms / 1000.0)),
        )),
    );
    vec![Stmt::Conditional {
        condition,
        then_branch: body,
        else_branch: None,
        span: None,
    }]
}

struct LocalClockFolder {
    origin_seconds: f64,
    rate: f64,
    bound_t: bool,
}

impl AstFolder for LocalClockFolder {
    fn fold_stmts(&mut self, statements: Vec<Stmt>) -> Vec<Stmt> {
        let previous = self.bound_t;
        let mut folded = Vec::with_capacity(statements.len());
        for statement in statements {
            match statement {
                Stmt::LetDecl {
                    is_pub,
                    name,
                    value,
                    span,
                } => {
                    let value = self.fold_expr(value);
                    if name == "t" {
                        self.bound_t = true;
                    }
                    folded.push(Stmt::LetDecl {
                        is_pub,
                        name,
                        value,
                        span,
                    });
                },
                other => folded.push(self.fold_stmt(other)),
            }
        }
        self.bound_t = previous;
        folded
    }

    fn fold_stmt(&mut self, statement: Stmt) -> Stmt {
        match statement {
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                modifiers,
                span,
            } => {
                let iterable = self.fold_expr(iterable);
                let modifiers =
                    modifiers.into_iter().map(|item| self.fold_modifier(item)).collect();
                let previous = self.bound_t;
                self.bound_t |= loop_binds_t(&var, index_var.as_deref());
                let body = self.fold_stmts(body);
                self.bound_t = previous;
                Stmt::ForLoop {
                    var,
                    index_var,
                    iterable,
                    body,
                    modifiers,
                    span,
                }
            },
            Stmt::FnDecl {
                is_pub,
                name,
                params,
                return_type,
                body,
                span,
            } => {
                let previous = self.bound_t;
                self.bound_t |= params.iter().any(|param| param.name == "t");
                let body = self.fold_stmts(body);
                self.bound_t = previous;
                Stmt::FnDecl {
                    is_pub,
                    name,
                    params,
                    return_type,
                    body,
                    span,
                }
            },
            Stmt::ReactiveBinding {
                target,
                property,
                value,
                value_span,
                span,
            } => Stmt::Assignment {
                target: target
                    .into_iter()
                    .map(|segment| self.fold_target_segment(segment))
                    .collect(),
                property,
                value: self.fold_expr(value),
                modifiers: Vec::new(),
                easing: None,
                value_span,
                span,
            },
            other => walk_fold_stmt(self, other),
        }
    }

    fn fold_expr(&mut self, expression: Expr) -> Expr {
        match expression {
            Expr::Ident(name) if name == "t" && !self.bound_t => Expr::Binary(
                Box::new(Expr::Binary(
                    Box::new(Expr::Ident("t".into())),
                    BinaryOp::Sub,
                    Box::new(Expr::Num(self.origin_seconds)),
                )),
                BinaryOp::Div,
                Box::new(Expr::Num(self.rate)),
            ),
            Expr::Closure(params, body) => {
                let previous = self.bound_t;
                self.bound_t |= params.iter().any(|param| param == "t");
                let body = self.fold_expr(*body);
                self.bound_t = previous;
                Expr::Closure(params, Box::new(body))
            },
            Expr::LetChain(bindings, tail) => {
                let previous = self.bound_t;
                let mut folded = Vec::with_capacity(bindings.len());
                for (name, value) in bindings {
                    let value = self.fold_expr(value);
                    if name == "t" {
                        self.bound_t = true;
                    }
                    folded.push((name, value));
                }
                let tail = self.fold_expr(*tail);
                self.bound_t = previous;
                Expr::LetChain(folded, Box::new(tail))
            },
            other => walk_fold_expr(self, other),
        }
    }

    fn fold_inline_item(&mut self, item: InlineItem) -> InlineItem {
        match item {
            InlineItem::ForLoop {
                var,
                index_var,
                iterable,
                body,
            } => {
                let iterable = self.fold_expr(iterable);
                let previous = self.bound_t;
                self.bound_t |= loop_binds_t(&var, index_var.as_deref());
                let body = body.into_iter().map(|item| self.fold_inline_item(item)).collect();
                self.bound_t = previous;
                InlineItem::ForLoop {
                    var,
                    index_var,
                    iterable,
                    body,
                }
            },
            other => animatix_syntax::visit::walk_fold_inline_item(self, other),
        }
    }
}

fn loop_binds_t(pattern: &crate::ast::LoopPattern, index_var: Option<&str>) -> bool {
    index_var == Some("t")
        || match pattern {
            crate::ast::LoopPattern::Single(name) => name == "t",
            crate::ast::LoopPattern::Tuple(names) => names.iter().any(|name| name == "t"),
        }
}

pub(super) fn plan_timing_error(message: &str, span: Option<Span>) -> Diagnostic {
    Diagnostic::error(DiagnosticCode::InvalidModifierValue, DiagnosticPhase::Build, message)
        .with_ast_span(span)
}

pub(super) fn plan_contains_audio(plan: &TimelinePlan) -> bool {
    fn statements_contain_audio(body: &[Stmt]) -> bool {
        struct Finder(bool);
        impl AstVisitor<'_> for Finder {
            fn visit_stmt(&mut self, statement: &Stmt) {
                if matches!(statement, Stmt::ActorDecl { ty, .. } if ty == "Audio") {
                    self.0 = true;
                }
                walk_stmt(self, statement);
            }

            fn visit_inline_item(&mut self, item: &InlineItem) {
                if matches!(item, InlineItem::Anonymous { ty, .. } | InlineItem::Labeled { ty, .. } if ty == "Audio")
                {
                    self.0 = true;
                }
                walk_inline_item(self, item);
            }
        }
        let mut finder = Finder(false);
        for statement in body {
            finder.visit_stmt(statement);
            if finder.0 {
                return true;
            }
        }
        false
    }

    plan.ops().iter().any(|op| match op {
        PlanOp::Statements(body) => statements_contain_audio(body),
        PlanOp::Sequence(children) | PlanOp::Stagger { children, .. } => {
            children.iter().any(|child| plan_contains_audio(child))
        },
        PlanOp::Retimed { plan, .. } => plan_contains_audio(plan),
        PlanOp::Builtin(_) | PlanOp::Property { .. } => false,
    })
}

#[cfg(test)]
mod extent_tests {
    use super::*;

    #[test]
    fn computed_marker_uses_a_prior_plan_local_duration_binding() {
        let mut env = Environment::new();
        env.set("pause", Value::Duration(2.0));
        let body = crate::ast::PlanBody::new(vec![
            Stmt::LetDecl {
                is_pub: false,
                name: "pause".into(),
                value: Expr::Duration(0.5),
                span: None,
            },
            Stmt::Keyframe {
                time: Time::Expression(Box::new(Expr::Ident("pause".into()))),
                body: Vec::new(),
                span: None,
            },
        ]);

        let plan = TimelinePlan::capture_plan_body(body, &env);

        assert_eq!(plan.extent().map(PlanDuration::milliseconds), Some(500.0));
        assert_eq!(env.get("pause"), Some(Value::Duration(2.0)));
    }
}
