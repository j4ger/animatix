//! Shared AST/InlineItem conversion and extraction helpers.

use animatix_syntax::ast::{ComponentDef, InlineItem, Stmt};

pub(super) fn stmt_has_label(stmt: &Stmt, label: &str) -> bool {
    matches!(stmt, Stmt::ActorDecl { label: l, .. } if l == label)
}

pub(super) fn stmt_to_inline_item(stmt: Stmt) -> InlineItem {
    match stmt {
        Stmt::ActorDecl {
            is_anonymous: true,
            ty,
            props,
            modifiers,
            children,
            ..
        } => InlineItem::Anonymous {
            ty,
            props,
            modifiers,
            children,
        },
        Stmt::ActorDecl {
            label,
            ty,
            props,
            modifiers,
            children,
            array_index,
            ..
        } => InlineItem::Labeled {
            label,
            array_index,
            ty,
            props,
            modifiers,
            children,
        },
        other => {
            tracing::warn!(
                "stmt_to_inline_item: non-ActorDecl statement discarded: {:?}",
                std::mem::discriminant(&other)
            );
            InlineItem::Anonymous {
                ty: "Group".into(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
            }
        },
    }
}

pub(super) fn inline_item_to_stmt(item: InlineItem, index: usize) -> Stmt {
    match item {
        InlineItem::ForLoop {
            var,
            index_var,
            iterable,
            body,
        } => Stmt::ForLoop {
            var,
            index_var,
            iterable,
            modifiers: vec![],
            body: body
                .into_iter()
                .enumerate()
                .map(|(i, item)| inline_item_to_stmt(item, i))
                .collect(),
            span: None,
        },
        InlineItem::Labeled {
            label,
            ty,
            props,
            modifiers,
            children,
            array_index,
        } => Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label,
            array_index,
            ty,
            props,
            modifiers,
            children,
            span: None,
        },
        InlineItem::Anonymous {
            ty,
            props,
            modifiers,
            children,
        } => Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: true,
            label: format!("__anon_root_{}", index),
            array_index: None,
            ty,
            props,
            modifiers,
            children,
            span: None,
        },
        InlineItem::SlotFill { slot, items, .. } => Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: format!("__slot_{}", slot),
            array_index: None,
            ty: "Group".into(),
            props: vec![],
            modifiers: vec![],
            children: items,
            span: None,
        },
        InlineItem::SlotMarker => {
            // Slot markers can't be converted to statements
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "__slot_marker".into(),
                array_index: None,
                ty: "Group".into(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            }
        },
    }
}

pub(super) fn extract_inline_item(stmts: &mut [Stmt], label: &str) -> Option<InlineItem> {
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::ActorDecl { children, .. } => {
                if let Some(idx) = children.iter().position(|c| inline_item_has_label(c, label)) {
                    return Some(children.remove(idx));
                }
                // Recurse into children
                for child in children.iter_mut() {
                    if let Some(extracted) = extract_from_inline_item(child, label) {
                        return Some(extracted);
                    }
                }
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::FnDecl { body, .. }
            | Stmt::Block { body, .. } => {
                if let Some(extracted) = extract_inline_item(body, label) {
                    return Some(extracted);
                }
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                if let Some(extracted) = extract_inline_item(then_branch, label) {
                    return Some(extracted);
                }
                if let Some(else_branch) = else_branch {
                    if let Some(extracted) = extract_inline_item(else_branch, label) {
                        return Some(extracted);
                    }
                }
            },
            Stmt::ForLoop { body, .. } => {
                if let Some(extracted) = extract_inline_item(body, label) {
                    return Some(extracted);
                }
            },
            _ => {},
        }
    }
    None
}

pub(super) fn extract_from_inline_item(item: &mut InlineItem, label: &str) -> Option<InlineItem> {
    match item {
        InlineItem::Labeled { children, .. }
        | InlineItem::Anonymous { children, .. }
        | InlineItem::ForLoop { body: children, .. } => {
            if let Some(idx) = children.iter().position(|c| inline_item_has_label(c, label)) {
                return Some(children.remove(idx));
            }
            for child in children.iter_mut() {
                if let Some(extracted) = extract_from_inline_item(child, label) {
                    return Some(extracted);
                }
            }
            None
        },
        InlineItem::SlotFill { items, .. } => {
            if let Some(idx) = items.iter().position(|c| inline_item_has_label(c, label)) {
                return Some(items.remove(idx));
            }
            for item in items.iter_mut() {
                if let Some(extracted) = extract_from_inline_item(item, label) {
                    return Some(extracted);
                }
            }
            None
        },
        InlineItem::SlotMarker => None,
    }
}

pub(super) fn inline_item_has_label(item: &InlineItem, label: &str) -> bool {
    matches!(item, InlineItem::Labeled { label: l, .. } if l == label)
}
