use crate::ast::{Expr, Modifier, Span, Stmt};
use crate::module::{ActionTemplate, InstanceActionRegistry};
use std::collections::HashMap;

/// Replace custom component action invocations with their inlined bodies.
///
/// When `pulse btn [200ms]` is encountered and `btn` has a custom action `pulse`,
/// the invocation is replaced with the action's body statements. Invocation modifiers
/// override body modifiers (e.g. `[200ms]` replaces any duration in the body).
///
/// Named modifiers that match action parameters are substituted into the body
/// before modifier override is applied: `pulse btn [200ms, scale: 1.5]` binds
/// `scale` to `1.5` in the action body.
///
/// Module-scoped actions are checked as a fallback when no component action matches.
pub(super) fn inline_custom_actions(
    stmts: Vec<Stmt>,
    registry: &InstanceActionRegistry,
    module_actions: &HashMap<String, ActionTemplate>,
) -> Vec<Stmt> {
    stmts
        .into_iter()
        .flat_map(|stmt| inline_stmt(stmt, registry, module_actions))
        .collect()
}

fn inline_stmt(
    stmt: Stmt,
    registry: &InstanceActionRegistry,
    module_actions: &HashMap<String, ActionTemplate>,
) -> Vec<Stmt> {
    match stmt {
        Stmt::Action(action, span) => {
            // Try component instance actions first (more specific)
            if let Some(target) = action.targets.first() {
                if let Some(template) = registry.get(target).and_then(|m| m.get(&action.verb)) {
                    let (body, unconsumed) = substitute_action_params(template, &action.modifiers);
                    return apply_modifiers_to_body(body, &unconsumed, span);
                }
            }
            // Fall back to module-scoped actions
            if let Some(template) = module_actions.get(&action.verb) {
                let (body, unconsumed) = substitute_action_params(template, &action.modifiers);
                return apply_modifiers_to_body(body, &unconsumed, span);
            }
            vec![Stmt::Action(action, span)]
        }
        Stmt::Keyframe { time, body, span } => vec![Stmt::Keyframe {
            time,
            body: inline_custom_actions(body, registry, module_actions),
            span,
        }],
        Stmt::RelativeKeyframe { offset, body, span } => vec![Stmt::RelativeKeyframe {
            offset,
            body: inline_custom_actions(body, registry, module_actions),
            span,
        }],
        Stmt::Sequence { body, span } => vec![Stmt::Sequence {
            body: inline_custom_actions(body, registry, module_actions),
            span,
        }],
        Stmt::Stagger { modifiers, body, span } => vec![Stmt::Stagger {
            modifiers,
            body: inline_custom_actions(body, registry, module_actions),
            span,
        }],
        Stmt::Always { body, span } => vec![Stmt::Always {
            body: inline_custom_actions(body, registry, module_actions),
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
            then_branch: inline_custom_actions(then_branch, registry, module_actions),
            else_branch: else_branch.map(|b| inline_custom_actions(b, registry, module_actions)),
            span,
        }],
        Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body,
            span,
        } => vec![Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body: inline_custom_actions(body, registry, module_actions),
            span,
        }],
        other => vec![other],
    }
}

/// Substitute action parameter values from invocation modifiers into the body.
///
/// Named modifiers like `scale: 1.5` are matched against action parameter names.
/// Positional time modifiers (e.g. `200ms`) are bound to `duration` param if present.
///
/// Returns the substituted body plus any modifiers that were NOT consumed as params.
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
        }
        _ => false,
    }
}

fn substitute_params_in_stmt(stmt: &Stmt, bindings: &HashMap<String, Expr>) -> Stmt {
    match stmt.clone() {
        Stmt::Assignment {
            target,
            property,
            value,
            modifiers,
            easing,
            value_span,
            span,
        } => Stmt::Assignment {
            target,
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
        }
        Stmt::Keyframe { time, body, span } => Stmt::Keyframe {
            time,
            body: body
                .iter()
                .map(|s| substitute_params_in_stmt(s, bindings))
                .collect(),
            span,
        },
        Stmt::RelativeKeyframe { offset, body, span } => Stmt::RelativeKeyframe {
            offset,
            body: body
                .iter()
                .map(|s| substitute_params_in_stmt(s, bindings))
                .collect(),
            span,
        },
        Stmt::Sequence { body, span } => Stmt::Sequence {
            body: body
                .iter()
                .map(|s| substitute_params_in_stmt(s, bindings))
                .collect(),
            span,
        },
        Stmt::Stagger { modifiers, body, span } => Stmt::Stagger {
            modifiers: modifiers
                .into_iter()
                .map(|m| substitute_params_in_modifier(&m, bindings))
                .collect(),
            body: body
                .iter()
                .map(|s| substitute_params_in_stmt(s, bindings))
                .collect(),
            span,
        },
        Stmt::Always { body, span } => Stmt::Always {
            body: body
                .iter()
                .map(|s| substitute_params_in_stmt(s, bindings))
                .collect(),
            span,
        },
        Stmt::ReactiveBinding {
            target,
            property,
            value,
            value_span,
            span,
        } => Stmt::ReactiveBinding {
            target,
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
        Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body,
            span,
        } => Stmt::ForLoop {
            var,
            index_var,
            iterable: substitute_params_in_expr(&iterable, bindings),
            body: body
                .iter()
                .map(|s| substitute_params_in_stmt(s, bindings))
                .collect(),
            span,
        },
        other => other,
    }
}

fn substitute_params_in_expr(expr: &Expr, bindings: &HashMap<String, Expr>) -> Expr {
    match expr {
        Expr::Ident(name) => bindings.get(name).cloned().unwrap_or_else(|| Expr::Ident(name.clone())),
        Expr::Path(parts) => {
            if let Some(first) = parts.first() {
                if let Some(bound) = bindings.get(first) {
                    if parts.len() == 1 {
                        return bound.clone();
                    }
                    // Complex case: param.field access — not supported for now
                }
            }
            Expr::Path(parts.clone())
        }
        Expr::Tuple(items) => Expr::Tuple(
            items.iter().map(|item| substitute_params_in_expr(item, bindings)).collect(),
        ),
        Expr::Binary(lhs, op, rhs) => Expr::Binary(
            Box::new(substitute_params_in_expr(lhs, bindings)),
            op.clone(),
            Box::new(substitute_params_in_expr(rhs, bindings)),
        ),
        Expr::Unary(op, value) => Expr::Unary(
            op.clone(),
            Box::new(substitute_params_in_expr(value, bindings)),
        ),
        Expr::Call(name, args) => Expr::Call(
            name.clone(),
            args.iter().map(|arg| substitute_params_in_expr(arg, bindings)).collect(),
        ),
        Expr::Method(target, name, args) => Expr::Method(
            Box::new(substitute_params_in_expr(target, bindings)),
            name.clone(),
            args.iter().map(|arg| substitute_params_in_expr(arg, bindings)).collect(),
        ),
        Expr::Closure(params, body) => Expr::Closure(
            params.clone(),
            Box::new(substitute_params_in_expr(body, bindings)),
        ),
        Expr::Conditional(cond, then_expr, else_expr) => Expr::Conditional(
            Box::new(substitute_params_in_expr(cond, bindings)),
            Box::new(substitute_params_in_expr(then_expr, bindings)),
            Box::new(substitute_params_in_expr(else_expr, bindings)),
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

fn substitute_params_in_modifier(modifier: &Modifier, bindings: &HashMap<String, Expr>) -> Modifier {
    Modifier {
        name: modifier.name.clone(),
        value: substitute_params_in_expr(&modifier.value, bindings),
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
                easing: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Action, Expr, Modifier, ParamDef, Stmt, TypeAnnotation};

    fn make_action(verb: &str, target: &str, modifiers: Vec<Modifier>) -> Stmt {
        Stmt::Action(
            Action {
                verb: verb.to_string(),
                targets: vec![target.to_string()],
                args: vec![],
                modifiers,
                byte_span: None,
            },
            None,
        )
    }

    fn make_assignment(target: &str, property: &str, value: Expr) -> Stmt {
        Stmt::Assignment {
            target: vec![target.to_string()],
            property: property.to_string(),
            value,
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }
    }

    #[test]
    fn action_params_substitute_into_body() {
        let template = ActionTemplate {
            params: vec![ParamDef {
                name: "scale".to_string(),
                param_type: Some(TypeAnnotation::Num),
                default: Some(Expr::Num(1.2)),
            }],
            body: vec![make_assignment("self", "scale", Expr::Ident("scale".to_string()))],
        };

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

        let module_actions: HashMap<String, ActionTemplate> = HashMap::new();
        let result = inline_stmt(invocation, &registry, &module_actions);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Stmt::Assignment { value, .. } => {
                assert_eq!(*value, Expr::Num(1.5));
            }
            other => panic!("expected assignment, got {:?}", other),
        }
    }

    #[test]
    fn action_params_use_default_when_not_provided() {
        let template = ActionTemplate {
            params: vec![ParamDef {
                name: "scale".to_string(),
                param_type: Some(TypeAnnotation::Num),
                default: Some(Expr::Num(1.2)),
            }],
            body: vec![make_assignment("self", "scale", Expr::Ident("scale".to_string()))],
        };

        let registry: InstanceActionRegistry =
            [("btn".to_string(), [("pulse".to_string(), template)].into_iter().collect())]
                .into_iter()
                .collect();

        let invocation = make_action("pulse", "btn", vec![]);

        let module_actions: HashMap<String, ActionTemplate> = HashMap::new();
        let result = inline_stmt(invocation, &registry, &module_actions);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Stmt::Assignment { value, .. } => {
                assert_eq!(*value, Expr::Num(1.2));
            }
            other => panic!("expected assignment, got {:?}", other),
        }
    }

    #[test]
    fn action_params_positional_time_bound_to_duration() {
        let template = ActionTemplate {
            params: vec![
                ParamDef {
                    name: "duration".to_string(),
                    param_type: Some(TypeAnnotation::Num),
                    default: Some(Expr::Ident("100ms".to_string())),
                },
                ParamDef {
                    name: "scale".to_string(),
                    param_type: Some(TypeAnnotation::Num),
                    default: Some(Expr::Num(1.15)),
                },
            ],
            body: vec![make_assignment(
                "self",
                "scale",
                Expr::Ident("scale".to_string()),
            )],
        };

        let registry: InstanceActionRegistry =
            [("btn".to_string(), [("pulse".to_string(), template)].into_iter().collect())]
                .into_iter()
                .collect();

        // Positional time `200ms` should auto-bind to `duration` param
        let invocation = make_action(
            "pulse",
            "btn",
            vec![
                Modifier {
                    name: None,
                    value: Expr::Ident("200ms".to_string()),
                },
                Modifier {
                    name: Some("scale".to_string()),
                    value: Expr::Num(1.5),
                },
            ],
        );

        let module_actions: HashMap<String, ActionTemplate> = HashMap::new();
        let result = inline_stmt(invocation, &registry, &module_actions);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Stmt::Assignment { value, modifiers, .. } => {
                assert_eq!(*value, Expr::Num(1.5));
                // `duration` was consumed as param, `scale` was consumed as param,
                // so no modifiers should remain to be applied to body
                assert!(modifiers.is_empty(), "expected no modifiers, got {:?}", modifiers);
            }
            other => panic!("expected assignment, got {:?}", other),
        }
    }

    #[test]
    fn action_params_unconsumed_modifiers_applied_to_body() {
        let template = ActionTemplate {
            params: vec![ParamDef {
                name: "scale".to_string(),
                param_type: Some(TypeAnnotation::Num),
                default: Some(Expr::Num(1.2)),
            }],
            body: vec![make_assignment("self", "scale", Expr::Ident("scale".to_string()))],
        };

        let registry: InstanceActionRegistry =
            [("btn".to_string(), [("pulse".to_string(), template)].into_iter().collect())]
                .into_iter()
                .collect();

        // `ease: bounce` is not a param, so it should be applied to the body
        let invocation = make_action(
            "pulse",
            "btn",
            vec![
                Modifier {
                    name: Some("scale".to_string()),
                    value: Expr::Num(1.5),
                },
                Modifier {
                    name: Some("ease".to_string()),
                    value: Expr::Ident("bounce".to_string()),
                },
            ],
        );

        let module_actions: HashMap<String, ActionTemplate> = HashMap::new();
        let result = inline_stmt(invocation, &registry, &module_actions);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Stmt::Assignment { value, modifiers, .. } => {
                assert_eq!(*value, Expr::Num(1.5));
                assert_eq!(modifiers.len(), 1);
                assert_eq!(modifiers[0].name, Some("ease".to_string()));
            }
            other => panic!("expected assignment, got {:?}", other),
        }
    }

    #[test]
    fn action_params_unknown_invocation_passthrough() {
        let registry: InstanceActionRegistry = HashMap::new();
        let module_actions: HashMap<String, ActionTemplate> = HashMap::new();
        let invocation = make_action("pulse", "btn", vec![]);
        let result = inline_stmt(invocation.clone(), &registry, &module_actions);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], invocation);
    }

    #[test]
    fn module_scoped_action_inlining() {
        let template = ActionTemplate {
            params: vec![],
            body: vec![make_assignment("self", "opacity", Expr::Num(0.5))],
        };

        let registry: InstanceActionRegistry = HashMap::new();
        let module_actions: HashMap<String, ActionTemplate> =
            [("fade".to_string(), template)].into_iter().collect();

        let invocation = make_action("fade", "btn", vec![]);

        let result = inline_stmt(invocation, &registry, &module_actions);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Stmt::Assignment { property, value, .. } => {
                assert_eq!(property, "opacity");
                assert_eq!(*value, Expr::Num(0.5));
            }
            other => panic!("expected assignment, got {:?}", other),
        }
    }
}
