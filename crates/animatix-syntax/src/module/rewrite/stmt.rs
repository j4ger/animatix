use super::*;

pub(crate) fn rewrite_stmt(
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
        Stmt::Step { index, span } => Stmt::Step {
            index: *index,
            span: *span,
        },
    }
}

pub(crate) fn rewrite_inline_items(
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
