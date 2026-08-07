use std::collections::HashMap;

use crate::ast::{Action, Expr, Modifier, Span, Stmt, TargetSegment, array_actor_label};
use crate::module::{ActionTemplate, InstanceActionRegistry};

// NOTE: This function takes ownership of Vec<Stmt> and produces a new
// Vec<Stmt> (owned tree transformation with potential 1→N expansion).
// The shared walk primitives work on references, not owned data, and
// use a FnMut(&T) -> () visitor pattern that cannot propagate the
// transformed output, making them incompatible.

/// Replace custom component action invocations with their inlined bodies.
///
/// When `pulse btn [200ms]` is encountered and `btn` has a custom action `pulse`,
/// the invocation is replaced with the action's body statements. Multi-target
/// invocations like `pulse btn1, btn2` expand once per matching target.
/// Invocation modifiers override body modifiers (e.g. `[200ms]` replaces any duration
/// in the body).
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
            let mut inlined = Vec::new();
            let mut remaining = Vec::new();

            for target in &action.targets {
                // Component instance actions are more specific than module actions.
                if let Some(template) = registry.get(target).and_then(|m| m.get(&action.verb)) {
                    inlined.extend(inline_target_body(template, target, &action, span, false));
                } else if let Some(template) = module_actions.get(&action.verb) {
                    inlined.extend(inline_target_body(template, target, &action, span, true));
                } else {
                    remaining.push(target.clone());
                }
            }

            if !remaining.is_empty() {
                inlined.push(Stmt::Action(
                    Action {
                        verb: action.verb,
                        targets: remaining,
                        args: action.args,
                        modifiers: action.modifiers,
                        byte_span: action.byte_span,
                    },
                    span,
                ));
            }

            inlined
        },
        // For all other statements, recurse into bodies using shared walk
        mut stmt => {
            let bodies = crate::walk::collect_stmt_bodies_mut(&mut stmt);
            for body in bodies {
                *body = inline_custom_actions(std::mem::take(body), registry, module_actions);
            }
            vec![stmt]
        },
    }
}

/// Inline one custom action template for a concrete invocation target.
///
/// Component templates are already prefix-rewritten during expansion, so only
/// module-scoped templates need a `self` rewrite here.
fn inline_target_body(
    template: &ActionTemplate,
    target: &str,
    action: &Action,
    span: Option<Span>,
    rewrite_self: bool,
) -> Vec<Stmt> {
    let (body, unconsumed) = substitute_action_params(template, &action.modifiers);
    let body = if rewrite_self {
        rewrite_self_targets(body, target)
    } else {
        body
    };
    apply_modifiers_to_body(body, &unconsumed, span)
}

/// Rewrite `self` references in a custom action body to the concrete target.
///
/// Component templates are already prefix-rewritten during expansion; this is
/// still safe because it only changes `self` when present.
fn rewrite_self_targets(body: Vec<Stmt>, target: &str) -> Vec<Stmt> {
    let known_labels = std::collections::HashSet::new();
    let bindings = HashMap::new();
    body.into_iter()
        .map(|stmt| super::rewrite::rewrite_stmt(&stmt, target, None, &known_labels, &bindings))
        .collect()
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
        },
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
            span,
        } => Stmt::ForLoop {
            var,
            index_var,
            iterable: substitute_params_in_expr(&iterable, bindings),
            body: body.iter().map(|s| substitute_params_in_stmt(s, bindings)).collect(),
            span,
        },
        other => other,
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
fn apply_modifiers_to_body(
    body: Vec<Stmt>,
    invocation_modifiers: &[Modifier],
    span: Option<Span>,
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
                span,
            },
            Stmt::Action(mut action, _) => {
                action.modifiers = invocation_modifiers.to_vec();
                Stmt::Action(action, span)
            },
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

    fn make_multi_action(verb: &str, targets: &[&str], modifiers: Vec<Modifier>) -> Stmt {
        Stmt::Action(
            Action {
                verb: verb.to_string(),
                targets: targets.iter().map(|t| t.to_string()).collect(),
                args: vec![],
                modifiers,
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

    #[test]
    fn action_params_substitute_into_body() {
        let template = ActionTemplate {
            params: vec![ParamDef {
                name: "scale".to_string(),
                param_type: Some(TypeAnnotation::Num),
                default: Some(Expr::Num(1.2)),
            }],
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
            },
            other => panic!("expected assignment, got {:?}", other),
        }
    }

    #[test]
    fn action_params_field_access_substitutes_into_body() {
        let template = ActionTemplate {
            params: vec![ParamDef {
                name: "point".to_string(),
                param_type: Some(TypeAnnotation::Num),
                default: None,
            }],
            body: vec![make_assignment(
                "self",
                "scale",
                Expr::Path(vec!["point".to_string(), "x".to_string()]),
            )],
        };

        let registry: InstanceActionRegistry =
            [("btn".to_string(), [("pulse".to_string(), template)].into_iter().collect())]
                .into_iter()
                .collect();

        let invocation = make_action(
            "pulse",
            "btn",
            vec![Modifier {
                name: Some("point".to_string()),
                value: Expr::Path(vec!["settings".to_string(), "point".to_string()]),
            }],
        );

        let module_actions: HashMap<String, ActionTemplate> = HashMap::new();
        let result = inline_stmt(invocation, &registry, &module_actions);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Stmt::Assignment { value, .. } => {
                assert_eq!(
                    *value,
                    Expr::Path(vec!["settings".to_string(), "point".to_string(), "x".to_string(),])
                );
            },
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

        let invocation = make_action("pulse", "btn", vec![]);

        let module_actions: HashMap<String, ActionTemplate> = HashMap::new();
        let result = inline_stmt(invocation, &registry, &module_actions);
        assert_eq!(result.len(), 1);
        match &result[0] {
            Stmt::Assignment { value, .. } => {
                assert_eq!(*value, Expr::Num(1.2));
            },
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
            Stmt::Assignment {
                value, modifiers, ..
            } => {
                assert_eq!(*value, Expr::Num(1.5));
                // `duration` was consumed as param, `scale` was consumed as param,
                // so no modifiers should remain to be applied to body
                assert!(modifiers.is_empty(), "expected no modifiers, got {:?}", modifiers);
            },
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
            Stmt::Assignment {
                value, modifiers, ..
            } => {
                assert_eq!(*value, Expr::Num(1.5));
                assert_eq!(modifiers.len(), 1);
                assert_eq!(modifiers[0].name, Some("ease".to_string()));
            },
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
            Stmt::Assignment {
                target,
                property,
                value,
                ..
            } => {
                assert_eq!(target, &[crate::ast::TargetSegment::Static("btn".to_string())]);
                assert_eq!(property, "opacity");
                assert_eq!(*value, Expr::Num(0.5));
            },
            other => panic!("expected assignment, got {:?}", other),
        }
    }

    #[test]
    fn multi_target_component_action_inlines_each_target() {
        let template_btn1 = ActionTemplate {
            params: vec![],
            body: vec![make_assignment("btn1", "scale", Expr::Num(1.2))],
        };
        let template_btn2 = ActionTemplate {
            params: vec![],
            body: vec![make_assignment("btn2", "scale", Expr::Num(1.2))],
        };
        let registry: InstanceActionRegistry = [
            ("btn1".to_string(), [("pulse".to_string(), template_btn1)].into_iter().collect()),
            ("btn2".to_string(), [("pulse".to_string(), template_btn2)].into_iter().collect()),
        ]
        .into_iter()
        .collect();

        let invocation = make_multi_action("pulse", &["btn1", "btn2"], vec![]);
        let module_actions: HashMap<String, ActionTemplate> = HashMap::new();
        let result = inline_stmt(invocation, &registry, &module_actions);

        assert_eq!(result.len(), 2);
        for (i, target) in ["btn1", "btn2"].into_iter().enumerate() {
            match &result[i] {
                Stmt::Assignment {
                    target: assignment_target,
                    ..
                } => {
                    assert_eq!(
                        assignment_target,
                        &[crate::ast::TargetSegment::Static(target.to_string())]
                    );
                },
                other => panic!("expected assignment, got {:?}", other),
            }
        }
    }

    #[test]
    fn multi_target_action_keeps_unmatched_targets_for_builtin_dispatch() {
        let template = ActionTemplate {
            params: vec![],
            body: vec![make_assignment("btn1", "scale", Expr::Num(1.2))],
        };
        let registry: InstanceActionRegistry =
            [("btn1".to_string(), [("pulse".to_string(), template)].into_iter().collect())]
                .into_iter()
                .collect();
        let modifiers = vec![Modifier {
            name: None,
            value: Expr::Ident("200ms".to_string()),
        }];
        let invocation = make_multi_action("pulse", &["btn1", "rect"], modifiers);
        let module_actions: HashMap<String, ActionTemplate> = HashMap::new();
        let result = inline_stmt(invocation, &registry, &module_actions);

        assert_eq!(result.len(), 2);
        match &result[1] {
            Stmt::Action(action, _) => {
                assert_eq!(action.verb, "pulse");
                assert_eq!(action.targets, vec!["rect".to_string()]);
                assert_eq!(action.modifiers.len(), 1);
            },
            other => panic!("expected remaining action, got {:?}", other),
        }
    }

    #[test]
    fn multi_target_module_action_rewrites_self_for_each_target() {
        let template = ActionTemplate {
            params: vec![],
            body: vec![make_assignment("self", "opacity", Expr::Num(0.5))],
        };
        let registry: InstanceActionRegistry = HashMap::new();
        let module_actions: HashMap<String, ActionTemplate> =
            [("fade".to_string(), template)].into_iter().collect();
        let invocation = make_multi_action("fade", &["a", "b"], vec![]);

        let result = inline_stmt(invocation, &registry, &module_actions);
        assert_eq!(result.len(), 2);
        for (i, target) in ["a", "b"].into_iter().enumerate() {
            match &result[i] {
                Stmt::Assignment {
                    target: assignment_target,
                    ..
                } => {
                    assert_eq!(
                        assignment_target,
                        &[crate::ast::TargetSegment::Static(target.to_string())]
                    );
                },
                other => panic!("expected assignment, got {:?}", other),
            }
        }
    }
}
