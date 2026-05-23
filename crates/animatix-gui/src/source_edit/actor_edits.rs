//! Edits related to actors: property changes, insertion, reordering, reparenting, and renaming.

use animatix::ast::{ComponentDef, Expr, InlineItem, Property, Stmt};

use super::apply::{find_actor_decl_mut, find_assignment_mut, find_prop_mut, walk_stmts_mut};
use super::apply::canonical_to_source;

// ---------------------------------------------------------------------------
// SetProperty
// ---------------------------------------------------------------------------

pub(super) fn set_property(stmts: &mut [Stmt], actor: &str, property: &str, value: Expr) -> bool {
    let source_prop = canonical_to_source(property);

    // 1. Try to find an ActorDecl and update its property.
    if let Some(actor_decl) = find_actor_decl_mut(stmts, actor) {
        if let Some(prop) = find_prop_mut(actor_decl, source_prop) {
            prop.value = value.clone();
            return true;
        }
    }

    // 2. Try to find an Assignment statement and update its value.
    if let Some(Stmt::Assignment { value: val, .. }) = find_assignment_mut(stmts, actor, source_prop) {
        *val = value;
        return true;
    }

    false
}

// ---------------------------------------------------------------------------
// InsertProperty
// ---------------------------------------------------------------------------

pub(super) fn insert_property(stmts: &mut [Stmt], actor: &str, property: &str, value: Expr) -> bool {
    let source_prop = canonical_to_source(property);

    if let Some(actor_decl) = find_actor_decl_mut(stmts, actor) {
        // Check if property already exists
        if find_prop_mut(actor_decl, source_prop).is_some() {
            // Already exists — fall through to update instead of insert.
            return false;
        }
        // Add new property
        if let Stmt::ActorDecl { ty, props, .. } = actor_decl {
            // Text, Math, Code types use generic props; Svg/Image use fixed schemas
            if ty == "Svg" || ty == "Image" {
                // These use fixed prop schemas; insertion not supported.
                return false;
            }
            props.push(Property {
                name: source_prop.into(),
                value,
                value_span: None,
                trailing_comment: None,
            });
            return true;
        }
    }

    false
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
) -> bool {
    if let Some(container_label) = container {
        // Insert as a child of the specified container
        if let Some(Stmt::ActorDecl { children, .. }) = find_actor_decl_mut(stmts, container_label) {
            children.push(InlineItem::Labeled {
                label: label.into(),
                ty: ty.into(),
                props: props.clone(),
                modifiers: vec![],
                children: vec![],
            });
            return true;
        }
        return false;
    }

    // Insert at top-level
    stmts.push(Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.into(),
        ty: ty.into(),
        props,
        modifiers: vec![],
        children: vec![],
        span: None,
    });
    true
}

// ---------------------------------------------------------------------------
// ReorderContainerChildren
// ---------------------------------------------------------------------------

pub(super) fn reorder_container_children(stmts: &mut [Stmt], container: &str, new_order: Vec<String>) -> bool {
    if let Some(Stmt::ActorDecl { children, .. }) = find_actor_decl_mut(stmts, container) {
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
        return true;
    }

    false
}

// ---------------------------------------------------------------------------
// Reparent
// ---------------------------------------------------------------------------

pub(super) fn reparent_actor(stmts: &mut Vec<Stmt>, actor: &str, new_parent: Option<&str>) -> bool {
    // 1. Find and extract the actor from its current location.
    let extracted = extract_inline_item(stmts, actor);
    let Some(item) = extracted else {
        // Actor not found as an inline child — try top-level Stmt.
        let idx = stmts.iter().position(|s| stmt_has_label(s, actor));
        let Some(idx) = idx else { return false; };
        let stmt = stmts.remove(idx);
        let item = stmt_to_inline_item(stmt);
        return insert_under_parent(stmts, item, new_parent);
    };

    insert_under_parent(stmts, item, new_parent)
}

fn insert_under_parent(stmts: &mut Vec<Stmt>, item: InlineItem, new_parent: Option<&str>) -> bool {
    match new_parent {
        None => {
            // Make top-level — anonymous items need a deterministic label.
            let index = stmts.len();
            stmts.push(inline_item_to_stmt(item, index));
            true
        }
        Some(parent_label) => {
            if let Some(parent_stmt) = find_actor_decl_mut(stmts, parent_label) {
                match parent_stmt {
                    Stmt::ActorDecl { ty, children, .. }
                        if is_container_type(ty) =>
                    {
                        children.push(item);
                        return true;
                    }
                    _ => {
                        // Target is not a container — wrap both in a new Group.
                        // First, extract the target actor too.
                        let target_extracted = extract_inline_item(stmts, parent_label);
                        let target_item = if let Some(target) = target_extracted {
                            target
                        } else {
                            // Target is top-level
                            let idx = stmts.iter().position(|s| stmt_has_label(s, parent_label));
                            let Some(idx) = idx else { return false; };
                            let stmt = stmts.remove(idx);
                            stmt_to_inline_item(stmt)
                        };

                        let group = InlineItem::Labeled {
                            label: format!("{}_group", parent_label),
                            ty: "Group".into(),
                            props: vec![],
                            modifiers: vec![],
                            children: vec![target_item, item],
                        };
                        stmts.push(inline_item_to_stmt(group, stmts.len()));
                        return true;
                    }
                }
            }
            false
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
        Stmt::ActorDecl { label, ty, props, modifiers, children, .. } => InlineItem::Labeled {
            label,
            ty,
            props,
            modifiers,
            children,
        },
        _ => InlineItem::Anonymous {
            ty: "Group".into(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
        },
    }
}

fn inline_item_to_stmt(item: InlineItem, index: usize) -> Stmt {
    match item {
        InlineItem::Labeled { label, ty, props, modifiers, children } => Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label,
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
        InlineItem::Labeled { children, .. } | InlineItem::Anonymous { children, .. } => {
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
    walk_stmts_mut(stmts, &mut |stmt| {
        rename_in_stmt(stmt, old_label, new_label);
    });
}

/// Rename actor references inside a single statement (non-recursive).
fn rename_in_stmt(stmt: &mut Stmt, old_label: &str, new_label: &str) {
    match stmt {
        Stmt::ActorDecl { label, children, .. } => {
            if label == old_label {
                *label = new_label.into();
            }
            rename_in_inline_items(children, old_label, new_label);
        }
        Stmt::Assignment { target, .. } => {
            if let Some(last) = target.last_mut() {
                if last == old_label {
                    *last = new_label.into();
                }
            }
        }
        Stmt::Action(action, _) => {
            for t in action.targets.iter_mut() {
                if t == old_label {
                    *t = new_label.into();
                }
            }
        }
        _ => {}
    }
}

fn rename_in_inline_items(items: &mut [InlineItem], old_label: &str, new_label: &str) {
    for item in items.iter_mut() {
        match item {
            InlineItem::Labeled { label, children, .. } => {
                if label == old_label {
                    *label = new_label.into();
                }
                rename_in_inline_items(children, old_label, new_label);
            }
            InlineItem::Anonymous { children, .. } => {
                rename_in_inline_items(children, old_label, new_label);
            }
            InlineItem::SlotFill { items: slot_items, .. } => {
                rename_in_inline_items(slot_items, old_label, new_label);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::apply::{find_actor_decl_mut, find_assignment_mut, find_prop_mut};
    use super::super::apply::{SourceEdit, apply_edit};
    use animatix::ast::{ComponentDef, Expr, InlineItem, Property, Stmt};
    use animatix::parser::parser;
    use chumsky::Parser;

    fn parse(source: &str) -> Vec<Stmt> {
        parser().parse(source).into_result().expect("failed to parse test source")
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
        assert!(apply_edit(&mut stmts, edit));

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
        assert!(apply_edit(&mut stmts, edit));

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
        assert!(apply_edit(&mut stmts, edit));

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
        assert!(apply_edit(&mut stmts, edit));

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
        assert!(!apply_edit(&mut stmts, edit));
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
        assert!(apply_edit(&mut stmts, edit));

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
        assert!(apply_edit(&mut stmts, edit));

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
        apply_edit(&mut stmts, edit);

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
        fn walk_inline(items: &[InlineItem], found_color: &mut bool, found_position: &mut bool) {
            for item in items {
                match item {
                    InlineItem::Labeled { children, .. } | InlineItem::Anonymous { children, .. } => {
                        walk_inline(children, found_color, found_position);
                    }
                    InlineItem::SlotFill { items: slot_items, .. } => {
                        walk_inline(slot_items, found_color, found_position);
                    }
                    _ => {}
                }
            }
        }
        walk(&stmts, &mut found_color, &mut found_position);
        assert!(found_color, "Color assignment should reference my_box");
        assert!(found_position, "Position assignment should reference my_box");
    }
}