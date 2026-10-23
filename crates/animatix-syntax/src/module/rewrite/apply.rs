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
            value: rewrite_expr(&prop.value, prefix, root_label, known_labels, bindings),
            value_span: prop.value_span,
            trailing_comment: prop.trailing_comment.clone(),
        })
        .collect()
}

pub(super) fn rewrite_modifiers(
    modifiers: &[Modifier],
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Vec<Modifier> {
    modifiers
        .iter()
        .map(|modifier| Modifier {
            name: modifier.name.clone(),
            value: rewrite_expr(&modifier.value, prefix, root_label, known_labels, bindings),
        })
        .collect()
}

pub(super) fn rewrite_expr(
    expr: &Expr,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Expr {
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
                    if let Some((second, remaining)) = rest.split_first() {
                        if root_label == Some(second.as_str()) {
                            let mut result = split_rewritten_label(&rewritten_first);
                            for seg in remaining {
                                result.push(rewrite_label(seg, prefix, root_label, known_labels));
                            }
                            return Expr::Path(result);
                        }
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
        Expr::Call(name, args) => Expr::Call(
            name.clone(),
            args.iter()
                .map(|arg| rewrite_expr(arg, prefix, root_label, known_labels, bindings))
                .collect(),
        ),
        Expr::Method(target, name, args) => Expr::Method(
            Box::new(rewrite_expr(target, prefix, root_label, known_labels, bindings)),
            name.clone(),
            args.iter()
                .map(|arg| rewrite_expr(arg, prefix, root_label, known_labels, bindings))
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
        Expr::Percent(value) => Expr::Percent(*value),
        Expr::Str(value) => Expr::Str(value.clone()),
        Expr::Bool(value) => Expr::Bool(*value),
        Expr::Null => Expr::Null,
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

    // Extract static strings from remaining segments for the self-skip check
    let rest_static: Vec<&str> = rest
        .iter()
        .filter_map(|s| match s {
            TargetSegment::Static(s) => Some(s.as_str()),
            TargetSegment::Indexed { .. } => None,
        })
        .collect();

    // If the path starts with `self` followed by the root label, skip the root label
    // since `self` already resolves to the prefixed root actor.
    let rest = if first_str == "self" {
        if let Some((second, remaining)) = rest_static.split_first() {
            if root_label == Some(second) {
                let mut result: Vec<TargetSegment> = split_rewritten_label(&rewritten_first)
                    .into_iter()
                    .map(TargetSegment::Static)
                    .collect();
                for seg in remaining {
                    result
                        .push(TargetSegment::Static(rewrite_path_segment(seg, prefix, root_label)));
                }
                return result;
            }
        }
        rest
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
