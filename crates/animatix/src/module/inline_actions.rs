use crate::ast::{Modifier, Span, Stmt};
use crate::module::InstanceActionRegistry;

/// Replace custom component action invocations with their inlined bodies.
///
/// When `pulse btn [200ms]` is encountered and `btn` has a custom action `pulse`,
/// the invocation is replaced with the action's body statements. Invocation modifiers
/// override body modifiers (e.g. `[200ms]` replaces any duration in the body).
pub(super) fn inline_custom_actions(
    stmts: Vec<Stmt>,
    registry: &InstanceActionRegistry,
) -> Vec<Stmt> {
    stmts
        .into_iter()
        .flat_map(|stmt| inline_stmt(stmt, registry))
        .collect()
}

fn inline_stmt(stmt: Stmt, registry: &InstanceActionRegistry) -> Vec<Stmt> {
    match stmt {
        Stmt::Action(action, span) => {
            if let Some(target) = action.targets.first() {
                if let Some(body) = registry.get(target).and_then(|m| m.get(&action.verb)) {
                    return apply_modifiers_to_body(body.clone(), &action.modifiers, span);
                }
            }
            vec![Stmt::Action(action, span)]
        }
        Stmt::Keyframe { time, body, span } => vec![Stmt::Keyframe {
            time,
            body: inline_custom_actions(body, registry),
            span,
        }],
        Stmt::RelativeKeyframe { offset, body, span } => vec![Stmt::RelativeKeyframe {
            offset,
            body: inline_custom_actions(body, registry),
            span,
        }],
        Stmt::Sequence { body, span } => vec![Stmt::Sequence {
            body: inline_custom_actions(body, registry),
            span,
        }],
        Stmt::Stagger { modifiers, body, span } => vec![Stmt::Stagger {
            modifiers,
            body: inline_custom_actions(body, registry),
            span,
        }],
        Stmt::Always { body, span } => vec![Stmt::Always {
            body: inline_custom_actions(body, registry),
            span,
        }],
        Stmt::Drive { label, body, span } => vec![Stmt::Drive {
            label,
            body: inline_custom_actions(body, registry),
            span,
        }],
        Stmt::ReactiveBinding { target, property, value, value_span, span } => vec![Stmt::ReactiveBinding {
            target,
            property,
            value,
            value_span,
            span,
        }],
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            span,
        } => vec![Stmt::Conditional {
            condition,
            then_branch: inline_custom_actions(then_branch, registry),
            else_branch: else_branch.map(|b| inline_custom_actions(b, registry)),
            span,
        }],
        Stmt::ForLoop {
            var,
            iterable,
            body,
            span,
        } => vec![Stmt::ForLoop {
            var,
            iterable,
            body: inline_custom_actions(body, registry),
            span,
        }],
        other => vec![other],
    }
}

/// Apply invocation modifiers to each body statement.
///
/// MVP rule: invocation modifiers replace body modifiers entirely.
/// `pulse btn [200ms]` turns `self.scale = 1.2 [100ms]` into `self.scale = 1.2 [200ms]`.
fn apply_modifiers_to_body(
    body: Vec<Stmt>,
    invocation_modifiers: &[Modifier],
    span: Option<Span>,
) -> Vec<Stmt> {
    if invocation_modifiers.is_empty() {
        return body;
    }

    body
        .into_iter()
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
                
                value_span,
                span,
            },
            Stmt::Action(mut action, _) => {
                action.modifiers = invocation_modifiers.to_vec();
                Stmt::Action(action, span)
            }
            other => other,
        })
        .collect()
}
