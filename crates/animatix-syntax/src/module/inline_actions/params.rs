use super::*;

pub(super) fn let_stmt(name: &str, value: Expr) -> Stmt {
    Stmt::LetDecl {
        is_pub: false,
        name: name.to_string(),
        value,
        span: None,
    }
}

/// Find parameters used as whole action/assignment targets (actor labels).
pub(super) fn collect_label_params(
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
pub(super) fn forwards_to_label_param(
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
pub(super) fn direct_label_params(
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
