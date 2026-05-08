//! AST-based source editing for the GUI inspector.
//!
//! Replaces the old byte-span surgery model (`source_edit.rs`) with semantic
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
    };

    let keyframe = Stmt::RelativeKeyframe {
        offset,
        body: vec![assignment],
        span: None,
    };

    // Insert after the keyframe that contains prev_time_s, or at the end.
    let insert_idx = find_keyframe_insertion_point(stmts, prev_time_s);
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
            | Stmt::Sequence { body }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body }
            | Stmt::LabeledAlways { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. })
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
            | Stmt::Sequence { body }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body }
            | Stmt::LabeledAlways { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. })
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
}
