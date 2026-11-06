use super::*;
use crate::visit::{AstFolder, walk_fold_stmt};

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

/// Substitutes parameter bindings through the statement kinds that component
/// templates can carry. Other statements are left untouched, matching the
/// template expander's original scope.
struct ParamSubstituter<'a> {
    bindings: &'a HashMap<String, Expr>,
}

impl AstFolder for ParamSubstituter<'_> {
    fn fold_expr(&mut self, expr: Expr) -> Expr {
        substitute_params_in_expr(&expr, self.bindings)
    }

    fn fold_action(&mut self, mut action: Action) -> Action {
        action.targets = action
            .targets
            .into_iter()
            .map(|target| substitute_label_in_target(&target, self.bindings))
            .collect();
        action.args = action.args.into_iter().map(|arg| self.fold_expr(arg)).collect();
        action.modifiers = action.modifiers.into_iter().map(|m| self.fold_modifier(m)).collect();
        action
    }

    fn fold_target_segment(&mut self, segment: TargetSegment) -> TargetSegment {
        match segment {
            TargetSegment::Static(_) => segment,
            TargetSegment::Indexed { base, index } => {
                let index = self.fold_expr(*index);
                if let Expr::Num(n) = &index
                    && n.trunc() == *n
                    && *n >= 0.0
                {
                    return TargetSegment::Static(array_actor_label(&base, *n as usize));
                }
                TargetSegment::Indexed {
                    base,
                    index: Box::new(index),
                }
            },
        }
    }

    fn fold_stmt(&mut self, stmt: Stmt) -> Stmt {
        match stmt {
            Stmt::LetDecl { .. }
            | Stmt::Assignment { .. }
            | Stmt::Action(..)
            | Stmt::Keyframe { .. }
            | Stmt::RelativeKeyframe { .. }
            | Stmt::Sequence { .. }
            | Stmt::Stagger { .. }
            | Stmt::Always { .. }
            | Stmt::ReactiveBinding { .. }
            | Stmt::Conditional { .. }
            | Stmt::Match { .. }
            | Stmt::ForLoop { .. } => walk_fold_stmt(self, stmt),
            other => other,
        }
    }
}

pub(super) fn substitute_params_in_stmt(stmt: &Stmt, bindings: &HashMap<String, Expr>) -> Stmt {
    ParamSubstituter { bindings }.fold_stmt(stmt.clone())
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

pub(super) fn substitute_params_in_expr(expr: &Expr, bindings: &HashMap<String, Expr>) -> Expr {
    match expr {
        Expr::Ident(name) => {
            bindings.get(name).cloned().unwrap_or_else(|| Expr::Ident(name.clone()))
        },
        Expr::Path(parts) => {
            if let Some(first) = parts.first()
                && let Some(bound) = bindings.get(first)
            {
                if parts.len() == 1 {
                    return bound.clone();
                }
                let rest = &parts[1..];
                return match bound {
                    Expr::Ident(name) => {
                        Expr::Path([name.clone()].into_iter().chain(rest.iter().cloned()).collect())
                    },
                    Expr::Path(inner) => {
                        Expr::Path(inner.iter().chain(rest.iter()).cloned().collect())
                    },
                    other => Expr::Method(Box::new(other.clone()), parts[1].clone(), Vec::new()),
                };
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
