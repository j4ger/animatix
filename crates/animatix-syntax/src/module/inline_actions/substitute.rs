use super::*;

pub(super) fn rewrite_self_targets(body: Vec<Stmt>, target: &str) -> Vec<Stmt> {
    let known_labels = std::collections::HashSet::new();
    let bindings = HashMap::new();
    body.into_iter()
        .map(|stmt| {
            crate::module::rewrite::rewrite_stmt(&stmt, target, None, &known_labels, &bindings)
        })
        .collect()
}

/// Check if an expression is a time literal (e.g. `200ms`, `2s`).
pub(super) fn is_time_expr(expr: &Expr) -> bool {
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

pub(super) fn substitute_params_in_stmt(stmt: &Stmt, bindings: &HashMap<String, Expr>) -> Stmt {
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
pub(super) fn substitute_label_in_target(target: &str, bindings: &HashMap<String, Expr>) -> String {
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

pub(super) fn substitute_params_in_target(
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

pub(super) fn substitute_params_in_expr(expr: &Expr, bindings: &HashMap<String, Expr>) -> Expr {
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

pub(super) fn substitute_params_in_modifier(
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
pub(super) fn apply_modifiers_to_body(
    body: Vec<Stmt>,
    invocation_modifiers: &[Modifier],
) -> Vec<Stmt> {
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
