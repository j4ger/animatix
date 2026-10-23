//! Actor deletion.

use animatix_syntax::ast::{InlineItem, Stmt};

use super::super::SourceEditError;
use super::common::inline_item_has_label;

/// Delete an actor declaration by label.
///
/// Only removes top-level `ActorDecl` statements. Does not remove
/// keyframe assignments (they become orphaned and harmless).
pub(in crate::source_edit) fn delete_actor(
    stmts: &mut Vec<Stmt>,
    label: &str,
) -> Result<(), SourceEditError> {
    if delete_actor_recursive(stmts, label) {
        Ok(())
    } else {
        Err(SourceEditError::ActorNotFound {
            actor: label.to_string(),
        })
    }
}

/// Recursively search for and remove an actor by label.
/// Returns true if the actor was found and removed.
fn delete_actor_recursive(stmts: &mut Vec<Stmt>, label: &str) -> bool {
    // Try top-level first
    if let Some(pos) = stmts
        .iter()
        .position(|s| matches!(s, Stmt::ActorDecl { label: l, .. } if l == label))
    {
        stmts.remove(pos);
        return true;
    }
    // Recurse into bodies
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::ActorDecl { children, .. } => {
                if delete_from_inline_items(children, label) {
                    return true;
                }
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::FnDecl { body, .. }
            | Stmt::Block { body, .. } => {
                if delete_actor_recursive(body, label) {
                    return true;
                }
            },
            Stmt::ComponentDef(def, _) => {
                if delete_actor_recursive(&mut def.body, label) {
                    return true;
                }
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                if delete_actor_recursive(then_branch, label) {
                    return true;
                }
                if let Some(else_stmts) = else_branch {
                    if delete_actor_recursive(else_stmts, label) {
                        return true;
                    }
                }
            },
            Stmt::ForLoop { body, .. } => {
                if delete_actor_recursive(body, label) {
                    return true;
                }
            },
            _ => {},
        }
    }
    false
}

/// Delete an actor from a list of inline items (children of a container).
fn delete_from_inline_items(items: &mut Vec<InlineItem>, label: &str) -> bool {
    if let Some(pos) = items.iter().position(|item| inline_item_has_label(item, label)) {
        items.remove(pos);
        return true;
    }
    for item in items.iter_mut() {
        match item {
            InlineItem::Anonymous { children, .. }
            | InlineItem::Labeled { children, .. }
            | InlineItem::ForLoop { body: children, .. } => {
                if delete_from_inline_items(children, label) {
                    return true;
                }
            },
            InlineItem::SlotFill { items, .. } => {
                if delete_from_inline_items(items, label) {
                    return true;
                }
            },
            InlineItem::SlotMarker => {},
        }
    }
    false
}
