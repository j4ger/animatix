use super::*;

pub(super) fn expand_stmt_list(
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

pub(super) fn expand_stmt(
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
pub(super) fn expand_target_call(
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
pub(super) fn expand_arg_call(
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
pub(super) fn expand_with_params(
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
