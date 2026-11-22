use super::*;

pub(super) fn rewrite_properties(
    props: &[Property],
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Vec<Property> {
    props
        .iter()
        .map(|prop| Property {
            name: prop.name.clone(),
            value: rewrite_property_value(prop, prefix, root_label, known_labels, bindings),
            value_span: prop.value_span,
            trailing_comment: prop.trailing_comment.clone(),
        })
        .collect()
}

pub(super) fn rewrite_property_value(
    prop: &Property,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Expr {
    // A bare role choice can have the same spelling as an actor label.
    // Component parameters still substitute before role resolution.
    if prop.name == "role"
        && matches!(&prop.value, Expr::Ident(name)
        if !bindings.contains_key(name) && animatix_core::roles::Role::from_name(name).is_some())
    {
        return prop.value.clone();
    }
    rewrite_expr(&prop.value, prefix, root_label, known_labels, bindings)
}

pub(super) fn rewrite_expr(
    expr: &Expr,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Expr {
    if let Some(mut members) = self_member_path(expr) {
        if members.first().is_some_and(|member| root_label == Some(member.as_str())) {
            members.remove(0);
        }
        let mut path = vec![prefix.to_string()];
        path.extend(members);
        return if path.len() == 1 {
            Expr::Ident(prefix.to_string())
        } else {
            Expr::Path(path)
        };
    }

    match expr {
        Expr::Ident(name) => bindings.get(name).cloned().unwrap_or_else(|| {
            Expr::Ident(rewrite_label_ref(name, prefix, root_label, known_labels))
        }),
        Expr::Path(parts) => {
            if let Some(bound) = parts.first().and_then(|part| bindings.get(part)) {
                if parts.len() == 1 {
                    return bound.clone();
                }
                let remaining = &parts[1..];
                return match bound {
                    Expr::Ident(name) => {
                        let mut path = split_rewritten_label(name);
                        path.extend(remaining.iter().cloned());
                        Expr::Path(path)
                    },
                    Expr::Path(path) => {
                        let mut path = path.clone();
                        path.extend(remaining.iter().cloned());
                        Expr::Path(path)
                    },
                    other => other.clone(),
                };
            }

            if let Some((first, rest)) = parts.split_first() {
                let rewritten_first = rewrite_label_ref(first, prefix, root_label, known_labels);

                // If the path starts with `self` followed by the root label, skip the root label
                let rest = if first == "self" {
                    if let Some((second, remaining)) = rest.split_first()
                        && root_label == Some(second.as_str())
                    {
                        let mut result = split_rewritten_label(&rewritten_first);
                        for seg in remaining {
                            result.push(rewrite_label(seg, prefix, root_label, known_labels));
                        }
                        return Expr::Path(result);
                    }
                    rest
                } else {
                    rest
                };

                let mut rewritten = split_rewritten_label(&rewritten_first);
                for seg in rest {
                    rewritten.push(rewrite_label(seg, prefix, root_label, known_labels));
                }
                Expr::Path(rewritten)
            } else {
                Expr::Path(parts.clone())
            }
        },
        Expr::QualifiedPath(parts) => Expr::QualifiedPath(parts.clone()),
        Expr::Field(receiver, field) => Expr::Field(
            Box::new(rewrite_expr(receiver, prefix, root_label, known_labels, bindings)),
            rewrite_label(field, prefix, root_label, known_labels),
        ),
        Expr::Index(target, index) => Expr::Index(
            Box::new(rewrite_expr(target, prefix, root_label, known_labels, bindings)),
            Box::new(rewrite_expr(index, prefix, root_label, known_labels, bindings)),
        ),
        Expr::Tuple(items) => Expr::Tuple(
            items
                .iter()
                .map(|item| rewrite_expr(item, prefix, root_label, known_labels, bindings))
                .collect(),
        ),
        Expr::List(items) => Expr::List(
            items
                .iter()
                .map(|item| rewrite_expr(item, prefix, root_label, known_labels, bindings))
                .collect(),
        ),
        Expr::Binary(lhs, op, rhs) => Expr::Binary(
            Box::new(rewrite_expr(lhs, prefix, root_label, known_labels, bindings)),
            op.clone(),
            Box::new(rewrite_expr(rhs, prefix, root_label, known_labels, bindings)),
        ),
        Expr::Unary(op, value) => Expr::Unary(
            op.clone(),
            Box::new(rewrite_expr(value, prefix, root_label, known_labels, bindings)),
        ),
        Expr::Call(callee, args) => Expr::Call(
            Box::new(rewrite_expr(callee, prefix, root_label, known_labels, bindings)),
            args.iter()
                .map(|arg| crate::ast::CallArgument {
                    name: arg.name.clone(),
                    value: rewrite_expr(&arg.value, prefix, root_label, known_labels, bindings),
                    byte_span: arg.byte_span,
                })
                .collect(),
        ),
        Expr::Closure(params, body) => Expr::Closure(
            params.clone(),
            Box::new(rewrite_expr(body, prefix, root_label, known_labels, bindings)),
        ),
        Expr::LetChain(bindings_list, tail) => Expr::LetChain(
            bindings_list
                .iter()
                .map(|(name, value)| {
                    (name.clone(), rewrite_expr(value, prefix, root_label, known_labels, bindings))
                })
                .collect(),
            Box::new(rewrite_expr(tail, prefix, root_label, known_labels, bindings)),
        ),
        Expr::Conditional(condition, then_expr, else_expr) => Expr::Conditional(
            Box::new(rewrite_expr(condition, prefix, root_label, known_labels, bindings)),
            Box::new(rewrite_expr(then_expr, prefix, root_label, known_labels, bindings)),
            Box::new(rewrite_expr(else_expr, prefix, root_label, known_labels, bindings)),
        ),
        Expr::Construct(name, props) => Expr::Construct(
            name.clone(),
            rewrite_properties(props, prefix, root_label, known_labels, bindings),
        ),
        Expr::Timeline(plan) => {
            Expr::Timeline(rewrite_plan(plan, prefix, root_label, known_labels, bindings))
        },
        Expr::Sequence(plan) => {
            Expr::Sequence(rewrite_plan(plan, prefix, root_label, known_labels, bindings))
        },
        Expr::Stagger { modifiers, plan } => Expr::Stagger {
            modifiers: modifiers
                .iter()
                .map(|modifier| crate::ast::Modifier {
                    name: modifier.name.clone(),
                    value: rewrite_expr(
                        &modifier.value,
                        prefix,
                        root_label,
                        known_labels,
                        bindings,
                    ),
                })
                .collect(),
            plan: rewrite_plan(plan, prefix, root_label, known_labels, bindings),
        },
        Expr::Match(scrutinee, arms) => Expr::Match(
            Box::new(rewrite_expr(scrutinee, prefix, root_label, known_labels, bindings)),
            arms.iter()
                .map(|(pat, arm_expr)| {
                    (
                        pat.clone(),
                        Box::new(rewrite_expr(
                            arm_expr,
                            prefix,
                            root_label,
                            known_labels,
                            bindings,
                        )),
                    )
                })
                .collect(),
        ),
        Expr::Num(value) => Expr::Num(*value),
        Expr::Duration(value) => Expr::Duration(*value),
        Expr::Unit => Expr::Unit,
        Expr::Percent(value) => Expr::Percent(*value),
        Expr::Str(value) => Expr::Str(value.clone()),
        Expr::Color(value) => Expr::Color(*value),
        Expr::Bool(value) => Expr::Bool(*value),
        Expr::Null => Expr::Null,
    }
}

fn self_member_path(expr: &Expr) -> Option<Vec<String>> {
    match expr {
        Expr::Ident(name) if name == "self" => Some(Vec::new()),
        Expr::Path(parts) if parts.first().is_some_and(|part| part == "self") => {
            Some(parts.iter().skip(1).cloned().collect())
        },
        Expr::Field(receiver, field) => {
            let mut members = self_member_path(receiver)?;
            members.push(field.clone());
            Some(members)
        },
        _ => None,
    }
}

fn rewrite_plan(
    plan: &crate::ast::PlanBody,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> crate::ast::PlanBody {
    crate::ast::PlanBody {
        statements: plan
            .statements
            .iter()
            .map(|stmt| {
                crate::module::rewrite::rewrite_stmt(
                    stmt,
                    prefix,
                    root_label,
                    known_labels,
                    bindings,
                )
            })
            .collect(),
        instance_methods: plan.instance_methods.clone(),
    }
}

pub(super) fn rewrite_label(
    label: &str,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
) -> String {
    if root_label == Some(label) {
        prefix.to_string()
    } else if known_labels.contains(label) {
        format!("{}.{}", prefix, label)
    } else {
        label.to_string()
    }
}

pub(super) fn rewrite_label_ref(
    label: &str,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
) -> String {
    if label == "scene" {
        label.to_string()
    } else if label == "self" {
        prefix.to_string()
    } else {
        rewrite_label(label, prefix, root_label, known_labels)
    }
}

pub(super) fn rewrite_label_path(
    parts: &[TargetSegment],
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
) -> Vec<TargetSegment> {
    let Some((first, rest)) = parts.split_first() else {
        return Vec::new();
    };

    // Indexed segments are source-level actor refs (`bar[i]`) and must have
    // their base label namespaced when a component rewrites `bar` -> `deck.bar`.
    let first_str = match first {
        TargetSegment::Static(s) => s.as_str(),
        TargetSegment::Indexed { base, index } => {
            let base_rewritten = rewrite_label_ref(base, prefix, root_label, known_labels);
            let mut result = vec![TargetSegment::Indexed {
                base: base_rewritten,
                index: index.clone(),
            }];
            for seg in rest {
                match seg {
                    TargetSegment::Static(s) => {
                        result.push(TargetSegment::Static(rewrite_path_segment(
                            s, prefix, root_label,
                        )));
                    },
                    TargetSegment::Indexed { base, index } => {
                        result.push(TargetSegment::Indexed {
                            base: rewrite_path_segment(base, prefix, root_label),
                            index: index.clone(),
                        });
                    },
                }
            }
            return result;
        },
    };
    let rewritten_first = rewrite_label_ref(first_str, prefix, root_label, known_labels);

    // Only remove an immediate static root segment. Preserve all descendant
    // segments (including runtime indexes) for the shared rewrite loop below.
    let rest = if first_str == "self" {
        match rest.split_first() {
            Some((TargetSegment::Static(second), remaining))
                if root_label == Some(second.as_str()) =>
            {
                remaining
            },
            _ => rest,
        }
    } else {
        rest
    };

    let mut rewritten: Vec<TargetSegment> = split_rewritten_label(&rewritten_first)
        .into_iter()
        .map(TargetSegment::Static)
        .collect();
    for seg in rest {
        match seg {
            TargetSegment::Static(s) => {
                rewritten.push(TargetSegment::Static(rewrite_path_segment(s, prefix, root_label)))
            },
            TargetSegment::Indexed { base, index } => rewritten.push(TargetSegment::Indexed {
                base: rewrite_path_segment(base, prefix, root_label),
                index: index.clone(),
            }),
        }
    }
    rewritten
}

/// Rewrite a path segment for use inside a label path. Unlike `rewrite_label`,
/// this does not add the prefix to known labels — the prefix is only applied
/// to the first element of the path via `rewrite_label_ref`.
pub(super) fn rewrite_path_segment(seg: &str, prefix: &str, root_label: Option<&str>) -> String {
    if seg == "scene" {
        seg.to_string()
    } else if root_label == Some(seg) {
        prefix.to_string()
    } else {
        seg.to_string()
    }
}

pub(super) fn split_rewritten_label(label: &str) -> Vec<String> {
    label.split('.').map(str::to_string).collect()
}
