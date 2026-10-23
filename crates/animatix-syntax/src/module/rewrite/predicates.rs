use super::*;

pub(super) fn stmt_needs_rewrite(
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
        Stmt::Expr(expr, ..) => expr_needs_rewrite(expr, root_label, known_labels, bindings),
        Stmt::Return { value, .. } => value
            .as_ref()
            .is_some_and(|v| expr_needs_rewrite(v, root_label, known_labels, bindings)),
        // Variants whose bodies are checked by the shared recursion below:
        // their immediate check is intentionally false.
        Stmt::Keyframe { .. }
        | Stmt::RelativeKeyframe { .. }
        | Stmt::Sequence { .. }
        | Stmt::Always { .. }
        | Stmt::ForLoop { .. }
        | Stmt::Block { .. }
        | Stmt::Scene { .. } => false,
        // Variants that never need label rewriting:
        Stmt::Import { .. } | Stmt::TypeAlias { .. } | Stmt::Play { .. } => false,
        // Component bodies are rewritten at instance-expansion time with
        // their own gate (see rewrite_stmt's ComponentDef handling).
        Stmt::ComponentDef(..) => false,
        Stmt::Comment(..) | Stmt::Step { .. } => false,
        Stmt::FnDecl { .. } => false, // its body gate lives in rewrite_stmt's FnDecl arm
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

pub(super) fn expr_needs_rewrite(
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

pub(super) fn inline_item_needs_rewrite(
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
