use std::collections::HashMap;

use crate::ast::{Action, Expr, Modifier, Stmt, TargetSegment, array_actor_label};
use crate::module::{FnTemplate, InstanceFnRegistry};

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
    registry: &InstanceFnRegistry,
    module_fns: &HashMap<String, FnTemplate>,
    diagnostics: &mut Vec<String>,
) -> Vec<Stmt> {
    let cycle_members = detect_fn_cycles(module_fns, diagnostics);
    let mut stack: Vec<String> = Vec::new();
    expand_stmt_list(stmts, registry, module_fns, &cycle_members, &mut stack, diagnostics).0
}

/// Detect recursive timeline-function call cycles before expansion.
///
/// The expander flattens nested calls across re-scan passes, so a runtime
/// call-stack guard cannot see cycles; report them up front instead.
fn detect_fn_cycles(
    module_fns: &HashMap<String, FnTemplate>,
    diagnostics: &mut Vec<String>,
) -> std::collections::HashSet<String> {
    fn called_names(stmt: &Stmt, out: &mut Vec<String>) {
        match stmt {
            Stmt::Action(action, ..) => out.push(action.verb.clone()),
            Stmt::LetDecl { value, .. } => collect_calls_in_expr(value, out),
            Stmt::Assignment { value, .. } => collect_calls_in_expr(value, out),
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                collect_calls_in_expr(condition, out);
                for s in then_branch {
                    called_names(s, out);
                }
                if let Some(eb) = else_branch {
                    for s in eb {
                        called_names(s, out);
                    }
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                collect_calls_in_expr(scrutinee, out);
                for (_, body) in arms {
                    for s in body {
                        called_names(s, out);
                    }
                }
            },
            Stmt::ForLoop { iterable, body, .. } => {
                collect_calls_in_expr(iterable, out);
                for s in body {
                    called_names(s, out);
                }
            },
            Stmt::Block { body, .. } => {
                for s in body {
                    called_names(s, out);
                }
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::FnDecl { body, .. }
            | Stmt::Scene { body, .. } => {
                for s in body {
                    called_names(s, out);
                }
            },
            _ => {},
        }
    }
    fn collect_calls_in_expr(expr: &crate::ast::Expr, out: &mut Vec<String>) {
        match expr {
            crate::ast::Expr::Call(name, args) => {
                out.push(name.clone());
                for arg in args {
                    collect_calls_in_expr(arg, out);
                }
            },
            crate::ast::Expr::Tuple(items) | crate::ast::Expr::List(items) => {
                for item in items {
                    collect_calls_in_expr(item, out);
                }
            },
            crate::ast::Expr::Binary(l, _, r) => {
                collect_calls_in_expr(l, out);
                collect_calls_in_expr(r, out);
            },
            crate::ast::Expr::Unary(_, v) => collect_calls_in_expr(v, out),
            crate::ast::Expr::Method(receiver, _, args) => {
                collect_calls_in_expr(receiver, out);
                for arg in args {
                    collect_calls_in_expr(arg, out);
                }
            },
            crate::ast::Expr::Closure(_, body) => collect_calls_in_expr(body, out),
            crate::ast::Expr::LetChain(bindings, tail) => {
                for (_, value) in bindings {
                    collect_calls_in_expr(value, out);
                }
                collect_calls_in_expr(tail, out);
            },
            crate::ast::Expr::Conditional(c, t, e) => {
                collect_calls_in_expr(c, out);
                collect_calls_in_expr(t, out);
                collect_calls_in_expr(e, out);
            },
            crate::ast::Expr::Match(scrutinee, arms) => {
                collect_calls_in_expr(scrutinee, out);
                for (_, arm) in arms {
                    collect_calls_in_expr(arm, out);
                }
            },
            crate::ast::Expr::Construct(_, props) => {
                for prop in props {
                    collect_calls_in_expr(&prop.value, out);
                }
            },
            crate::ast::Expr::Index(target, index) => {
                collect_calls_in_expr(target, out);
                collect_calls_in_expr(index, out);
            },
            _ => {},
        }
    }

    // Build adjacency: fn -> fns it calls (only timeline fns call other fns).
    let mut graph: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (name, template) in module_fns {
        if template.return_type.is_some() {
            continue;
        }
        let mut calls = Vec::new();
        for stmt in &template.body {
            called_names(stmt, &mut calls);
        }
        graph.insert(name.clone(), calls);
    }
    // DFS cycle detection (white/gray/black).
    enum Color {
        White,
        Gray,
        Black,
    }
    let mut colors: std::collections::HashMap<&str, Color> = std::collections::HashMap::new();
    let mut stack: Vec<String> = Vec::new();
    let mut cycle_members: std::collections::HashSet<String> = std::collections::HashSet::new();
    fn visit<'a>(
        name: &'a str,
        graph: &'a std::collections::HashMap<String, Vec<String>>,
        colors: &mut std::collections::HashMap<&'a str, Color>,
        stack: &mut Vec<String>,
        cycle_members: &mut std::collections::HashSet<String>,
        diagnostics: &mut Vec<String>,
    ) {
        colors.insert(name, Color::Gray);
        stack.push(name.to_string());
        if let Some(callees) = graph.get(name) {
            for callee in callees {
                match colors.get(callee.as_str()).unwrap_or(&Color::White) {
                    Color::Gray => {
                        let cycle_start = stack.iter().position(|s| s == callee).unwrap_or(0);
                        let cycle = stack[cycle_start..].join(" -> ");
                        diagnostics.push(format!(
                            "recursive timeline-function cycle: {cycle} -> {callee}; recursion is not supported"
                        ));
                        for member in &stack[cycle_start..] {
                            cycle_members.insert(member.clone());
                        }
                        cycle_members.insert(callee.clone());
                    },
                    Color::White => visit(callee, graph, colors, stack, cycle_members, diagnostics),
                    Color::Black => {},
                }
            }
        }
        stack.pop();
        colors.insert(name, Color::Black);
    }
    for name in graph.keys() {
        if matches!(colors.get(name.as_str()).unwrap_or(&Color::White), Color::White) {
            visit(name, &graph, &mut colors, &mut stack, &mut cycle_members, diagnostics);
        }
    }
    cycle_members
}

fn expand_stmt_list(
    stmts: Vec<Stmt>,
    registry: &InstanceFnRegistry,
    module_fns: &HashMap<String, FnTemplate>,
    cycle_members: &std::collections::HashSet<String>,
    stack: &mut Vec<String>,
    diagnostics: &mut Vec<String>,
) -> (Vec<Stmt>, bool) {
    let mut expanded = true;
    let mut current = stmts;
    let mut guard = 0;
    let mut any_expanded = false;
    // Re-scan until a pass expands nothing (nested timeline-function calls
    // inside expanded blocks expand in later passes; built-in/unknown actions
    // terminate the loop). The guard is a safety net against expander bugs.
    while expanded && guard < 128 {
        let mut next = Vec::new();
        let mut did_expand = false;
        for stmt in current {
            let (out, expanded_any) =
                expand_stmt(stmt, registry, module_fns, cycle_members, stack, diagnostics);
            did_expand |= expanded_any;
            next.extend(out);
        }
        current = next;
        expanded = did_expand;
        any_expanded |= did_expand;
        guard += 1;
    }
    (current, any_expanded)
}

fn expand_stmt(
    stmt: Stmt,
    registry: &InstanceFnRegistry,
    module_fns: &HashMap<String, FnTemplate>,
    cycle_members: &std::collections::HashSet<String>,
    stack: &mut Vec<String>,
    diagnostics: &mut Vec<String>,
) -> (Vec<Stmt>, bool) {
    match stmt {
        Stmt::Action(action, span) => {
            let mut inlined = Vec::new();
            let mut remaining = Vec::new();
            let mut remaining_index = Vec::new();

            if action.targets.is_empty() {
                // Function-style call `highlight_key(bars, key)` (or `f()`):
                // bind positional arguments to parameters.
                if let Some(template) = module_fns.get(&action.verb) {
                    if template.return_type.is_none() && !cycle_members.contains(&action.verb) {
                        inlined.extend(expand_arg_call(template, &action, module_fns, stack));
                    } else if cycle_members.contains(&action.verb) {
                        // Cycle already reported by detect_fn_cycles; drop the
                        // call so it does not surface as "unknown action".
                    } else {
                        // Statement-level pure-function call cannot emit
                        // timeline events; report it clearly instead of
                        // falling through to "unknown action".
                        diagnostics.push(format!(
                            "pure function '{}' must be called in an expression, e.g. `let v = {}(...)`",
                            action.verb, action.verb
                        ));
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
                        inlined.extend(expand_target_call(
                            template, target, &action, module_fns, stack,
                        ));
                    } else if let Some(template) = module_fns.get(&action.verb) {
                        if template.return_type.is_none() {
                            inlined.extend(expand_target_call(
                                template, target, &action, module_fns, stack,
                            ));
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
            let did_expand = inlined.iter().any(|stmt| matches!(stmt, Stmt::Block { .. }));
            (inlined, did_expand)
        },
        // For all other statements, recurse into bodies using shared walk.
        Stmt::FnDecl { .. } => {
            // Function declarations are templates expanded at call sites;
            // expanding them in place would re-expand on every pass.
            (vec![stmt], false)
        },
        mut stmt => {
            let bodies = crate::walk::collect_stmt_bodies_mut(&mut stmt);
            let mut did_expand = false;
            for body in bodies {
                let (next, expanded_any) = expand_stmt_list(
                    std::mem::take(body),
                    registry,
                    module_fns,
                    cycle_members,
                    stack,
                    diagnostics,
                );
                did_expand |= expanded_any;
                *body = next;
            }
            (vec![stmt], did_expand)
        },
    }
}

/// Expand a target-style call `pulse btn [strength: 1.3]` into a scoped block
/// for the concrete target: value parameters bind via `let`, label parameters
/// (whole-target matches) substitute into target strings, `self` is rewritten
/// to the target, and the body is wrapped in a `Stmt::Block` so local `let`
/// bindings do not leak.
fn expand_target_call(
    template: &FnTemplate,
    target: &str,
    action: &Action,
    module_fns: &HashMap<String, FnTemplate>,
    stack: &mut Vec<String>,
) -> Vec<Stmt> {
    if stack.iter().any(|name| name == &action.verb) {
        // Cycle already reported by detect_fn_cycles; drop the call so it
        // does not surface as a confusing "unknown action".
        return Vec::new();
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
    let body = expand_with_params(template, &values, module_fns);
    let body = rewrite_self_targets(body, target);
    let body = apply_modifiers_to_body(body, &unconsumed);
    stack.pop();
    vec![Stmt::Block { body, span: None }]
}

/// Expand a function-style call `highlight_key(bars, key)` into a scoped block:
/// positional arguments bind to parameters in order, defaults fill the rest.
fn expand_arg_call(
    template: &FnTemplate,
    action: &Action,
    module_fns: &HashMap<String, FnTemplate>,
    stack: &mut Vec<String>,
) -> Vec<Stmt> {
    if stack.iter().any(|name| name == &action.verb) {
        // Cycle already reported by detect_fn_cycles; drop the call so it
        // does not surface as a confusing "unknown action".
        return Vec::new();
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
    let body = expand_with_params(template, &values, module_fns);
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
    template: &FnTemplate,
    values: &HashMap<String, Expr>,
    module_fns: &HashMap<String, FnTemplate>,
) -> Vec<Stmt> {
    let label_params = collect_label_params(&template.body, values, module_fns);
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
    module_fns: &HashMap<String, FnTemplate>,
) -> std::collections::HashSet<String> {
    // Direct label params: names used as whole action/assignment targets.
    let mut labels = direct_label_params(body, values);

    // Transitive label params: a param passed positionally to a callee's
    // label parameter is itself a label (`run(b)` forwards `bars` into
    // `bubble_sort(bars, ...)` where `bars` is a label param of the callee).
    // Precompute each callee's label-param positions, then iterate to a
    // fixpoint (chains of forwarding functions).
    let callee_label_positions: HashMap<&str, Vec<bool>> = module_fns
        .iter()
        .map(|(name, template)| {
            let callee_params: HashMap<String, Expr> = template
                .params
                .iter()
                .map(|p| (p.name.clone(), Expr::Ident(p.name.clone())))
                .collect();
            let callee_labels = direct_label_params(&template.body, &callee_params);
            let positions =
                template.params.iter().map(|p| callee_labels.contains(&p.name)).collect();
            (name.as_str(), positions)
        })
        .collect();

    let mut changed = true;
    while changed {
        changed = false;
        for param in values.keys().clone() {
            if labels.contains(param) {
                continue;
            }
            if forwards_to_label_param(body, param, &callee_label_positions) {
                labels.insert(param.clone());
                changed = true;
            }
        }
    }
    labels
}

/// Is `param` passed as an argument to any callee's label-param position?
fn forwards_to_label_param(
    body: &[Stmt],
    param: &str,
    callee_label_positions: &HashMap<&str, Vec<bool>>,
) -> bool {
    fn walk(stmt: &Stmt, param: &str, callee_label_positions: &HashMap<&str, Vec<bool>>) -> bool {
        match stmt {
            Stmt::Action(action, ..) if action.targets.is_empty() => {
                let Some(positions) = callee_label_positions.get(action.verb.as_str()) else {
                    return false;
                };
                for (position, arg) in action.args.iter().enumerate() {
                    if matches!(arg, Expr::Ident(name) if name == param)
                        && positions.get(position).copied().unwrap_or(false)
                    {
                        return true;
                    }
                }
                false
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                then_branch.iter().any(|s| walk(s, param, callee_label_positions))
                    || else_branch
                        .as_ref()
                        .is_some_and(|eb| eb.iter().any(|s| walk(s, param, callee_label_positions)))
            },
            Stmt::Match { arms, .. } => arms
                .iter()
                .any(|(_, arm)| arm.iter().any(|s| walk(s, param, callee_label_positions))),
            Stmt::ForLoop { body, .. }
            | Stmt::Block { body, .. }
            | Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::FnDecl { body, .. }
            | Stmt::Scene { body, .. } => {
                body.iter().any(|s| walk(s, param, callee_label_positions))
            },
            _ => false,
        }
    }
    body.iter().any(|s| walk(s, param, callee_label_positions))
}

/// Parameters used as whole action/assignment targets in a body.
fn direct_label_params(
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
    // Dotted targets (`bars.bar[j]`) substitute through their first segment,
    // matching the label-parameter detection in `collect_label_params`.
    let (base, rest) = target.split_once('.').unwrap_or((target, ""));
    let Some(bound) = bindings.get(base) else {
        return target.to_string();
    };
    let substituted = match bound {
        Expr::Ident(name) => name.clone(),
        Expr::Path(parts) => parts.join("."),
        _ => return target.to_string(),
    };
    if rest.is_empty() {
        substituted
    } else {
        format!("{substituted}.{rest}")
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
        Expr::LetChain(bindings_list, tail) => Expr::LetChain(
            bindings_list
                .iter()
                .map(|(name, value)| (name.clone(), substitute_params_in_expr(value, bindings)))
                .collect(),
            Box::new(substitute_params_in_expr(tail, bindings)),
        ),
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

    fn template_with(params: Vec<ParamDef>, body: Vec<Stmt>) -> FnTemplate {
        FnTemplate {
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
        let registry: InstanceFnRegistry =
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
        let module_fns: HashMap<String, FnTemplate> = HashMap::new();
        let result = expand_stmt_list(
            vec![invocation],
            &registry,
            &module_fns,
            &std::collections::HashSet::new(),
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .0;
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
        let module_fns: HashMap<String, FnTemplate> =
            [("highlight_key".to_string(), template)].into_iter().collect();
        let invocation =
            make_arg_call("highlight_key", vec![Expr::Ident("bars".to_string()), Expr::Num(2.0)]);
        let registry: InstanceFnRegistry = HashMap::new();
        let result = expand_stmt_list(
            vec![invocation],
            &registry,
            &module_fns,
            &std::collections::HashSet::new(),
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .0;
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
    fn recursion_cycle_is_diagnosed_and_dropped() {
        let template = template_with(vec![], vec![make_arg_call("b", Vec::new())]);
        let module_fns: HashMap<String, FnTemplate> = [
            ("a".to_string(), template.clone()),
            ("b".to_string(), template),
        ]
        .into_iter()
        .collect();
        let invocation = make_arg_call("a", Vec::new());
        let registry: InstanceFnRegistry = HashMap::new();
        // The cycle pre-pass reports the recursion and the expander drops the
        // calls, so the run must terminate without infinite recursion or a
        // surviving "unknown action".
        let mut diagnostics = Vec::new();
        let result = expand_fn_calls(vec![invocation], &registry, &module_fns, &mut diagnostics);
        assert!(
            diagnostics.iter().any(|d| d.contains("recursive")),
            "expected a recursion diagnostic, got: {diagnostics:?}"
        );
        assert!(
            !result.iter().any(|s| matches!(s, Stmt::Action(..))),
            "cycle calls must be dropped, got: {result:?}"
        );
    }

    #[test]
    fn pure_function_call_is_not_expanded() {
        let template = FnTemplate {
            params: vec![],
            return_type: Some(TypeAnnotation::Num),
            body: vec![],
        };
        let module_fns: HashMap<String, FnTemplate> =
            [("dnf".to_string(), template)].into_iter().collect();
        let invocation = make_arg_call("dnf", vec![Expr::Ident("arr".to_string())]);
        let registry: InstanceFnRegistry = HashMap::new();
        let mut diagnostics = Vec::new();
        let result = expand_stmt_list(
            vec![invocation],
            &registry,
            &module_fns,
            &std::collections::HashSet::new(),
            &mut Vec::new(),
            &mut diagnostics,
        )
        .0;
        // The pure call is diagnosed and dropped (no "unknown action" at runtime).
        assert!(
            diagnostics.iter().any(|d| d.contains("must be called in an expression")),
            "expected a pure-function statement diagnostic, got: {diagnostics:?}"
        );
        assert!(result.is_empty(), "pure statement call must be dropped");
    }
}
