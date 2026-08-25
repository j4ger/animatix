use super::{
    Action, ComponentDef, Expr, HashMap, HashSet, InlineItem, Modifier, Property, Stmt,
    TargetSegment,
};

/// Recursively check if a statement contains any identifiers that need rewriting.
fn stmt_needs_rewrite(
    stmt: &Stmt,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> bool {
    // Per-variant checks that DON'T involve body recursion
    let immediate = match stmt {
        Stmt::ActorDecl {
            label,
            props,
            modifiers,
            children,
            ..
        } => {
            root_label == Some(label.as_str())
                || known_labels.contains(label.as_str())
                || props
                    .iter()
                    .any(|p| expr_needs_rewrite(&p.value, root_label, known_labels, bindings))
                || modifiers
                    .iter()
                    .any(|m| expr_needs_rewrite(&m.value, root_label, known_labels, bindings))
                || children
                    .iter()
                    .any(|item| inline_item_needs_rewrite(item, root_label, known_labels, bindings))
        },
        Stmt::Assignment {
            target,
            value,
            modifiers,
            ..
        } => {
            target.iter().any(|t| match t {
                TargetSegment::Static(s) => {
                    s == "self"
                        || root_label == Some(s.as_str())
                        || known_labels.contains(s.as_str())
                },
                TargetSegment::Indexed { base, index } => {
                    base == "self"
                        || root_label == Some(base.as_str())
                        || known_labels.contains(base.as_str())
                        || expr_needs_rewrite(index, root_label, known_labels, bindings)
                },
            }) || expr_needs_rewrite(value, root_label, known_labels, bindings)
                || modifiers
                    .iter()
                    .any(|m| expr_needs_rewrite(&m.value, root_label, known_labels, bindings))
        },
        Stmt::ReactiveBinding { target, value, .. } => {
            target.iter().any(|t| match t {
                TargetSegment::Static(s) => {
                    s == "self"
                        || root_label == Some(s.as_str())
                        || known_labels.contains(s.as_str())
                },
                TargetSegment::Indexed { base, index } => {
                    base == "self"
                        || root_label == Some(base.as_str())
                        || known_labels.contains(base.as_str())
                        || expr_needs_rewrite(index, root_label, known_labels, bindings)
                },
            }) || expr_needs_rewrite(value, root_label, known_labels, bindings)
        },
        Stmt::LetDecl { value, .. } => {
            expr_needs_rewrite(value, root_label, known_labels, bindings)
        },
        Stmt::Config { settings, .. } => settings
            .iter()
            .any(|p| expr_needs_rewrite(&p.value, root_label, known_labels, bindings)),
        Stmt::Stagger { modifiers, .. } => modifiers
            .iter()
            .any(|m| expr_needs_rewrite(&m.value, root_label, known_labels, bindings)),
        Stmt::Action(action, ..) => action.targets.iter().any(|t| {
            t == "self" || root_label == Some(t.as_str()) || known_labels.contains(t.as_str())
        }),
        Stmt::Conditional { condition, .. } => {
            expr_needs_rewrite(condition, root_label, known_labels, bindings)
        },
        Stmt::Match { scrutinee, .. } => {
            expr_needs_rewrite(scrutinee, root_label, known_labels, bindings)
        },
        _ => false,
    };

    if immediate {
        return true;
    }

    // Body recursion uses shared walk — any variant with a body is handled automatically.
    // If a new Stmt variant with a body is added, update walk.rs and this will work.
    let mut body_needs = false;
    crate::walk::recurse_stmt_bodies(stmt, &mut |body| {
        if !body_needs {
            body_needs =
                body.iter().any(|s| stmt_needs_rewrite(s, root_label, known_labels, bindings));
        }
    });
    body_needs
}

fn expr_needs_rewrite(
    expr: &Expr,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> bool {
    let mut needs = false;
    crate::walk::walk_expr(expr, &mut |e| {
        if !needs {
            match e {
                Expr::Ident(name) => {
                    needs = bindings.contains_key(name.as_str())
                        || name == "self"
                        || root_label == Some(name.as_str())
                        || known_labels.contains(name.as_str());
                },
                Expr::Path(parts) => {
                    needs = parts.iter().any(|p| {
                        bindings.contains_key(p.as_str())
                            || p == "self"
                            || root_label == Some(p.as_str())
                            || known_labels.contains(p.as_str())
                    });
                },
                _ => {},
            }
        }
    });
    needs
}

fn inline_item_needs_rewrite(
    item: &InlineItem,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> bool {
    let mut needs = false;
    crate::walk::walk_inline_item(item, &mut |i| {
        if needs {
            return;
        }
        match i {
            InlineItem::Labeled {
                label,
                props,
                modifiers,
                ..
            } => {
                needs = root_label == Some(label.as_str())
                    || known_labels.contains(label.as_str())
                    || props
                        .iter()
                        .any(|p| expr_needs_rewrite(&p.value, root_label, known_labels, bindings))
                    || modifiers
                        .iter()
                        .any(|m| expr_needs_rewrite(&m.value, root_label, known_labels, bindings));
            },
            InlineItem::Anonymous {
                props, modifiers, ..
            } => {
                needs = props
                    .iter()
                    .any(|p| expr_needs_rewrite(&p.value, root_label, known_labels, bindings))
                    || modifiers
                        .iter()
                        .any(|m| expr_needs_rewrite(&m.value, root_label, known_labels, bindings));
            },
            InlineItem::SlotFill { .. } | InlineItem::ForLoop { .. } | InlineItem::SlotMarker => {
                // These variants don't have immediate rewrite triggers;
                // their children are handled by walk_inline_item's recursion.
            },
        }
    });
    needs
}

pub(super) fn rewrite_stmt(
    stmt: &Stmt,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Stmt {
    match stmt {
        Stmt::Return { value, span, .. } => Stmt::Return {
            value: value
                .as_ref()
                .map(|expr| rewrite_expr(expr, prefix, root_label, known_labels, bindings)),
            span: *span,
        },
        Stmt::Expr(expr, span) => {
            Stmt::Expr(rewrite_expr(expr, prefix, root_label, known_labels, bindings), *span)
        },
        Stmt::Block { body, span, .. } => Stmt::Block {
            body: body
                .iter()
                .map(|s| rewrite_stmt(s, prefix, root_label, known_labels, bindings))
                .collect(),
            span: *span,
        },
        Stmt::ActorDecl {
            is_pub,
            label,
            array_index,
            ty,
            props,
            modifiers,
            children,
            span,
            ..
        } => Stmt::ActorDecl {
            is_pub: *is_pub,
            is_anonymous: false,
            label: rewrite_label(label, prefix, root_label, known_labels),
            array_index: array_index.clone(),
            ty: ty.clone(),
            props: rewrite_properties(props, prefix, root_label, known_labels, bindings),
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
            children: rewrite_inline_items(children, prefix, root_label, known_labels, bindings),
            span: *span,
        },
        Stmt::Assignment {
            target,
            property,
            value,
            modifiers,
            easing,
            value_span,
            span,
            ..
        } => Stmt::Assignment {
            target: rewrite_label_path(target, prefix, root_label, known_labels),
            property: property.clone(),
            value: rewrite_expr(value, prefix, root_label, known_labels, bindings),
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
            easing: *easing,
            value_span: *value_span,
            span: *span,
        },
        Stmt::Sequence { body, span, .. } => Stmt::Sequence {
            body: if body.iter().any(|s| stmt_needs_rewrite(s, root_label, known_labels, bindings))
            {
                body.iter()
                    .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                    .collect()
            } else {
                body.clone()
            },
            span: *span,
        },
        Stmt::Stagger {
            modifiers,
            body,
            span,
            ..
        } => Stmt::Stagger {
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
            body: if body.iter().any(|s| stmt_needs_rewrite(s, root_label, known_labels, bindings))
            {
                body.iter()
                    .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                    .collect()
            } else {
                body.clone()
            },
            span: *span,
        },
        Stmt::Action(action, span) => Stmt::Action(
            Action {
                verb: action.verb.clone(),
                targets: action
                    .targets
                    .iter()
                    .map(|target| rewrite_label_ref(target, prefix, root_label, known_labels))
                    .collect(),
                target_index: action
                    .target_index
                    .iter()
                    .map(|index| {
                        index.as_ref().map(|expr| {
                            rewrite_expr(expr, prefix, root_label, known_labels, bindings)
                        })
                    })
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
            },
            *span,
        ),
        Stmt::LetDecl {
            is_pub,
            name,
            value,
            span,
            ..
        } => Stmt::LetDecl {
            is_pub: *is_pub,
            name: name.clone(),
            value: rewrite_expr(value, prefix, root_label, known_labels, bindings),
            span: *span,
        },
        Stmt::TypeAlias {
            is_pub,
            name,
            annotation,
            span,
        } => Stmt::TypeAlias {
            is_pub: *is_pub,
            name: name.clone(),
            annotation: annotation.clone(),
            span: *span,
        },
        Stmt::Keyframe {
            time, body, span, ..
        } => Stmt::Keyframe {
            time: time.clone(),
            body: if body.iter().any(|s| stmt_needs_rewrite(s, root_label, known_labels, bindings))
            {
                body.iter()
                    .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                    .collect()
            } else {
                body.clone()
            },
            span: *span,
        },
        Stmt::RelativeKeyframe {
            offset, body, span, ..
        } => Stmt::RelativeKeyframe {
            offset: offset.clone(),
            body: if body.iter().any(|s| stmt_needs_rewrite(s, root_label, known_labels, bindings))
            {
                body.iter()
                    .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                    .collect()
            } else {
                body.clone()
            },
            span: *span,
        },
        Stmt::Always { body, span, .. } => Stmt::Always {
            body: if body.iter().any(|s| stmt_needs_rewrite(s, root_label, known_labels, bindings))
            {
                body.iter()
                    .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                    .collect()
            } else {
                body.clone()
            },
            span: *span,
        },
        Stmt::ReactiveBinding {
            target,
            property,
            value,
            value_span,
            span,
            ..
        } => Stmt::ReactiveBinding {
            target: rewrite_label_path(target, prefix, root_label, known_labels),
            property: property.clone(),
            value: rewrite_expr(value, prefix, root_label, known_labels, bindings),
            value_span: *value_span,
            span: *span,
        },
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            span,
            ..
        } => Stmt::Conditional {
            condition: rewrite_expr(condition, prefix, root_label, known_labels, bindings),
            then_branch: if then_branch
                .iter()
                .any(|s| stmt_needs_rewrite(s, root_label, known_labels, bindings))
            {
                then_branch
                    .iter()
                    .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                    .collect()
            } else {
                then_branch.clone()
            },
            else_branch: else_branch.as_ref().map(|branch| {
                if branch.iter().any(|s| stmt_needs_rewrite(s, root_label, known_labels, bindings))
                {
                    branch
                        .iter()
                        .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                        .collect()
                } else {
                    branch.clone()
                }
            }),
            span: *span,
        },
        Stmt::Match {
            scrutinee,
            arms,
            span,
            ..
        } => Stmt::Match {
            scrutinee: rewrite_expr(scrutinee, prefix, root_label, known_labels, bindings),
            arms: arms
                .iter()
                .map(|(pat, body)| {
                    let rewritten_body = if body
                        .iter()
                        .any(|s| stmt_needs_rewrite(s, root_label, known_labels, bindings))
                    {
                        body.iter()
                            .map(|stmt| {
                                rewrite_stmt(stmt, prefix, root_label, known_labels, bindings)
                            })
                            .collect()
                    } else {
                        body.clone()
                    };
                    (pat.clone(), rewritten_body)
                })
                .collect(),
            span: *span,
        },
        Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body,
            modifiers,
            span,
            ..
        } => Stmt::ForLoop {
            var: var.clone(),
            index_var: index_var.clone(),
            iterable: rewrite_expr(iterable, prefix, root_label, known_labels, bindings),
            body: if body.iter().any(|s| stmt_needs_rewrite(s, root_label, known_labels, bindings))
            {
                body.iter()
                    .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                    .collect()
            } else {
                body.clone()
            },
            modifiers: rewrite_modifiers(modifiers, prefix, root_label, known_labels, bindings),
            span: *span,
        },
        Stmt::ComponentDef(definition, span) => Stmt::ComponentDef(
            ComponentDef {
                is_pub: definition.is_pub,
                name: definition.name.clone(),
                params: definition.params.clone(),
                body: if definition
                    .body
                    .iter()
                    .any(|s| stmt_needs_rewrite(s, root_label, known_labels, bindings))
                {
                    definition
                        .body
                        .iter()
                        .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                        .collect()
                } else {
                    definition.body.clone()
                },
            },
            *span,
        ),
        Stmt::FnDecl {
            is_pub,
            name,
            params,
            return_type,
            body,
            span,
            ..
        } => Stmt::FnDecl {
            is_pub: *is_pub,
            name: name.clone(),
            params: params.clone(),
            return_type: return_type.clone(),
            body: if body.iter().any(|s| stmt_needs_rewrite(s, root_label, known_labels, bindings))
            {
                body.iter()
                    .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                    .collect()
            } else {
                body.clone()
            },
            span: *span,
        },
        Stmt::Config { settings, span, .. } => Stmt::Config {
            settings: rewrite_properties(settings, prefix, root_label, known_labels, bindings),
            span: *span,
        },
        Stmt::Import {
            path, alias, span, ..
        } => Stmt::Import {
            path: path.clone(),
            alias: alias.clone(),
            span: *span,
        },
        Stmt::Comment(comment, span) => Stmt::Comment(comment.clone(), *span),
        // Multi-scene composition statements: pass through unchanged
        Stmt::Scene {
            name,
            config,
            body,
            span,
        } => Stmt::Scene {
            name: name.clone(),
            config: rewrite_properties(config, prefix, root_label, known_labels, bindings),
            body: body
                .iter()
                .map(|stmt| rewrite_stmt(stmt, prefix, root_label, known_labels, bindings))
                .collect(),
            span: *span,
        },
        Stmt::Play {
            scene_name,
            transition,
            span,
        } => Stmt::Play {
            scene_name: scene_name.clone(),
            transition: transition.clone(),
            span: *span,
        },
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
                array_index,
                ty,
                props,
                modifiers,
                children,
            } => InlineItem::Labeled {
                label: rewrite_label(label, prefix, root_label, known_labels),
                array_index: array_index.clone(),
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
            InlineItem::ForLoop {
                var,
                index_var,
                iterable,
                body,
            } => InlineItem::ForLoop {
                var: var.clone(),
                index_var: index_var.clone(),
                iterable: rewrite_expr(iterable, prefix, root_label, known_labels, bindings),
                body: rewrite_inline_items(body, prefix, root_label, known_labels, bindings),
            },
            InlineItem::SlotMarker => InlineItem::SlotMarker,
            InlineItem::SlotFill { slot, items } => InlineItem::SlotFill {
                slot: slot.clone(),
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
fn rewrite_path_segment(seg: &str, prefix: &str, root_label: Option<&str>) -> String {
    if seg == "scene" {
        seg.to_string()
    } else if root_label == Some(seg) {
        prefix.to_string()
    } else {
        seg.to_string()
    }
}

fn split_rewritten_label(label: &str) -> Vec<String> {
    label.split('.').map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_media_statement_position_expressions() {
        let stmt = Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "logo".to_string(),
            array_index: None,
            ty: "Svg".to_string(),
            props: vec![
                Property {
                    name: "url".to_string(),
                    value: Expr::Str("examples/vector.svg".to_string()),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "at".to_string(),
                    value: Expr::Ident("badge".to_string()),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "anchor".to_string(),
                    value: Expr::Path(vec!["scene".to_string(), "top".to_string()]),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "offset".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(0.0), Expr::Ident("delta".to_string())]),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "scale".to_string(),
                    value: Expr::Num(1.0),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        };
        let known_labels = HashSet::from(["logo".to_string(), "badge".to_string()]);
        let bindings = HashMap::from([("delta".to_string(), Expr::Num(48.0))]);

        let rewritten = rewrite_stmt(&stmt, "hero", None, &known_labels, &bindings);

        match rewritten {
            Stmt::ActorDecl {
                label, ty, props, ..
            } => {
                assert_eq!(label, "hero.logo");
                assert_eq!(ty, "Svg");
                let get_prop = |name: &str| -> Option<&Expr> {
                    props.iter().find(|p| p.name == name).map(|p| &p.value)
                };
                assert_eq!(get_prop("at"), Some(&Expr::Ident("hero.badge".to_string())));
                assert_eq!(
                    get_prop("anchor"),
                    Some(&Expr::Path(vec!["scene".to_string(), "top".to_string()]))
                );
                assert_eq!(
                    get_prop("offset"),
                    Some(&Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(48.0)]))
                );
            },
            other => unreachable!("expected rewritten actor decl statement, got {other:?}"),
        }
    }
}
