//! Container child reordering and actor reparenting.

use animatix_syntax::ast::{InlineItem, Stmt};

use super::super::SourceEditError;
use super::super::apply::find_actor_decl_mut;
use super::common::{
    extract_inline_item, inline_item_to_stmt, stmt_has_label, stmt_to_inline_item,
};

pub(in crate::source_edit) fn reorder_container_children(
    stmts: &mut [Stmt],
    container: &str,
    new_order: Vec<String>,
) -> Result<(), SourceEditError> {
    let Stmt::ActorDecl { children, .. } =
        find_actor_decl_mut(stmts, container).ok_or_else(|| {
            SourceEditError::ContainerNotFound {
                container: container.to_string(),
            }
        })?
    else {
        return Err(SourceEditError::ContainerNotFound {
            container: container.to_string(),
        });
    };

    let mut labeled = std::collections::BTreeMap::<String, InlineItem>::new();
    let mut remaining = Vec::new();

    for item in children.drain(..) {
        match &item {
            InlineItem::Labeled { label, .. } if new_order.iter().any(|l| l == label) => {
                labeled.insert(label.clone(), item);
            },
            _ => remaining.push(item),
        }
    }

    let mut reordered = Vec::new();
    for label in new_order {
        if let Some(item) = labeled.remove(&label) {
            reordered.push(item);
        }
    }

    reordered.extend(remaining);
    reordered.extend(labeled.into_values());
    *children = reordered;
    Ok(())
}

pub(in crate::source_edit) fn reparent_actor(
    stmts: &mut Vec<Stmt>,
    actor: &str,
    new_parent: Option<&str>,
) -> Result<(), SourceEditError> {
    // 1. Find and extract the actor from its current location.
    let extracted = extract_inline_item(stmts, actor);
    let item = if let Some(item) = extracted {
        item
    } else {
        // Actor not found as an inline child — try top-level Stmt.
        let idx = stmts.iter().position(|s| stmt_has_label(s, actor)).ok_or_else(|| {
            SourceEditError::ActorNotFound {
                actor: actor.to_string(),
            }
        })?;
        let stmt = stmts.remove(idx);
        let item = stmt_to_inline_item(stmt);
        return insert_under_parent(stmts, item, new_parent);
    };

    insert_under_parent(stmts, item, new_parent)
}

fn insert_under_parent(
    stmts: &mut Vec<Stmt>,
    item: InlineItem,
    new_parent: Option<&str>,
) -> Result<(), SourceEditError> {
    match new_parent {
        None => {
            // Make top-level — anonymous items need a deterministic label.
            let index = stmts.len();
            stmts.push(inline_item_to_stmt(item, index));
            Ok(())
        },
        Some(parent_label) => {
            let parent_stmt = find_actor_decl_mut(stmts, parent_label).ok_or_else(|| {
                SourceEditError::ParentNotFound {
                    parent: parent_label.to_string(),
                }
            })?;
            match parent_stmt {
                Stmt::ActorDecl { ty, children, .. } if is_container_type(ty) => {
                    children.push(item);
                    Ok(())
                },
                _ => {
                    // Target is not a container — wrap both in a new Group.
                    // First, extract the target actor too.
                    let target_extracted = extract_inline_item(stmts, parent_label);
                    let target_item = if let Some(target) = target_extracted {
                        target
                    } else {
                        // Target is top-level
                        let idx = stmts
                            .iter()
                            .position(|s| stmt_has_label(s, parent_label))
                            .ok_or_else(|| SourceEditError::ParentNotFound {
                                parent: parent_label.to_string(),
                            })?;
                        let stmt = stmts.remove(idx);
                        stmt_to_inline_item(stmt)
                    };

                    let group = InlineItem::Labeled {
                        label: format!("{}_group", parent_label),
                        array_index: None,
                        ty: "Group".into(),
                        props: vec![],
                        modifiers: vec![],
                        children: vec![target_item, item],
                    };
                    stmts.push(inline_item_to_stmt(group, stmts.len()));
                    Ok(())
                },
            }
        },
    }
}

fn is_container_type(ty: &str) -> bool {
    animatix_syntax::animatix_std::caps_for_type(ty)
        .is_some_and(|caps| caps.is_container || caps.is_effect_scope())
}
