//! Drive block helpers: rewrites empty-target assignments inside `drive`
//! blocks by prepending the drive label.

use super::*;

impl Timeline {
    /// Recursively rewrite assignments inside a `drive` block by prepending
    /// the drive label to single-segment (empty-target) assignments.
    pub(super) fn rewrite_drive_assignments(&self, stmts: &[Stmt], label: &str) -> Vec<Stmt> {
        stmts
            .iter()
            .map(|stmt| match stmt {
                Stmt::Assignment {
                    target,
                    property,
                    value,
                    modifiers,
                    easing,
                    value_span,
                    span,
                } if target.is_empty() => Stmt::Assignment {
                    target: vec![label.to_string()],
                    property: property.clone(),
                    value: value.clone(),
                    modifiers: modifiers.clone(),
                    easing: *easing,
                    value_span: *value_span,
                    span: *span,
                },
                Stmt::Assignment { .. } => stmt.clone(),
                Stmt::Conditional {
                    condition,
                    then_branch,
                    else_branch,
                    span,
                } => Stmt::Conditional {
                    condition: condition.clone(),
                    then_branch: self.rewrite_drive_assignments(then_branch, label),
                    else_branch: else_branch
                        .as_ref()
                        .map(|b| self.rewrite_drive_assignments(b, label)),
                    span: *span,
                },
                Stmt::ForLoop {
                    var,
                    iterable,
                    body,
                    span,
                } => Stmt::ForLoop {
                    var: var.clone(),
                    iterable: iterable.clone(),
                    body: self.rewrite_drive_assignments(body, label),
                    span: *span,
                },
                Stmt::Always { body, span } => Stmt::Always {
                    body: self.rewrite_drive_assignments(body, label),
                    span: *span,
                },
                Stmt::Drive {
                    label: inner_label,
                    body,
                    span,
                } => Stmt::Drive {
                    label: inner_label.clone(),
                    body: self.rewrite_drive_assignments(body, label),
                    span: *span,
                },
                other => other.clone(),
            })
            .collect()
    }
}