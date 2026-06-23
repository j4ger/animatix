//! Edits related to actors: property changes, insertion, reordering, reparenting, and renaming.

use animatix_syntax::ast::{ComponentDef, Expr, InlineItem, Property, Stmt};

use animatix_syntax::walk::{walk_inline_items_mut, walk_stmts_mut};

use super::apply::{find_actor_decl_mut, find_assignment_mut, find_prop_mut};
use super::apply::canonical_to_source;
use super::SourceEditError;

// ---------------------------------------------------------------------------
// SetProperty
// ---------------------------------------------------------------------------

pub(super) fn set_property(stmts: &mut [Stmt], actor: &str, property: &str, value: Expr) -> Result<(), SourceEditError> {
    let source_prop = canonical_to_source(property);

    // 1. Try to find an ActorDecl and update its property.
    if let Some(actor_decl) = find_actor_decl_mut(stmts, actor) {
        if let Some(prop) = find_prop_mut(actor_decl, source_prop) {
            prop.value = value.clone();
            return Ok(());
        }
    }

    // 2. Try to find an Assignment statement and update its value.
    if let Some(Stmt::Assignment { value: val, .. }) = find_assignment_mut(stmts, actor, source_prop) {
        *val = value;
        return Ok(());
    }

    Err(SourceEditError::PropertyNotFound {
        actor: actor.to_string(),
        property: property.to_string(),
    })
}

// ---------------------------------------------------------------------------
// InsertProperty
// ---------------------------------------------------------------------------

pub(super) fn insert_property(stmts: &mut [Stmt], actor: &str, property: &str, value: Expr) -> Result<(), SourceEditError> {
    let source_prop = canonical_to_source(property);

    let actor_decl = find_actor_decl_mut(stmts, actor)
        .ok_or_else(|| SourceEditError::ActorNotFound { actor: actor.to_string() })?;

    // Check if property already exists
    if find_prop_mut(actor_decl, source_prop).is_some() {
        return Err(SourceEditError::PropertyAlreadyExists {
            actor: actor.to_string(),
            property: property.to_string(),
        });
    }

    // Add new property
    if let Stmt::ActorDecl { ty, props, .. } = actor_decl {
        // Text, Math, Code types use generic props; Svg/Image use fixed schemas
        if ty == "Svg" || ty == "Image" {
            return Err(SourceEditError::FixedSchemaUnsupported {
                actor: actor.to_string(),
                ty: ty.clone(),
            });
        }
        props.push(Property {
            name: source_prop.into(),
            value,
            value_span: None,
            trailing_comment: None,
        });
        Ok(())
    } else {
        Err(SourceEditError::ActorNotFound { actor: actor.to_string() })
    }
}

// ---------------------------------------------------------------------------
// InsertActor
// ---------------------------------------------------------------------------

pub(super) fn insert_actor(
    stmts: &mut Vec<Stmt>,
    ty: &str,
    label: &str,
    props: Vec<Property>,
    container: Option<&str>,
    _time_s: f64,
) -> Result<(), SourceEditError> {
    if let Some(container_label) = container {
        // Insert as a child of the specified container
        let container_decl = find_actor_decl_mut(stmts, container_label)
            .ok_or_else(|| SourceEditError::ContainerNotFound {
                container: container_label.to_string(),
            })?;
        if let Stmt::ActorDecl { children, .. } = container_decl {
            children.push(InlineItem::Labeled {
                label: label.into(),
                array_index: None,
                ty: ty.into(),
                props: props.clone(),
                modifiers: vec![],
                children: vec![],
            });
            Ok(())
        } else {
            Err(SourceEditError::ContainerNotFound {
                container: container_label.to_string(),
            })
        }
    } else {
        // Insert at top-level
        stmts.push(Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: label.into(),
            array_index: None,
            ty: ty.into(),
            props,
            modifiers: vec![],
            children: vec![],
            span: None,
        });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ReorderContainerChildren
// ---------------------------------------------------------------------------

pub(super) fn reorder_container_children(stmts: &mut [Stmt], container: &str, new_order: Vec<String>) -> Result<(), SourceEditError> {
    let Stmt::ActorDecl { children, .. } = find_actor_decl_mut(stmts, container)
        .ok_or_else(|| SourceEditError::ContainerNotFound {
            container: container.to_string(),
        })? else {
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
            }
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

// ---------------------------------------------------------------------------
// Reparent
// ---------------------------------------------------------------------------

pub(super) fn reparent_actor(stmts: &mut Vec<Stmt>, actor: &str, new_parent: Option<&str>) -> Result<(), SourceEditError> {
    // 1. Find and extract the actor from its current location.
    let extracted = extract_inline_item(stmts, actor);
    let item = if let Some(item) = extracted {
        item
    } else {
        // Actor not found as an inline child — try top-level Stmt.
        let idx = stmts.iter().position(|s| stmt_has_label(s, actor))
            .ok_or_else(|| SourceEditError::ActorNotFound { actor: actor.to_string() })?;
        let stmt = stmts.remove(idx);
        let item = stmt_to_inline_item(stmt);
        return insert_under_parent(stmts, item, new_parent);
    };

    insert_under_parent(stmts, item, new_parent)
}

fn insert_under_parent(stmts: &mut Vec<Stmt>, item: InlineItem, new_parent: Option<&str>) -> Result<(), SourceEditError> {
    match new_parent {
        None => {
            // Make top-level — anonymous items need a deterministic label.
            let index = stmts.len();
            stmts.push(inline_item_to_stmt(item, index));
            Ok(())
        }
        Some(parent_label) => {
            let parent_stmt = find_actor_decl_mut(stmts, parent_label)
                .ok_or_else(|| SourceEditError::ParentNotFound {
                    parent: parent_label.to_string(),
                })?;
            match parent_stmt {
                Stmt::ActorDecl { ty, children, .. }
                    if is_container_type(ty) =>
                {
                    children.push(item);
                    Ok(())
                }
                _ => {
                    // Target is not a container — wrap both in a new Group.
                    // First, extract the target actor too.
                    let target_extracted = extract_inline_item(stmts, parent_label);
                    let target_item = if let Some(target) = target_extracted {
                        target
                    } else {
                        // Target is top-level
                        let idx = stmts.iter().position(|s| stmt_has_label(s, parent_label))
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
                }
            }
        }
    }
}

fn is_container_type(ty: &str) -> bool {
    matches!(ty, "Row" | "Col" | "Grid" | "Stack" | "Group" | "Mask")
}

fn stmt_has_label(stmt: &Stmt, label: &str) -> bool {
    matches!(stmt, Stmt::ActorDecl { label: l, .. } if l == label)
}

fn stmt_to_inline_item(stmt: Stmt) -> InlineItem {
    match stmt {
        Stmt::ActorDecl { is_anonymous: true, ty, props, modifiers, children, .. } => {
            InlineItem::Anonymous {
                ty,
                props,
                modifiers,
                children,
            }
        }
        Stmt::ActorDecl { label, ty, props, modifiers, children, array_index, .. } => InlineItem::Labeled {
            label,
            array_index,
            ty,
            props,
            modifiers,
            children,
        },
        other => {
            tracing::warn!("stmt_to_inline_item: non-ActorDecl statement discarded: {:?}", std::mem::discriminant(&other));
            InlineItem::Anonymous {
                ty: "Group".into(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
            }
        }
    }
}

fn inline_item_to_stmt(item: InlineItem, index: usize) -> Stmt {
    match item {
        InlineItem::ForLoop { var, index_var, iterable, body } => {
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body: body.into_iter().enumerate()
                    .map(|(i, item)| inline_item_to_stmt(item, i))
                    .collect(),
                span: None,
            }
        }
        InlineItem::Labeled { label, ty, props, modifiers, children, array_index } => Stmt::ActorDecl {
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
        InlineItem::Anonymous { ty, props, modifiers, children } => Stmt::ActorDecl {
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
        }
    }
}

fn extract_inline_item(stmts: &mut [Stmt], label: &str) -> Option<InlineItem> {
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
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                if let Some(extracted) = extract_inline_item(body, label) {
                    return Some(extracted);
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                if let Some(extracted) = extract_inline_item(then_branch, label) {
                    return Some(extracted);
                }
                if let Some(else_branch) = else_branch {
                    if let Some(extracted) = extract_inline_item(else_branch, label) {
                        return Some(extracted);
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                if let Some(extracted) = extract_inline_item(body, label) {
                    return Some(extracted);
                }
            }
            _ => {}
        }
    }
    None
}

fn extract_from_inline_item(item: &mut InlineItem, label: &str) -> Option<InlineItem> {
    match item {
        InlineItem::Labeled { children, .. } | InlineItem::Anonymous { children, .. } | InlineItem::ForLoop { body: children, .. } => {
            if let Some(idx) = children.iter().position(|c| inline_item_has_label(c, label)) {
                return Some(children.remove(idx));
            }
            for child in children.iter_mut() {
                if let Some(extracted) = extract_from_inline_item(child, label) {
                    return Some(extracted);
                }
            }
            None
        }
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
        }
        InlineItem::SlotMarker => None,
    }
}

fn inline_item_has_label(item: &InlineItem, label: &str) -> bool {
    matches!(item, InlineItem::Labeled { label: l, .. } if l == label)
}

// ---------------------------------------------------------------------------
// Rename
// ---------------------------------------------------------------------------

/// Rename all references to `old_label` into `new_label` throughout the AST.
pub(crate) fn rename_all_references(stmts: &mut [Stmt], old_label: &str, new_label: &str) {
    rename_in_stmt(stmts, old_label, new_label);
}

/// Rename actor references in a statement list using shared walk primitives.
///
/// Uses `walk_stmts_mut` for recursive statement traversal and
/// `walk_inline_items_mut` for inline item children of ActorDecl nodes.
fn rename_in_stmt(stmts: &mut [Stmt], old_label: &str, new_label: &str) {
    walk_stmts_mut(stmts, &mut |stmt| {
        match stmt {
            Stmt::ActorDecl { label, children, .. } => {
                if label == old_label {
                    *label = new_label.to_string();
                }
                walk_inline_items_mut(children, &mut |item| {
                    if let InlineItem::Labeled { label, .. } = item {
                        if label == old_label {
                            *label = new_label.to_string();
                        }
                    }
                });
            }
            Stmt::Assignment { target, .. } => {
                for part in target.iter_mut() {
                    if part == old_label {
                        *part = new_label.to_string();
                    }
                }
            }
            Stmt::Action(action, _) => {
                for t in action.targets.iter_mut() {
                    if t == old_label {
                        *t = new_label.to_string();
                    }
                }
            }
            _ => {}
        }
    });
}

// ---------------------------------------------------------------------------
// InsertSnippet
// ---------------------------------------------------------------------------

/// Insert a parsed snippet (AST fragment) into the statement list.
///
/// If `time_s` is provided, tries to insert inside the keyframe at that time.
/// If `container` is provided, tries to insert as children of that container.
/// Otherwise, appends to the top level.
pub(super) fn insert_snippet(
    stmts: &mut Vec<Stmt>,
    fragment: Vec<Stmt>,
    time_s: Option<f64>,
    container: Option<&str>,
) -> Result<(), SourceEditError> {
    // If a container is specified, insert as children of that container.
    if let Some(container_label) = container {
        let container_decl = find_actor_decl_mut(stmts, container_label)
            .ok_or_else(|| SourceEditError::ContainerNotFound {
                container: container_label.to_string(),
            })?;
        if let Stmt::ActorDecl { children, .. } = container_decl {
            // Convert top-level stmts to inline items and append.
            for stmt in fragment {
                children.push(stmt_to_inline_item(stmt));
            }
            return Ok(());
        } else {
            return Err(SourceEditError::ContainerNotFound {
                container: container_label.to_string(),
            });
        }
    }

    // If a time is specified, try to find the keyframe at that time.
    if let Some(target_time) = time_s {
        for stmt in stmts.iter_mut() {
            if let Stmt::Keyframe { time, body, .. } = stmt {
                let kf_time = super::apply::time_to_seconds(time);
                if (kf_time - target_time).abs() < 0.001 {
                    body.extend(fragment);
                    return Ok(());
                }
            }
        }
        // No matching keyframe found — create one.
        stmts.push(Stmt::Keyframe {
            time: animatix_syntax::ast::Time::Seconds(target_time),
            body: fragment,
            span: None,
        });
        return Ok(());
    }

    // Default: append to top level.
    stmts.extend(fragment);
    Ok(())
}

/// Delete an actor declaration by label.
///
/// Only removes top-level `ActorDecl` statements. Does not remove
/// keyframe assignments (they become orphaned and harmless).
pub(super) fn delete_actor(stmts: &mut Vec<Stmt>, label: &str) -> Result<(), SourceEditError> {
    if delete_actor_recursive(stmts, label) {
        Ok(())
    } else {
        Err(SourceEditError::ActorNotFound { actor: label.to_string() })
    }
}

/// Recursively search for and remove an actor by label.
/// Returns true if the actor was found and removed.
fn delete_actor_recursive(stmts: &mut Vec<Stmt>, label: &str) -> bool {
    // Try top-level first
    if let Some(pos) = stmts.iter().position(|s| matches!(s, Stmt::ActorDecl { label: l, .. } if l == label)) {
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
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentAction { body, .. } => {
                if delete_actor_recursive(body, label) {
                    return true;
                }
            }
            Stmt::ComponentDef(def, _) => {
                if delete_actor_recursive(&mut def.body, label) {
                    return true;
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                if delete_actor_recursive(then_branch, label) {
                    return true;
                }
                if let Some(else_stmts) = else_branch {
                    if delete_actor_recursive(else_stmts, label) {
                        return true;
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                if delete_actor_recursive(body, label) {
                    return true;
                }
            }
            _ => {}
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
            | InlineItem::Labeled { children, .. } | InlineItem::ForLoop { body: children, .. } => {
                if delete_from_inline_items(children, label) {
                    return true;
                }
            }
            InlineItem::SlotFill { items, .. } => {
                if delete_from_inline_items(items, label) {
                    return true;
                }
            }
            InlineItem::SlotMarker => {}
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::apply::{find_actor_decl_mut, find_assignment_mut, find_prop_mut};
    use super::super::apply::{SourceEdit, apply_edit};
    use animatix_syntax::ast::{ComponentDef, Expr, InlineItem, Property, Stmt};
    use animatix_syntax::parser::{parser, parser_simple};
    use chumsky::Parser;

    fn parse(source: &str) -> Vec<Stmt> {
        parser_simple().parse(source).into_result().expect("failed to parse test source")
    }

    #[test]
    fn set_existing_property() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200), color: red"#);
        let edit = SourceEdit::SetProperty {
            actor: "btn".into(),
            property: "color".into(),
            value: Expr::Ident("blue".into()),
        };
        assert!(apply_edit(&mut stmts, edit).is_ok());

        let actor = find_actor_decl_mut(&mut stmts, "btn").expect("actor 'btn' should exist");
        let prop = find_prop_mut(actor, "color").expect("property 'color' should exist");
        assert_eq!(prop.value, Expr::Ident("blue".into()));
    }

    #[test]
    fn set_property_uses_canonical_name() {
        let mut stmts = parse(r#"#0s
btn: Rect, at: (100, 200)"#);
        let edit = SourceEdit::SetProperty {
            actor: "btn".into(),
            property: "position".into(),
            value: Expr::Tuple(vec![Expr::Num(150.0), Expr::Num(250.0)]),
        };
        assert!(apply_edit(&mut stmts, edit).is_ok());

        let actor = find_actor_decl_mut(&mut stmts, "btn").expect("actor 'btn' should exist");
        let prop = find_prop_mut(actor, "at").expect("property 'at' should exist");
        assert_eq!(
            prop.value,
            Expr::Tuple(vec![Expr::Num(150.0), Expr::Num(250.0)])
        );
    }

    #[test]
    fn set_assignment_value() {
        let mut stmts = parse(r#"#2s
btn.color = red"#);
        let edit = SourceEdit::SetProperty {
            actor: "btn".into(),
            property: "color".into(),
            value: Expr::Ident("blue".into()),
        };
        assert!(apply_edit(&mut stmts, edit).is_ok());

        let assignment = find_assignment_mut(&mut stmts, "btn", "color"
        ).expect("assignment 'btn.color' should exist");
        if let Stmt::Assignment { value, .. } = assignment {
            assert_eq!(*value, Expr::Ident("blue".into()));
        } else {
            panic!("Expected Assignment");
        }
    }

    #[test]
    fn insert_new_property() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)"#);
        let edit = SourceEdit::InsertProperty {
            actor: "btn".into(),
            property: "color".into(),
            value: Expr::Ident("blue".into()),
        };
        assert!(apply_edit(&mut stmts, edit).is_ok());

        let actor = find_actor_decl_mut(&mut stmts, "btn").expect("actor 'btn' should exist");
        let prop = find_prop_mut(actor, "color").expect("property 'color' should exist");
        assert_eq!(prop.value, Expr::Ident("blue".into()));
    }

    #[test]
    fn insert_property_returns_false_if_already_exists() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)"#);
        let edit = SourceEdit::InsertProperty {
            actor: "btn".into(),
            property: "size".into(),
            value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(50.0)]),
        };
        // Should fail because "size" already exists
        assert!(apply_edit(&mut stmts, edit).is_err());
    }

    #[test]
    fn insert_actor_top_level() {
        let mut stmts = parse(r#"btn: Rect, size: (100, 200)"#);
        let edit = SourceEdit::InsertActor {
            ty: "Ellipse".into(),
            label: "circle1".into(),
            props: vec![
                Property {
                    name: "at".into(),
                    value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(50.0)]),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            container: None,
            time_s: 0.0,
        };
        assert!(apply_edit(&mut stmts, edit).is_ok());

        // Should have 2 statements now
        assert_eq!(stmts.len(), 2);

        // Second statement should be the new actor
        if let Stmt::ActorDecl { label, ty, .. } = &stmts[1] {
            assert_eq!(label, "circle1");
            assert_eq!(ty, "Ellipse");
        } else {
            panic!("Expected ActorDecl at index 1");
        }
    }

    #[test]
    fn insert_actor_into_container() {
        let mut stmts = parse(r#"row1: Row, gap: 8 {
  btn: Rect, size: (100, 200)
}"#);
        let edit = SourceEdit::InsertActor {
            ty: "Ellipse".into(),
            label: "circle1".into(),
            props: vec![
                Property {
                    name: "at".into(),
                    value: Expr::Tuple(vec![Expr::Num(50.0), Expr::Num(50.0)]),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            container: Some("row1".into()),
            time_s: 0.0,
        };
        assert!(apply_edit(&mut stmts, edit).is_ok());

        // Find the container and verify it has the new child
        let container = find_actor_decl_mut(&mut stmts, "row1").expect("container 'row1' should exist");
        if let Stmt::ActorDecl { children, .. } = container {
            assert_eq!(children.len(), 2);
            if let InlineItem::Labeled { label, ty, .. } = &children[1] {
                assert_eq!(label, "circle1");
                assert_eq!(ty, "Ellipse");
            } else {
                panic!("Expected Labeled child at index 1");
            }
        } else {
            panic!("Expected ActorDecl for row1");
        }
    }

    #[test]
    fn rename_actor_and_references() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#1s
btn.color = blue
btn.position = (200, 100)"#);

        let edit = SourceEdit::RenameActor {
            old_label: "btn".into(),
            new_label: "my_box".into(),
        };
        let _ = apply_edit(&mut stmts, edit);

        // Actor decl should be renamed
        let actor = find_actor_decl_mut(&mut stmts, "my_box").expect("renamed actor 'my_box' should exist");
        if let Stmt::ActorDecl { label, .. } = actor {
            assert_eq!(label, "my_box");
        } else {
            panic!("Expected ActorDecl");
        }

        // Old name should not exist
        assert!(find_actor_decl_mut(&mut stmts, "btn").is_none());

        // Assignments should be renamed (search recursively through keyframes)
        let mut found_color = false;
        let mut found_position = false;
        fn walk(stmts: &[Stmt], found_color: &mut bool, found_position: &mut bool) {
            for stmt in stmts {
                match stmt {
                    Stmt::Assignment { target, property, .. } => {
                        if target.last() == Some(&"my_box".to_string()) {
                            if property == "color" {
                                *found_color = true;
                            }
                            if property == "position" {
                                *found_position = true;
                            }
                        }
                        assert!(
                            target.last() != Some(&"btn".to_string()),
                            "Old reference 'btn' should have been renamed"
                        );
                    }
                    Stmt::Keyframe { body, .. }
                    | Stmt::RelativeKeyframe { body, .. }
                    | Stmt::Sequence { body, .. }
                    | Stmt::Stagger { body, .. }
                    | Stmt::Always { body, .. }
                    | Stmt::ComponentAction { body, .. } => {
                        walk(body, found_color, found_position);
                    }
                    Stmt::ComponentDef(ComponentDef { body, .. }, _) => {
                        walk(body, found_color, found_position);
                    }
                    Stmt::Conditional { then_branch, else_branch, .. } => {
                        walk(then_branch, found_color, found_position);
                        if let Some(else_b) = else_branch {
                            walk(else_b, found_color, found_position);
                        }
                    }
                    Stmt::ForLoop { body, .. } => {
                        walk(body, found_color, found_position);
                    }
                    Stmt::ActorDecl { children, .. } => {
                        walk_inline(children, found_color, found_position);
                    }
                    _ => {}
                }
            }
        }
        fn walk_inline(items: &[InlineItem], _found_color: &mut bool, _found_position: &mut bool) {
            for item in items {
                match item {
                    InlineItem::Labeled { children, .. } | InlineItem::Anonymous { children, .. } | InlineItem::ForLoop { body: children, .. } => {
                        walk_inline(children, _found_color, _found_position);
                    }
                    InlineItem::SlotFill { items: slot_items, .. } => {
                        walk_inline(slot_items, _found_color, _found_position);
                    }
                    _ => {}
                }
            }
        }
        walk(&stmts, &mut found_color, &mut found_position);
        assert!(found_color, "Color assignment should reference my_box");
        assert!(found_position, "Position assignment should reference my_box");
    }

    #[test]
    fn insert_snippet_at_top_level() {
        use animatix_syntax::ast::{Expr, Property, Stmt};

        let mut stmts = vec![];
        let fragment = vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "title".into(),
                array_index: None,
                ty: "Text".into(),
                props: vec![Property::new("content", Expr::Str("Hello".into()))],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];
        let result = super::insert_snippet(&mut stmts, fragment, None, None);
        assert!(result.is_ok());
        assert_eq!(stmts.len(), 1);
        if let Stmt::ActorDecl { label, .. } = &stmts[0] {
            assert_eq!(label, "title");
        } else {
            panic!("Expected ActorDecl");
        }
    }

    #[test]
    fn insert_snippet_into_keyframe() {
        use animatix_syntax::ast::{Stmt, Time};

        let mut stmts = vec![
            Stmt::Keyframe {
                time: Time::Seconds(0.0),
                body: vec![],
                span: None,
            },
        ];
        let fragment = vec![
            Stmt::Action(animatix_syntax::ast::Action {
                verb: "fade-in".into(),
                targets: vec!["title".into()],
                args: vec![],
                modifiers: vec![],
                byte_span: None,
            }, None),
        ];
        let result = super::insert_snippet(&mut stmts, fragment, Some(0.0), None);
        assert!(result.is_ok());
        if let Stmt::Keyframe { body, .. } = &stmts[0] {
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Keyframe");
        }
    }

    #[test]
    fn insert_snippet_into_container() {
        use animatix_syntax::ast::Stmt;

        let mut stmts = vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "container".into(),
                array_index: None,
                ty: "Row".into(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];
        let fragment = vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "child".into(),
                array_index: None,
                ty: "Text".into(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];
        let result = super::insert_snippet(&mut stmts, fragment, None, Some("container"));
        assert!(result.is_ok());
        if let Stmt::ActorDecl { children, .. } = &stmts[0] {
            assert_eq!(children.len(), 1);
        } else {
            panic!("Expected ActorDecl");
        }
    }
}