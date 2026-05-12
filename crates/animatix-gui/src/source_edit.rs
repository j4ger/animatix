//! AST-based source editing for the GUI inspector.
//!
//! Replaces the old byte-span surgery model with semantic
//! edits applied directly to the AST. After mutation, the entire AST is
//! re-serialized via [`animatix::to_source::stmts_to_source`].

use animatix::ast::{ComponentDef, Expr, InlineItem, Property, Stmt, Time};

// ---------------------------------------------------------------------------
// Property name mapping
// ---------------------------------------------------------------------------

/// Convert a canonical inspector property name to its source equivalent.
pub fn canonical_to_source(name: &str) -> &str {
    match name {
        "position" => "at",
        other => other,
    }
}

/// Convert a source property name to its canonical inspector equivalent.
pub fn source_to_canonical(name: &str) -> &str {
    match name {
        "at" => "position",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Edit operations
// ---------------------------------------------------------------------------

/// A semantic edit description.
#[derive(Debug, Clone)]
pub enum SourceEdit {
    /// Update an existing property value on an actor declaration or assignment.
    SetProperty {
        actor: String,
        property: String,
        value: Expr,
    },
    /// Add a new property to an actor declaration.
    InsertProperty {
        actor: String,
        property: String,
        value: Expr,
    },
    /// Insert a keyframe block with an assignment.
    InsertKeyframe {
        actor: String,
        property: String,
        value: Expr,
        /// Absolute time in seconds.
        time_s: f64,
        /// Time of the previous keyframe (for relative offset calculation).
        prev_time_s: f64,
    },
    /// Reorder a container's inline children by label.
    ReorderContainerChildren {
        container: String,
        new_order: Vec<String>,
    },
    /// Insert a new actor declaration into the scene.
    InsertActor {
        ty: String,
        label: String,
        props: Vec<Property>,
        container: Option<String>,
        time_s: f64,
    },
    /// Rename an actor (and all references to it) throughout the AST.
    RenameActor {
        old_label: String,
        new_label: String,
    },
}

/// Apply a semantic edit to a statement list.
///
/// Returns `true` if the edit was applied successfully.
pub fn apply_edit(stmts: &mut Vec<Stmt>, edit: SourceEdit) -> bool {
    match edit {
        SourceEdit::SetProperty { actor, property, value } => {
            set_property(stmts, &actor, &property, value)
        }
        SourceEdit::InsertProperty { actor, property, value } => {
            insert_property(stmts, &actor, &property, value)
        }
        SourceEdit::InsertKeyframe {
            actor,
            property,
            value,
            time_s,
            prev_time_s,
        } => insert_keyframe(stmts, &actor, &property, value, time_s, prev_time_s),
        SourceEdit::ReorderContainerChildren { container, new_order } => {
            reorder_container_children(stmts, &container, new_order)
        }
        SourceEdit::InsertActor {
            ty,
            label,
            props,
            container,
            time_s,
        } => insert_actor(stmts, &ty, &label, props, container.as_deref(), time_s),
        SourceEdit::RenameActor {
            old_label,
            new_label,
        } => {
            rename_all_references(stmts, &old_label, &new_label);
            true
        }
    }
}

// ---------------------------------------------------------------------------
// SetProperty
// ---------------------------------------------------------------------------

fn set_property(stmts: &mut [Stmt], actor: &str, property: &str, value: Expr) -> bool {
    let source_prop = canonical_to_source(property);

    // 1. Try to find an ActorDecl and update its property.
    if let Some(actor_decl) = find_actor_decl_mut(stmts, actor) {
        if let Some(prop) = find_prop_mut(actor_decl, source_prop) {
            prop.value = value.clone();
            return true;
        }
    }

    // 2. Try to find an Assignment statement and update its value.
    if let Some(assignment) = find_assignment_mut(stmts, actor, source_prop) {
        if let Stmt::Assignment { value: val, .. } = assignment {
            *val = value;
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// InsertProperty
// ---------------------------------------------------------------------------

fn insert_property(stmts: &mut [Stmt], actor: &str, property: &str, value: Expr) -> bool {
    let source_prop = canonical_to_source(property);

    if let Some(actor_decl) = find_actor_decl_mut(stmts, actor) {
        // Check if property already exists
        if find_prop_mut(actor_decl, source_prop).is_some() {
            // Already exists — fall through to update instead of insert.
            return false;
        }
        // Add new property
        match actor_decl {
            Stmt::ActorDecl { props, .. }
            | Stmt::Text { props, .. }
            | Stmt::Math { props, .. }
            | Stmt::Code { props, .. } => {
                props.push(Property {
                    name: source_prop.into(),
                    value,
                    value_span: None,
                trailing_comment: None,
                });
                return true;
            }
            Stmt::Svg { .. } | Stmt::Image { .. } => {
                // These use fixed prop schemas; insertion not supported.
                return false;
            }
            _ => {}
        }
    }

    false
}

// ---------------------------------------------------------------------------
// InsertKeyframe
// ---------------------------------------------------------------------------

fn insert_keyframe(
    stmts: &mut Vec<Stmt>,
    actor: &str,
    property: &str,
    value: Expr,
    time_s: f64,
    prev_time_s: f64,
) -> bool {
    let delta_s = time_s - prev_time_s;
    if delta_s < 0.001 {
        return false;
    }

    let source_prop = canonical_to_source(property);

    // Format the time offset.
    let offset = if delta_s < 1.0 {
        Time::Milliseconds((delta_s * 1000.0).round() as u64)
    } else if delta_s == delta_s.floor() {
        Time::Seconds(delta_s)
    } else {
        Time::Seconds(delta_s)
    };

    let assignment = Stmt::Assignment {
        target: vec![actor.into()],
        property: source_prop.into(),
        value,
        modifiers: vec![],
        value_span: None,
        span: None,
    };

    let keyframe = Stmt::RelativeKeyframe {
        offset,
        body: vec![assignment],
        span: None,
    };

    // Insert after the keyframe that contains prev_time_s, or at the end.
    let mut insert_idx = find_keyframe_insertion_point(stmts, prev_time_s);

    // If there are no keyframes before the insertion point and prev_time_s is ~0,
    // wrap any leading top-level declarations in a #0s keyframe so they don't
    // get shifted to a later time by the new relative keyframe.
    if insert_idx == 0 && prev_time_s < 0.001 && !stmts.is_empty() {
        let first_is_keyframe = matches!(
            stmts[0],
            Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. }
        );
        if !first_is_keyframe {
            let decl_end = stmts
                .iter()
                .position(|s| matches!(s, Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. }))
                .unwrap_or(stmts.len());
            if decl_end > 0 {
                let decls: Vec<Stmt> = stmts.drain(0..decl_end).collect();
                let zero_kf = Stmt::Keyframe {
                    time: Time::Seconds(0.0),
                    body: decls,
                    span: None,
                };
                stmts.insert(0, zero_kf);
                insert_idx = 1;
            }
        }
    }

    // If the next statement is a RelativeKeyframe, subtract delta_s from its
    // offset so that subsequent keyframes keep their original absolute times.
    if insert_idx < stmts.len() {
        if let Stmt::RelativeKeyframe { offset: ref mut next_offset, .. } = stmts[insert_idx] {
            let next_delta_s = time_to_seconds(next_offset);
            let new_next_delta_s = next_delta_s - delta_s;
            if new_next_delta_s >= 0.001 {
                *next_offset = if new_next_delta_s < 1.0 {
                    Time::Milliseconds((new_next_delta_s * 1000.0).round() as u64)
                } else if new_next_delta_s == new_next_delta_s.floor() {
                    Time::Seconds(new_next_delta_s)
                } else {
                    Time::Seconds(new_next_delta_s)
                };
            }
        }
    }

    stmts.insert(insert_idx, keyframe);
    true
}

/// Find the index after which a new keyframe at `time_s` should be inserted.
fn find_keyframe_insertion_point(stmts: &[Stmt], time_s: f64) -> usize {
    let mut last_kf_idx = 0usize;
    let mut current_time = 0.0f64;

    for (i, stmt) in stmts.iter().enumerate() {
        match stmt {
            Stmt::Keyframe { time, .. } => {
                current_time = time_to_seconds(time);
                if current_time <= time_s {
                    last_kf_idx = i + 1;
                }
            }
            Stmt::RelativeKeyframe { offset, .. } => {
                current_time += time_to_seconds(offset);
                if current_time <= time_s {
                    last_kf_idx = i + 1;
                }
            }
            _ => {}
        }
    }

    last_kf_idx
}

fn time_to_seconds(t: &Time) -> f64 {
    match t {
        Time::Seconds(s) => *s,
        Time::Milliseconds(ms) => *ms as f64 / 1000.0,
    }
}

fn insert_actor(
    stmts: &mut Vec<Stmt>,
    ty: &str,
    label: &str,
    props: Vec<Property>,
    container: Option<&str>,
    _time_s: f64,
) -> bool {
    if let Some(container_label) = container {
        // Insert as a child of the specified container
        if let Some(actor_decl) = find_actor_decl_mut(stmts, container_label) {
            if let Stmt::ActorDecl { children, .. } = actor_decl {
                children.push(InlineItem::Labeled {
                    label: label.into(),
                    ty: ty.into(),
                    props: props.clone(),
                    modifiers: vec![],
                    children: vec![],
                });
                return true;
            }
        }
        return false;
    }

    // Insert at top-level
    stmts.push(Stmt::ActorDecl {
        is_pub: false,
        label: label.into(),
        ty: ty.into(),
        props,
        modifiers: vec![],
        children: vec![],
        span: None,
    });
    true
}

fn reorder_container_children(stmts: &mut [Stmt], container: &str, new_order: Vec<String>) -> bool {
    if let Some(actor_decl) = find_actor_decl_mut(stmts, container) {
        if let Stmt::ActorDecl { children, .. } = actor_decl {
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
    }

    false
}

// ---------------------------------------------------------------------------
// AST traversal helpers
// ---------------------------------------------------------------------------

/// Find a mutable reference to an ActorDecl (or Text/Math/Code/Svg/Image) with
/// the given label anywhere in the statement tree.
fn find_actor_decl_mut<'a>(stmts: &'a mut [Stmt], label: &str) -> Option<&'a mut Stmt> {
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::ActorDecl { label: l, .. } if l == label => return Some(stmt),
            Stmt::Text { label: Some(l), .. } if l == label => return Some(stmt),
            Stmt::Math { label: Some(l), .. } if l == label => return Some(stmt),
            Stmt::Code { label: Some(l), .. } if l == label => return Some(stmt),
            Stmt::Svg { label: Some(l), .. } if l == label => return Some(stmt),
            Stmt::Image { label: Some(l), .. } if l == label => return Some(stmt),
            // Recurse into containers
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::LabeledAlways { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                if let Some(found) = find_actor_decl_mut(body, label) {
                    return Some(found);
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                if let Some(found) = find_actor_decl_mut(then_branch, label) {
                    return Some(found);
                }
                if let Some(else_b) = else_branch {
                    if let Some(found) = find_actor_decl_mut(else_b, label) {
                        return Some(found);
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                if let Some(found) = find_actor_decl_mut(body, label) {
                    return Some(found);
                }
            }
            Stmt::ActorDecl { children, .. } => {
                if let Some(found) = find_inline_item_mut(children, label) {
                    // We found the inline item but need to return it as a Stmt.
                    // Inline items are not Stmts, so we can't return them here.
                    // This is a limitation — nested children aren't editable via
                    // this path. For now, we only support top-level actors.
                    let _ = found;
                }
            }
            _ => {}
        }
    }
    None
}

/// Find a mutable reference to an Assignment statement for the given actor
/// and property anywhere in the statement tree.
fn find_assignment_mut<'a>(
    stmts: &'a mut [Stmt],
    actor: &str,
    property: &str,
) -> Option<&'a mut Stmt> {
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::Assignment { target, property: prop, .. }
                if target.last().map_or(false, |t| t == actor) && prop == property =>
            {
                return Some(stmt);
            }
            // Recurse
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::LabeledAlways { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                if let Some(found) = find_assignment_mut(body, actor, property) {
                    return Some(found);
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                if let Some(found) = find_assignment_mut(then_branch, actor, property) {
                    return Some(found);
                }
                if let Some(else_b) = else_branch {
                    if let Some(found) = find_assignment_mut(else_b, actor, property) {
                        return Some(found);
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                if let Some(found) = find_assignment_mut(body, actor, property) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// Find a property by name inside an actor-like statement.
fn find_prop_mut<'a>(stmt: &'a mut Stmt, name: &str) -> Option<&'a mut Property> {
    let props: &mut Vec<Property> = match stmt {
        Stmt::ActorDecl { props, .. }
        | Stmt::Text { props, .. }
        | Stmt::Math { props, .. }
        | Stmt::Code { props, .. } => props,
        _ => return None,
    };
    props.iter_mut().find(|p| p.name == name)
}

/// Recursively search for a labeled InlineItem. (Used for finding nested actors.)
fn find_inline_item_mut<'a>(
    items: &'a mut [InlineItem],
    label: &str,
) -> Option<&'a mut InlineItem> {
    for item in items.iter_mut() {
        match item {
            InlineItem::Labeled { label: l, children, .. } if l == label => return Some(item),
            InlineItem::Labeled { children, .. } | InlineItem::Anonymous { children, .. } => {
                if let Some(found) = find_inline_item_mut(children, label) {
                    return Some(found);
                }
            }
            InlineItem::SlotFill { items: slot_items, .. } => {
                if let Some(found) = find_inline_item_mut(slot_items, label) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Rename
// ---------------------------------------------------------------------------

/// Rename all references to `old_label` into `new_label` throughout the AST.
fn rename_all_references(stmts: &mut [Stmt], old_label: &str, new_label: &str) {
    for stmt in stmts.iter_mut() {
        rename_in_stmt(stmt, old_label, new_label);
    }
}

fn rename_in_stmt(stmt: &mut Stmt, old_label: &str, new_label: &str) {
    match stmt {
        // Actor declarations
        Stmt::ActorDecl { label, children, .. } => {
            if label == old_label {
                *label = new_label.into();
            }
            rename_in_inline_items(children, old_label, new_label);
        }
        Stmt::Text { label, .. }
        | Stmt::Math { label, .. }
        | Stmt::Code { label, .. }
        | Stmt::Svg { label, .. }
        | Stmt::Image { label, .. } => {
            if let Some(l) = label {
                if l == old_label {
                    *l = new_label.into();
                }
            }
        }
        // Assignments: target = [..., "actor"] ; property = "prop"
        Stmt::Assignment { target, .. } => {
            if let Some(last) = target.last_mut() {
                if last == old_label {
                    *last = new_label.into();
                }
            }
        }
        // Action: action.verb targets
        Stmt::Action(action, _) => {
            for t in action.targets.iter_mut() {
                if t == old_label {
                    *t = new_label.into();
                }
            }
        }
        // Containers / bodies
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::LabeledAlways { body, .. }
        | Stmt::ComponentDef(ComponentDef { body, .. }, _)
        | Stmt::ComponentAction { body, .. } => {
            rename_all_references(body, old_label, new_label);
        }
        Stmt::Conditional { then_branch, else_branch, .. } => {
            rename_all_references(then_branch, old_label, new_label);
            if let Some(else_b) = else_branch {
                rename_all_references(else_b, old_label, new_label);
            }
        }
        Stmt::ForLoop { body, .. } => {
            rename_all_references(body, old_label, new_label);
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
    use super::*;
    use animatix::ast::{Expr, Stmt, Time};
    use animatix::parser::parser;
    use chumsky::Parser;

    fn parse(source: &str) -> Vec<Stmt> {
        parser().parse(source).unwrap()
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

        let actor = find_actor_decl_mut(&mut stmts, "btn").unwrap();
        let prop = find_prop_mut(actor, "color").unwrap();
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

        let actor = find_actor_decl_mut(&mut stmts, "btn").unwrap();
        let prop = find_prop_mut(actor, "at").unwrap();
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
        ).unwrap();
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

        let actor = find_actor_decl_mut(&mut stmts, "btn").unwrap();
        let prop = find_prop_mut(actor, "color").unwrap();
        assert_eq!(prop.value, Expr::Ident("blue".into()));
    }

    #[test]
    fn insert_keyframe_block() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#2s
btn.color = red"#);
        let edit = SourceEdit::InsertKeyframe {
            actor: "btn".into(),
            property: "color".into(),
            value: Expr::Ident("blue".into()),
            time_s: 3.0,
            prev_time_s: 2.0,
        };
        assert!(apply_edit(&mut stmts, edit));

        // Should have 3 top-level keyframes now
        assert_eq!(stmts.len(), 3);

        // The new keyframe should be a RelativeKeyframe after the #2s one
        if let Stmt::RelativeKeyframe { offset, body, .. } = &stmts[2] {
            assert_eq!(*offset, Time::Seconds(1.0));
            assert_eq!(body.len(), 1);
            if let Stmt::Assignment { target, property, .. } = &body[0] {
                assert_eq!(target, &vec!["btn".to_string()]);
                assert_eq!(property, "color");
            } else {
                panic!("Expected Assignment");
            }
        } else {
            panic!("Expected RelativeKeyframe");
        }
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
    fn insert_keyframe_wraps_declarations_in_zero_keyframe() {
        // No keyframes at all — inserting a relative keyframe must wrap the
        // top-level declarations in #0s so they don't get shifted.
        let mut stmts = parse(r#"btn: Rect, size: (100, 200)
circle: Circle, radius: 50"#);

        let edit = SourceEdit::InsertKeyframe {
            actor: "btn".into(),
            property: "position".into(),
            value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(0.0)]),
            time_s: 0.5,
            prev_time_s: 0.0,
        };
        assert!(apply_edit(&mut stmts, edit));

        // Should now be: #0s, #0s, #+500ms (parser wraps each top-level decl in #0s)
        assert_eq!(stmts.len(), 3);

        // First two statements are #0s wrapping each declaration (parser behavior)
        if let Stmt::Keyframe { time, body, .. } = &stmts[0] {
            assert_eq!(*time, Time::Seconds(0.0));
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Keyframe at index 0, got {:?}", stmts[0]);
        }
        if let Stmt::Keyframe { time, body, .. } = &stmts[1] {
            assert_eq!(*time, Time::Seconds(0.0));
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected Keyframe at index 1, got {:?}", stmts[1]);
        }

        // Third statement is the new relative keyframe
        if let Stmt::RelativeKeyframe { offset, body, .. } = &stmts[2] {
            assert_eq!(*offset, Time::Milliseconds(500));
            assert_eq!(body.len(), 1);
        } else {
            panic!("Expected RelativeKeyframe at index 2");
        }
    }

    #[test]
    fn insert_keyframe_adjusts_subsequent_relative_offset() {
        // Inserting between #0s and #+1s should adjust the #+1s offset to #+500ms
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#+1s
btn.color = red"#);

        let edit = SourceEdit::InsertKeyframe {
            actor: "btn".into(),
            property: "position".into(),
            value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(0.0)]),
            time_s: 0.5,
            prev_time_s: 0.0,
        };
        assert!(apply_edit(&mut stmts, edit));

        // Should have 3 top-level statements
        assert_eq!(stmts.len(), 3);

        // New keyframe at index 1
        if let Stmt::RelativeKeyframe { offset, .. } = &stmts[1] {
            assert_eq!(*offset, Time::Milliseconds(500));
        } else {
            panic!("Expected RelativeKeyframe at index 1");
        }

        // Existing keyframe at index 2 — offset should be reduced
        if let Stmt::RelativeKeyframe { offset, .. } = &stmts[2] {
            assert_eq!(*offset, Time::Milliseconds(500));
        } else {
            panic!("Expected RelativeKeyframe at index 2");
        }
    }

    #[test]
    fn insert_actor_top_level() {
        let mut stmts = parse(r#"btn: Rect, size: (100, 200)"#);
        let edit = SourceEdit::InsertActor {
            ty: "Circle".into(),
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
            assert_eq!(ty, "Circle");
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
            ty: "Circle".into(),
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
        let container = find_actor_decl_mut(&mut stmts, "row1").unwrap();
        if let Stmt::ActorDecl { children, .. } = container {
            assert_eq!(children.len(), 2);
            if let InlineItem::Labeled { label, ty, .. } = &children[1] {
                assert_eq!(label, "circle1");
                assert_eq!(ty, "Circle");
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
        let actor = find_actor_decl_mut(&mut stmts, "my_box").unwrap();
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
                    | Stmt::LabeledAlways { body, .. }
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
