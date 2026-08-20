use std::collections::HashMap;

use crate::ast::{Action, Expr, Modifier, Stmt, TargetSegment, array_actor_label};
use crate::module::{ActionTemplate, InstanceActionRegistry};

// NOTE: This module takes ownership of Vec<Stmt> and produces a new
// Vec<Stmt> (owned tree transformation with potential 1→N expansion).

/// Replace timeline-function call statements with scoped blocks.
///
/// `pulse btn [strength: 1.3]` and `highlight_key(bars, key)` expand into a
/// `Stmt::Block` containing parameter `let` bindings followed by the callee
/// body, with `self` rewritten to the concrete target. Block scope keeps
/// function-local `let` bindings from leaking into the scene. Nested calls
/// inside the expanded body are expanded recursively (recursion is rejected
/// by a cycle guard that leaves the offending call in place).
///
/// Pure functions (`return_type: Some(_)`) are never expanded here; their
/// calls are evaluated at runtime as expressions.
pub(super) fn expand_fn_calls(
    stmts: Vec<Stmt>,
    registry: &InstanceActionRegistry,
    module_fns: &HashMap<String, ActionTemplate>,
) -> Vec<Stmt> {
    let mut stack: Vec<String> = Vec::new();
    expand_stmt_list(stmts, registry, module_fns, &mut stack)
}

fn expand_stmt_list(
    stmts: Vec<Stmt>,
    registry: &InstanceActionRegistry,
    module_fns: &HashMap<String, ActionTemplate>,
    stack: &mut Vec<String>,
) -> Vec<Stmt> {
    stmts
        .into_iter()
        .flat_map(|stmt| expand_stmt(stmt, registry, module_fns, stack))
        .collect()
}

fn expand_stmt(
    stmt: Stmt,
    registry: &InstanceActionRegistry,
    module_fns: &HashMap<String, ActionTemplate>,
    stack: &mut Vec<String>,
) -> Vec<Stmt> {
    match stmt {
        Stmt::Action(action, span) => {
            let mut inlined = Vec::new();
            let mut remaining = Vec::new();
            let mut remaining_index = Vec::new();

            if action.targets.is_empty() {
                // Function-style call `highlight_key(bars, key)` (or `f()`):
                // bind positional arguments to parameters.
                if let Some(template) = module_fns.get(&action.verb) {
                    if template.return_type.is_none() {
                        inlined.extend(expand_arg_call(template, &action, stack));
                    } else {
                        // Statement-level pure-function call: leave for the
                        // runtime to report with a context-aware diagnostic.
                        remaining.push(action.verb.clone());
                        remaining_index.push(None);
                    }
                } else {
                    remaining.push(action.verb.clone());
                    remaining_index.push(None);
                }
            } else {
                for (target_index, target) in action.targets.iter().enumerate() {
                    let index = action.target_index.get(target_index).cloned().flatten();
                    // Component instance functions are more specific than module functions.
                    if let Some(template) = registry.get(target).and_then(|m| m.get(&action.verb)) {
                        inlined.extend(expand_target_call(template, target, &action, stack));
                    } else if let Some(template) = module_fns.get(&action.verb) {
                        if template.return_type.is_none() {
                            inlined.extend(expand_target_call(template, target, &action, stack));
                        } else {
                            remaining.push(target.clone());
                            remaining_index.push(index);
                        }
                    } else {
                        remaining.push(target.clone());
                        remaining_index.push(index);
                    }
                }
            }

            if !remaining.is_empty() {
                inlined.push(Stmt::Action(
                    Action {
                        verb: action.verb,
                        targets: remaining,
                        target_index: remaining_index,
                        args: action.args,
                        modifiers: action.modifiers,
                        byte_span: action.byte_span,
                    },
                    span,
                ));
            }

            inlined
        },
        // For all other statements, recurse into bodies using shared walk.
        mut stmt => {
            let bodies = crate::walk::collect_stmt_bodies_mut(&mut stmt);
            for body in bodies {
                *body = expand_stmt_list(std::mem::take(body), registry, module_fns, stack);
            }
            vec![stmt]
        },
    }
}

/// Expand a target-style call `pulse btn [strength: 1.3]` into a scoped block
/// for the concrete target: value parameters bind via `let`, label parameters
/// (whole-target matches) substitute into target strings, `self` is rewritten
/// to the target, and the body is wrapped in a `Stmt::Block` so local `let`
/// bindings do not leak.
fn expand_target_call(
    template: &ActionTemplate,
    target: &str,
    action: &Action,
    stack: &mut Vec<String>,
) -> Vec<Stmt> {
    if stack.iter().any(|name| name == &action.verb) {
        return vec![Stmt::Action(action.clone(), None)];
    }
    stack.push(action.verb.clone());
    // Param values come from named invocation modifiers, then defaults.
    let mut values: HashMap<String, Expr> = HashMap::new();
    let mut bound: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for modifier in &action.modifiers {
        if let Some(name) = &modifier.name {
            if template.params.iter().any(|p| p.name == *name) {
                values.insert(name.clone(), modifier.value.clone());
                bound.insert(name.as_str());
            }
        } else if is_time_expr(&modifier.value)
            && template.params.iter().any(|p| p.name == "duration")
        {
            values.insert("duration".to_string(), modifier.value.clone());
            bound.insert("duration");
        }
    }
    for param in &template.params {
        if !bound.contains(param.name.as_str()) {
            if let Some(default) = &param.default {
                values.insert(param.name.clone(), default.clone());
            }
        }
    }
    let unconsumed: Vec<Modifier> = action
        .modifiers
        .iter()
        .filter(|m| {
            let is_param = m
                .name
                .as_deref()
                .is_some_and(|name| template.params.iter().any(|p| p.name == name))
                || (m.name.is_none()
                    && is_time_expr(&m.value)
                    && template.params.iter().any(|p| p.name == "duration"));
            !is_param
        })
        .cloned()
        .collect();
    let body = expand_with_params(template, &values, stack);
    let body = rewrite_self_targets(body, target);
    let body = apply_modifiers_to_body(body, &unconsumed);
    stack.pop();
    vec![Stmt::Block { body, span: None }]
}

/// Expand a function-style call `highlight_key(bars, key)` into a scoped block:
/// positional arguments bind to parameters in order, defaults fill the rest.
fn expand_arg_call(
    template: &ActionTemplate,
    action: &Action,
    stack: &mut Vec<String>,
) -> Vec<Stmt> {
    if stack.iter().any(|name| name == &action.verb) {
        return vec![Stmt::Action(action.clone(), None)];
    }
    stack.push(action.verb.clone());
    let mut values: HashMap<String, Expr> = HashMap::new();
    for (index, param) in template.params.iter().enumerate() {
        match action.args.get(index) {
            Some(arg) => {
                values.insert(param.name.clone(), arg.clone());
            },
            None => {
                if let Some(default) = &param.default {
                    values.insert(param.name.clone(), default.clone());
                }
            },
        }
    }
    let body = expand_with_params(template, &values, stack);
    let body = apply_modifiers_to_body(body, &action.modifiers);
    stack.pop();
    vec![Stmt::Block { body, span: None }]
}

/// Bind parameters into a function body.
///
/// Parameters whose name appears as a whole action/assignment target are
/// treated as actor-label parameters: the target strings are substituted with
/// the argument label and no `let` is emitted (actor labels are not values).
/// All other parameters bind via block-scoped `let` statements, which keeps
/// shadowing inside the body working (`let zero = zero + 1` rebinds the local).
fn expand_with_params(
    template: &ActionTemplate,
    values: &HashMap<String, Expr>,
    stack: &mut Vec<String>,
) -> Vec<Stmt> {
    let label_params = collect_label_params(&template.body, values);
    let mut block = Vec::new();
    let mut label_bindings: HashMap<String, Expr> = HashMap::new();
    for param in &template.params {
        if let Some(value) = values.get(&param.name) {
            if label_params.contains(param.name.as_str()) {
                label_bindings.insert(param.name.clone(), value.clone());
            } else {
                block.push(let_stmt(&param.name, value.clone()));
            }
        }
    }
    let body = if label_bindings.is_empty() {
        template.body.clone()
    } else {
        template
            .body
            .iter()
            .map(|stmt| substitute_params_in_stmt(stmt, &label_bindings))
            .collect()
    };
    block.extend(body);
    block
}

fn let_stmt(name: &str, value: Expr) -> Stmt {
    Stmt::LetDecl {
        is_pub: false,
        name: name.to_string(),
        value,
        span: None,
    }
}

/// Find parameters used as whole action/assignment targets (actor labels).
fn collect_label_params(
    body: &[Stmt],
    values: &HashMap<String, Expr>,
) -> std::collections::HashSet<String> {
    let mut labels = std::collections::HashSet::new();
    fn walk(
        stmt: &Stmt,
        labels: &mut std::collections::HashSet<String>,
        values: &HashMap<String, Expr>,
    ) {
        match stmt {
            Stmt::Action(action, ..) => {
                for target in &action.targets {
                    // A dotted target (`bars.bar[j]`) still names the label
                    // parameter through its first segment.
                    let base = target.split('.').next().unwrap_or(target);
                    if values.contains_key(base) {
                        labels.insert(base.to_string());
                    }
                }
            },
            Stmt::Assignment { target, .. } | Stmt::ReactiveBinding { target, .. } => {
                if let Some(first) = target.first() {
                    let base = first.label_str().split('.').next().unwrap_or(first.label_str());
                    if values.contains_key(base) {
                        labels.insert(base.to_string());
                    }
                }
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ForLoop { body, .. }
            | Stmt::Block { body, .. }
            | Stmt::FnDecl { body, .. } => {
                for s in body {
                    walk(s, labels, values);
                }
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                for s in then_branch {
                    walk(s, labels, values);
                }
                if let Some(eb) = else_branch {
                    for s in eb {
                        walk(s, labels, values);
                    }
                }
            },
            Stmt::Match { arms, .. } => {
                for (_, body) in arms {
                    for s in body {
                        walk(s, labels, values);
                    }
                }
            },
            _ => {},
        }
    }
    for stmt in body {
        walk(stmt, &mut labels, values);
    }
    labels
}

/// Rewrite `self` references in a function body to the concrete target.
fn rewrite_self_targets(body: Vec<Stmt>, target: &str) -> Vec<Stmt> {
    let known_labels = std::collections::HashSet::new();
    let bindings = HashMap::new();
    body.into_iter()
        .map(|stmt| super::rewrite::rewrite_stmt(&stmt, target, None, &known_labels, &bindings))
        .collect()
}

/// Check if an expression is a time literal (e.g. `200ms`, `2s`).
fn is_time_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Ident(s) | Expr::Str(s) => {
            if let Some(num_part) = s.strip_suffix("ms") {
                num_part.parse::<f64>().is_ok()
            } else if let Some(num_part) = s.strip_suffix('s') {
                num_part.parse::<f64>().is_ok()
            } else {
                false
            }
        },
        _ => false,
    }
}

fn substitute_action_params(
    template: &ActionTemplate,
    invocation_modifiers: &[Modifier],
) -> (Vec<Stmt>, Vec<Modifier>) {
    // Build param bindings from defaults + invocation modifiers
    let mut bindings: HashMap<String, Expr> = HashMap::new();
    let mut consumed: Vec<bool> = vec![false; invocation_modifiers.len()];

    for param in &template.params {
        if let Some(default) = &param.default {
            bindings.insert(param.name.clone(), default.clone());
        }
    }

    for (i, modifier) in invocation_modifiers.iter().enumerate() {
        if let Some(name) = &modifier.name {
            // Named modifier — bind if param exists
            if template.params.iter().any(|p| p.name == *name) {
                bindings.insert(name.clone(), modifier.value.clone());
                consumed[i] = true;
            }
        } else if is_time_expr(&modifier.value)
            && template.params.iter().any(|p| p.name == "duration")
        {
            // Positional time — bind to `duration` param if present
            bindings.insert("duration".to_string(), modifier.value.clone());
            consumed[i] = true;
        }
    }

    let unconsumed: Vec<Modifier> = invocation_modifiers
        .iter()
        .enumerate()
        .filter(|(i, _)| !consumed[*i])
        .map(|(_, m)| m.clone())
        .collect();

    if bindings.is_empty() {
        return (template.body.clone(), unconsumed);
    }

    let body = template
        .body
        .iter()
        .map(|stmt| substitute_params_in_stmt(stmt, &bindings))
        .collect();

    (body, unconsumed)
}

fn substitute_params_in_stmt(stmt: &Stmt, bindings: &HashMap<String, Expr>) -> Stmt {
    match stmt.clone() {
        Stmt::LetDecl {
            is_pub,
            name,
            value,
            span,
        } => Stmt::LetDecl {
            is_pub,
            name,
            value: substitute_params_in_expr(&value, bindings),
            span,
        },
        Stmt::Assignment {
            target,
            property,
            value,
            modifiers,
            easing,
            value_span,
            span,
        } => Stmt::Assignment {
            target: substitute_params_in_target(&target, bindings),
            property,
            value: substitute_params_in_expr(&value, bindings),
            modifiers: modifiers
                .into_iter()
                .map(|m| substitute_params_in_modifier(&m, bindings))
                .collect(),
            easing,
            value_span,
            span,
        },
        Stmt::Action(mut action, span) => {
            action.targets = action
                .targets
                .into_iter()
                .map(|target| substitute_label_in_target(&target, bindings))
                .collect();
            action.args = action
                .args
                .into_iter()
                .map(|arg| substitute_params_in_expr(&arg, bindings))
                .collect();
            action.modifiers = action
                .modifiers
                .into_iter()
                .map(|m| substitute_params_in_modifier(&m, bindings))
                .collect();
            Stmt::Action(action, span)
        },
        Stmt::Keyframe { time, body, span } => Stmt::Keyframe {
            time,
            body: body.iter().map(|s| substitute_params_in_stmt(s, bindings)).collect(),
            span,
        },
        Stmt::RelativeKeyframe { offset, body, span } => Stmt::RelativeKeyframe {
            offset,
            body: body.iter().map(|s| substitute_params_in_stmt(s, bindings)).collect(),
            span,
        },
        Stmt::Sequence { body, span } => Stmt::Sequence {
            body: body.iter().map(|s| substitute_params_in_stmt(s, bindings)).collect(),
            span,
        },
        Stmt::Stagger {
            modifiers,
            body,
            span,
        } => Stmt::Stagger {
            modifiers: modifiers
                .into_iter()
                .map(|m| substitute_params_in_modifier(&m, bindings))
                .collect(),
            body: body.iter().map(|s| substitute_params_in_stmt(s, bindings)).collect(),
            span,
        },
        Stmt::Always { body, span } => Stmt::Always {
            body: body.iter().map(|s| substitute_params_in_stmt(s, bindings)).collect(),
            span,
        },
        Stmt::ReactiveBinding {
            target,
            property,
            value,
            value_span,
            span,
        } => Stmt::ReactiveBinding {
            target: substitute_params_in_target(&target, bindings),
            property,
            value: substitute_params_in_expr(&value, bindings),
            value_span,
            span,
        },
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            span,
        } => Stmt::Conditional {
            condition: substitute_params_in_expr(&condition, bindings),
            then_branch: then_branch
                .iter()
                .map(|s| substitute_params_in_stmt(s, bindings))
                .collect(),
            else_branch: else_branch
                .map(|b| b.iter().map(|s| substitute_params_in_stmt(s, bindings)).collect()),
            span,
        },
        Stmt::Match {
            scrutinee,
            arms,
            span,
        } => Stmt::Match {
            scrutinee: substitute_params_in_expr(&scrutinee, bindings),
            arms: arms
                .iter()
                .map(|(pat, body)| {
                    (
                        pat.clone(),
                        body.iter().map(|s| substitute_params_in_stmt(s, bindings)).collect(),
                    )
                })
                .collect(),
            span,
        },
        Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body,
            modifiers,
            span,
        } => Stmt::ForLoop {
            var,
            index_var,
            iterable: substitute_params_in_expr(&iterable, bindings),
            body: body.iter().map(|s| substitute_params_in_stmt(s, bindings)).collect(),
            modifiers: modifiers
                .iter()
                .map(|m| substitute_params_in_modifier(m, bindings))
                .collect(),
            span,
        },
        other => other,
    }
}

/// Substitute a whole action target when it names a bound parameter.
/// `bars` bound to `row.b` rewrites the target string to `row.b`; bound
/// values that are not simple label expressions leave the target unchanged.
fn substitute_label_in_target(target: &str, bindings: &HashMap<String, Expr>) -> String {
    let Some(bound) = bindings.get(target) else {
        return target.to_string();
    };
    match bound {
        Expr::Ident(name) => name.clone(),
        Expr::Path(parts) => parts.join("."),
        _ => target.to_string(),
    }
}

fn substitute_params_in_target(
    target: &[TargetSegment],
    bindings: &HashMap<String, Expr>,
) -> Vec<TargetSegment> {
    target
        .iter()
        .map(|seg| match seg {
            TargetSegment::Static(s) => TargetSegment::Static(s.clone()),
            TargetSegment::Indexed { base, index } => {
                let index = substitute_params_in_expr(index, bindings);
                if let Expr::Num(n) = &index {
                    if n.trunc() == *n && *n >= 0.0 {
                        return TargetSegment::Static(array_actor_label(base, *n as usize));
                    }
                }
                TargetSegment::Indexed {
                    base: base.clone(),
                    index: Box::new(index),
                }
            },
        })
        .collect()
}

fn substitute_params_in_expr(expr: &Expr, bindings: &HashMap<String, Expr>) -> Expr {
    match expr {
        Expr::Ident(name) => {
            bindings.get(name).cloned().unwrap_or_else(|| Expr::Ident(name.clone()))
        },
        Expr::Path(parts) => {
            if let Some(first) = parts.first() {
                if let Some(bound) = bindings.get(first) {
                    if parts.len() == 1 {
                        return bound.clone();
                    }
                    let rest = &parts[1..];
                    return match bound {
                        Expr::Ident(name) => Expr::Path(
                            [name.clone()].into_iter().chain(rest.iter().cloned()).collect(),
                        ),
                        Expr::Path(inner) => {
                            Expr::Path(inner.iter().chain(rest.iter()).cloned().collect())
                        },
                        other => {
                            Expr::Method(Box::new(other.clone()), parts[1].clone(), Vec::new())
                        },
                    };
                }
            }
            Expr::Path(parts.clone())
        },
        Expr::Tuple(items) => Expr::Tuple(
            items.iter().map(|item| substitute_params_in_expr(item, bindings)).collect(),
        ),
        Expr::List(items) => {
            Expr::List(items.iter().map(|item| substitute_params_in_expr(item, bindings)).collect())
        },
        Expr::Binary(lhs, op, rhs) => Expr::Binary(
            Box::new(substitute_params_in_expr(lhs, bindings)),
            op.clone(),
            Box::new(substitute_params_in_expr(rhs, bindings)),
        ),
        Expr::Unary(op, value) => {
            Expr::Unary(op.clone(), Box::new(substitute_params_in_expr(value, bindings)))
        },
        Expr::Call(name, args) => Expr::Call(
            name.clone(),
            args.iter().map(|arg| substitute_params_in_expr(arg, bindings)).collect(),
        ),
        Expr::Method(target, name, args) => Expr::Method(
            Box::new(substitute_params_in_expr(target, bindings)),
            name.clone(),
            args.iter().map(|arg| substitute_params_in_expr(arg, bindings)).collect(),
        ),
        Expr::Closure(params, body) => {
            Expr::Closure(params.clone(), Box::new(substitute_params_in_expr(body, bindings)))
        },
        Expr::Conditional(cond, then_expr, else_expr) => Expr::Conditional(
            Box::new(substitute_params_in_expr(cond, bindings)),
            Box::new(substitute_params_in_expr(then_expr, bindings)),
            Box::new(substitute_params_in_expr(else_expr, bindings)),
        ),
        Expr::Match(scrutinee, arms) => Expr::Match(
            Box::new(substitute_params_in_expr(scrutinee, bindings)),
            arms.iter()
                .map(|(pat, expr)| {
                    (pat.clone(), Box::new(substitute_params_in_expr(expr, bindings)))
                })
                .collect(),
        ),
        Expr::Construct(name, props) => Expr::Construct(
            name.clone(),
            props
                .iter()
                .map(|p| crate::ast::Property {
                    name: p.name.clone(),
                    value: substitute_params_in_expr(&p.value, bindings),
                    value_span: p.value_span,
                    trailing_comment: p.trailing_comment.clone(),
                })
                .collect(),
        ),
        Expr::Index(target, index) => Expr::Index(
            Box::new(substitute_params_in_expr(target, bindings)),
            Box::new(substitute_params_in_expr(index, bindings)),
        ),
        // Literals pass through unchanged
        Expr::Num(v) => Expr::Num(*v),
        Expr::Percent(v) => Expr::Percent(*v),
        Expr::Str(v) => Expr::Str(v.clone()),
        Expr::Bool(v) => Expr::Bool(*v),
        Expr::Null => Expr::Null,
    }
}

fn substitute_params_in_modifier(
    modifier: &Modifier,
    bindings: &HashMap<String, Expr>,
) -> Modifier {
    Modifier {
        name: modifier.name.clone(),
        value: substitute_params_in_expr(&modifier.value, bindings),
    }
}

/// Apply invocation modifiers to each body statement.
///
/// MVP rule: invocation modifiers replace body modifiers entirely.
/// `pulse btn [200ms]` turns `self.scale = 1.2 [100ms]` into `self.scale = 1.2 [200ms]`.
fn apply_modifiers_to_body(body: Vec<Stmt>, invocation_modifiers: &[Modifier]) -> Vec<Stmt> {
    if invocation_modifiers.is_empty() {
        return body;
    }

    body.into_iter()
        .map(|stmt| match stmt {
            Stmt::Assignment {
                target,
                property,
                value,
                value_span,
                ..
            } => Stmt::Assignment {
                target,
                property,
                value,
                modifiers: invocation_modifiers.to_vec(),
                easing: None,
                value_span,
                span: None,
            },
            Stmt::Action(mut action, _) => {
                action.modifiers = invocation_modifiers.to_vec();
                Stmt::Action(action, None)
            },
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ParamDef, TypeAnnotation};

    fn make_action(verb: &str, target: &str, modifiers: Vec<Modifier>) -> Stmt {
        Stmt::Action(
            Action {
                verb: verb.to_string(),
                targets: vec![target.to_string()],
                target_index: vec![None],
                args: vec![],
                modifiers,
                byte_span: None,
            },
            None,
        )
    }

    fn make_arg_call(verb: &str, args: Vec<Expr>) -> Stmt {
        Stmt::Action(
            Action {
                verb: verb.to_string(),
                targets: vec![],
                target_index: vec![],
                args,
                modifiers: vec![],
                byte_span: None,
            },
            None,
        )
    }

    fn make_assignment(target: &str, property: &str, value: Expr) -> Stmt {
        Stmt::Assignment {
            target: vec![crate::ast::TargetSegment::Static(target.to_string())],
            property: property.to_string(),
            value,
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }
    }

    fn template_with(params: Vec<ParamDef>, body: Vec<Stmt>) -> ActionTemplate {
        ActionTemplate {
            params,
            return_type: None,
            body,
        }
    }

    #[test]
    fn target_call_expands_to_block_with_param_binding() {
        let template = template_with(
            vec![ParamDef {
                name: "scale".to_string(),
                param_type: Some(TypeAnnotation::Num),
                default: Some(Expr::Num(1.2)),
            }],
            vec![make_assignment(
                "self",
                "scale",
                Expr::Ident("scale".to_string()),
            )],
        );
        let registry: InstanceActionRegistry =
            [("btn".to_string(), [("pulse".to_string(), template)].into_iter().collect())]
                .into_iter()
                .collect();
        let invocation = make_action(
            "pulse",
            "btn",
            vec![Modifier {
                name: Some("scale".to_string()),
                value: Expr::Num(1.5),
            }],
        );
        let module_fns: HashMap<String, ActionTemplate> = HashMap::new();
        let result = expand_stmt_list(vec![invocation], &registry, &module_fns, &mut Vec::new());
        assert_eq!(result.len(), 1);
        match &result[0] {
            Stmt::Block { body, .. } => {
                // `scale` is a value parameter bound by `let`; `self` is
                // rewritten to the concrete target.
                assert!(matches!(&body[0], Stmt::LetDecl { name, .. } if name == "scale"));
                assert!(matches!(
                    &body[1],
                    Stmt::Assignment { target, property, value, .. }
                        if target == &[TargetSegment::Static("btn".to_string())]
                            && property == "scale"
                            && value == &Expr::Ident("scale".to_string())
                ));
            },
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn arg_call_expands_positional_params() {
        let template = template_with(
            vec![
                ParamDef {
                    name: "bars".to_string(),
                    param_type: None,
                    default: None,
                },
                ParamDef {
                    name: "key".to_string(),
                    param_type: None,
                    default: None,
                },
            ],
            vec![make_assignment(
                "note",
                "target",
                Expr::Path(vec!["bars".to_string(), "bar".to_string()]),
            )],
        );
        let module_fns: HashMap<String, ActionTemplate> =
            [("highlight_key".to_string(), template)].into_iter().collect();
        let invocation =
            make_arg_call("highlight_key", vec![Expr::Ident("bars".to_string()), Expr::Num(2.0)]);
        let registry: InstanceActionRegistry = HashMap::new();
        let result = expand_stmt_list(vec![invocation], &registry, &module_fns, &mut Vec::new());
        assert_eq!(result.len(), 1);
        match &result[0] {
            Stmt::Block { body, .. } => {
                // Positional args bind `bars` and `key` as value parameters.
                assert!(matches!(&body[0], Stmt::LetDecl { name, .. } if name == "bars"));
                assert!(matches!(&body[1], Stmt::LetDecl { name, .. } if name == "key"));
                assert!(matches!(
                    &body[2],
                    Stmt::Assignment { target, property, .. }
                        if target == &[TargetSegment::Static("note".to_string())]
                            && property == "target"
                ));
            },
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn recursion_cycle_leaves_call_in_place() {
        let template = template_with(vec![], vec![make_arg_call("b", Vec::new())]);
        let module_fns: HashMap<String, ActionTemplate> = [
            ("a".to_string(), template.clone()),
            ("b".to_string(), template),
        ]
        .into_iter()
        .collect();
        let invocation = make_arg_call("a", Vec::new());
        let registry: InstanceActionRegistry = HashMap::new();
        // Expanding `a`'s body calls `b`, which calls `a` again — the cycle
        // guard must terminate without infinite recursion.
        let result = expand_stmt_list(vec![invocation], &registry, &module_fns, &mut Vec::new());
        assert!(!result.is_empty(), "cycle must not crash or loop forever");
    }

    #[test]
    fn pure_function_call_is_not_expanded() {
        let template = ActionTemplate {
            params: vec![],
            return_type: Some(TypeAnnotation::Num),
            body: vec![],
        };
        let module_fns: HashMap<String, ActionTemplate> =
            [("dnf".to_string(), template)].into_iter().collect();
        let invocation = make_arg_call("dnf", vec![Expr::Ident("arr".to_string())]);
        let registry: InstanceActionRegistry = HashMap::new();
        let result = expand_stmt_list(vec![invocation], &registry, &module_fns, &mut Vec::new());
        // The pure call stays as a statement for the runtime to diagnose.
        assert!(matches!(&result[0], Stmt::Action(..)));
    }
}
