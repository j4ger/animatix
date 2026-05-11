use super::{Action, ComponentDef, Expr, HashMap, HashSet, InlineItem, Modifier, Property, Stmt};

pub(super) fn rewrite_stmt(
    stmt: &Stmt,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Stmt {
    match stmt {
        Stmt::Text {
            label,
            props,
            modifiers,
            ..
        } => Stmt::Text {
            label: label
                .as_ref()
                .map(|label| rewrite_label(label, prefix, root_label, known_labels)),
            props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
            span: None,
        },
        Stmt::Math {
            label,
            props,
            modifiers,
            ..
        } => Stmt::Math {
            label: label
                .as_ref()
                .map(|label| rewrite_label(label, prefix, root_label, known_labels)),
            props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
            span: None,
        },
        Stmt::Code {
            label,
            props,
            modifiers,
            ..
        } => Stmt::Code {
            label: label
                .as_ref()
                .map(|label| rewrite_label(label, prefix, root_label, known_labels)),
            props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
            span: None,
        },
        Stmt::Svg {
            label,
            url,
            at,
            anchor,
            offset,
            scale,
            ..
        } => Stmt::Svg {
            label: label
                .as_ref()
                .map(|label| rewrite_label(label, prefix, root_label, known_labels)),
            url: url.clone(),
            at: at
                .as_ref()
                .map(|expr| rewrite_expr(expr, prefix, root_label, known_labels, bindings)),
            anchor: anchor
                .as_ref()
                .map(|expr| rewrite_expr(expr, prefix, root_label, known_labels, bindings)),
            offset: offset
                .as_ref()
                .map(|expr| rewrite_expr(expr, prefix, root_label, known_labels, bindings)),
            scale: *scale,
            span: None,
        },
        Stmt::Image {
            label,
            url,
            at,
            anchor,
            offset,
            size,
            ..
        } => Stmt::Image {
            label: label
                .as_ref()
                .map(|label| rewrite_label(label, prefix, root_label, known_labels)),
            url: url.clone(),
            at: at
                .as_ref()
                .map(|expr| rewrite_expr(expr, prefix, root_label, known_labels, bindings)),
            anchor: anchor
                .as_ref()
                .map(|expr| rewrite_expr(expr, prefix, root_label, known_labels, bindings)),
            offset: offset
                .as_ref()
                .map(|expr| rewrite_expr(expr, prefix, root_label, known_labels, bindings)),
            size: *size,
            span: None,
        },
        Stmt::ActorDecl {
            is_pub,
            label,
            ty,
            props,
            modifiers,
            children,
            ..
        } => Stmt::ActorDecl {
            is_pub: *is_pub,
            label: rewrite_label(label, prefix, root_label, known_labels),
            ty: ty.clone(),
            props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
            children: rewrite_inline_items(children, prefix, root_label, known_labels, bindings),
            span: None,
        },
        Stmt::Assignment {
            target,
            property,
            value,
            modifiers,
            value_span,
            ..
        } => Stmt::Assignment {
            target: rewrite_label_path(target, prefix, root_label, known_labels),
            property: property.clone(),
            value: rewrite_expr(value, prefix, root_label, known_labels, bindings),
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
            value_span: *value_span,
            span: None,
        },
        Stmt::Sequence { body, .. } => Stmt::Sequence {
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
            span: None,
        },
        Stmt::Stagger { modifiers, body, .. } => Stmt::Stagger {
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
            span: None,
        },
        Stmt::Action(action, ..) => Stmt::Action(Action {
            verb: action.verb.clone(),
            targets: action
                .targets
                .iter()
                .map(|target| rewrite_label_ref(target, prefix, root_label, known_labels))
                .collect(),
            args: action
                .args
                .iter()
                .map(|arg| rewrite_expr(arg, prefix, root_label, known_labels, bindings))
                .collect(),
            modifiers: rewrite_modifiers(
                &action.modifiers,
                prefix,
                root_label,
                known_labels,
                bindings,
            ),
            byte_span: action.byte_span,
        }, None),
        Stmt::LetDecl { is_pub, name, value, .. } => Stmt::LetDecl {
            is_pub: *is_pub,
            name: name.clone(),
            value: rewrite_expr(value, prefix, root_label, known_labels, bindings),
            span: None,
        },
        Stmt::Keyframe { time, body, .. } => Stmt::Keyframe {
            time: time.clone(),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
            span: None,
        },
        Stmt::RelativeKeyframe { offset, body, .. } => Stmt::RelativeKeyframe {
            offset: offset.clone(),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
            span: None,
        },
        Stmt::Always { body, .. } => Stmt::Always {
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
            span: None,
        },
        Stmt::LabeledAlways { label, body, .. } => Stmt::LabeledAlways {
            label: rewrite_label(label, prefix, root_label, known_labels),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
            span: None,
        },
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            ..
        } => Stmt::Conditional {
            condition: rewrite_expr(condition, prefix, root_label, known_labels, bindings),
            then_branch: then_branch
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
            else_branch: else_branch.as_ref().map(|branch| {
                branch
                    .iter()
                    .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                    .collect()
            }),
            span: None,
        },
        Stmt::ForLoop {
            var,
            iterable,
            body,
            ..
        } => Stmt::ForLoop {
            var: var.clone(),
            iterable: rewrite_expr(iterable, prefix, root_label, known_labels, bindings),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
            span: None,
        },
        Stmt::ComponentDef(definition, ..) => Stmt::ComponentDef(ComponentDef {
            is_pub: definition.is_pub,
            name: definition.name.clone(),
            params: definition.params.clone(),
            body: definition
                .body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
        }, None),
        Stmt::ComponentAction { name, params, body, .. } => Stmt::ComponentAction {
            name: name.clone(),
            params: params.clone(),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
            span: None,
        },
        Stmt::Config { settings, .. } => Stmt::Config {
            settings: rewrite_properties(settings, prefix, root_label, known_labels, bindings),
            span: None,
        },
        Stmt::Import { path, alias, .. } => Stmt::Import { path: path.clone(), alias: alias.clone(), span: None },
        Stmt::Use { path, items, .. } => Stmt::Use {
            path: path.clone(),
            items: items.clone(),
            span: None,
        },
        Stmt::Comment(comment, ..) => Stmt::Comment(comment.clone(), None),
    }
}

pub(super) fn rewrite_inline_items(
    items: &[InlineItem],
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Vec<InlineItem> {
    items
        .iter()
        .map(|item| match item {
            InlineItem::Anonymous {
                ty,
                props,
                modifiers,
                children,
            } => InlineItem::Anonymous {
                ty: ty.clone(),
                props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
                modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
                children: rewrite_inline_items(
                    children,
                    prefix,
                    root_label,
                    known_labels,
                    bindings,
                ),
            },
            InlineItem::Labeled {
                label,
                ty,
                props,
                modifiers,
                children,
            } => InlineItem::Labeled {
                label: rewrite_label(label, prefix, root_label, known_labels),
                ty: ty.clone(),
                props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
                modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
                children: rewrite_inline_items(
                    children,
                    prefix,
                    root_label,
                    known_labels,
                    bindings,
                ),
            },
            InlineItem::SlotMarker => InlineItem::SlotMarker,
            InlineItem::SlotFill { slot_name, items } => InlineItem::SlotFill {
                slot_name: slot_name.clone(),
                items: rewrite_inline_items(items, prefix, root_label, known_labels, bindings),
            },
        })
        .collect()
}

fn rewrite_properties(
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

fn rewrite_modifiers(
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

fn rewrite_expr(
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
                    }
                    Expr::Path(path) => {
                        let mut path = path.clone();
                        path.extend(remaining.iter().cloned());
                        Expr::Path(path)
                    }
                    other => other.clone(),
                };
            }

            if let Some((first, rest)) = parts.split_first() {
                let mut rewritten = split_rewritten_label(&rewrite_label_ref(
                    first,
                    prefix,
                    root_label,
                    known_labels,
                ));
                rewritten.extend(rest.iter().cloned());
                Expr::Path(rewritten)
            } else {
                Expr::Path(parts.clone())
            }
        }
        Expr::Index(target, index) => Expr::Index(
            Box::new(rewrite_expr(
                target,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
            Box::new(rewrite_expr(
                index,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
        ),
        Expr::Tuple(items) => Expr::Tuple(
            items
                .iter()
                .map(|item| rewrite_expr(item, prefix, root_label, known_labels, bindings))
                .collect(),
        ),
        Expr::Binary(lhs, op, rhs) => Expr::Binary(
            Box::new(rewrite_expr(
                lhs,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
            op.clone(),
            Box::new(rewrite_expr(
                rhs,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
        ),
        Expr::Unary(op, value) => Expr::Unary(
            op.clone(),
            Box::new(rewrite_expr(
                value,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
        ),
        Expr::Call(name, args) => Expr::Call(
            name.clone(),
            args.iter()
                .map(|arg| rewrite_expr(arg, prefix, root_label, known_labels, bindings))
                .collect(),
        ),
        Expr::Method(target, name, args) => Expr::Method(
            Box::new(rewrite_expr(
                target,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
            name.clone(),
            args.iter()
                .map(|arg| rewrite_expr(arg, prefix, root_label, known_labels, bindings))
                .collect(),
        ),
        Expr::Closure(params, body) => Expr::Closure(
            params.clone(),
            Box::new(rewrite_expr(
                body,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
        ),
        Expr::Conditional(condition, then_expr, else_expr) => Expr::Conditional(
            Box::new(rewrite_expr(
                condition,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
            Box::new(rewrite_expr(
                then_expr,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
            Box::new(rewrite_expr(
                else_expr,
                prefix,
                root_label,
                known_labels,
                bindings,
            )),
        ),
        Expr::Construct(name, props) => Expr::Construct(
            name.clone(),
            rewrite_properties(props, prefix, root_label, known_labels, bindings),
        ),
        Expr::Num(value) => Expr::Num(*value),
        Expr::Percent(value) => Expr::Percent(*value),
        Expr::Str(value) => Expr::Str(value.clone()),
        Expr::Bool(value) => Expr::Bool(*value),
        Expr::Null => Expr::Null,
    }
}

fn rewrite_label(
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

fn rewrite_label_ref(
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

fn rewrite_label_path(
    parts: &[String],
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
) -> Vec<String> {
    let Some((first, rest)) = parts.split_first() else {
        return Vec::new();
    };

    let mut rewritten =
        split_rewritten_label(&rewrite_label_ref(first, prefix, root_label, known_labels));
    rewritten.extend(rest.iter().cloned());
    rewritten
}

fn split_rewritten_label(label: &str) -> Vec<String> {
    label.split('.').map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_media_statement_position_expressions() {
        let stmt = Stmt::Svg {
            label: Some("logo".to_string()),
            url: "examples/vector.svg".to_string(),
            at: Some(Expr::Ident("badge".to_string())),
            anchor: Some(Expr::Path(vec!["scene".to_string(), "top".to_string()])),
            offset: Some(Expr::Tuple(vec![
                Expr::Num(0.0),
                Expr::Ident("delta".to_string()),
            ])),
            scale: 1.0,
            span: None,
        };
        let known_labels = HashSet::from(["logo".to_string(), "badge".to_string()]);
        let bindings = HashMap::from([("delta".to_string(), Expr::Num(48.0))]);

        let rewritten = rewrite_stmt(&stmt, "hero", None, &known_labels, &bindings);

        match rewritten {
            Stmt::Svg {
                label,
                at,
                anchor,
                offset,
                ..
            } => {
                assert_eq!(label.as_deref(), Some("hero.logo"));
                assert_eq!(at, Some(Expr::Ident("hero.badge".to_string())));
                assert_eq!(
                    anchor,
                    Some(Expr::Path(vec!["scene".to_string(), "top".to_string()]))
                );
                assert_eq!(
                    offset,
                    Some(Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(48.0)]))
                );
            }
            other => panic!("expected rewritten svg statement, got {other:?}"),
        }
    }
}
