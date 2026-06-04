//! AST-based source editing — core types, dispatch, and shared traversal helpers.
//!
//! Replaces the old byte-span surgery model with semantic
//! edits applied directly to the AST. After mutation, the entire AST is
//! re-serialized via [`animatix_syntax::to_source::stmts_to_source`].

use animatix_syntax::ast::{ComponentDef, Expr, Modifier, Property, Stmt, Time, Transition};

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
    /// Update a property value inside an existing keyframe.
    MergeKeyframe {
        actor: String,
        property: String,
        value: Expr,
        /// Absolute time of the keyframe to update (in seconds).
        time_s: f64,
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
    /// Reorder top-level scene declarations.
    ReorderScenes {
        new_order: Vec<String>,
    },
    /// Set or remove the play target for a scene.
    SetPlayTarget {
        scene: String,
        target: Option<String>,
    },
    /// Update the transition on a scene's play statement.
    SetTransition {
        from_scene: String,
        transition: Option<Transition>,
    },
    /// Set explicit duration for a scene (in seconds). Pass None to remove.
    SetSceneDuration {
        scene: String,
        duration_s: Option<f64>,
    },
    /// Rename a scene and update all play references.
    RenameScene {
        old_name: String,
        new_name: String,
    },
    /// Add a new empty scene declaration.
    AddScene {
        name: String,
    },
    /// Delete a scene declaration and all play references to it.
    DeleteScene {
        name: String,
    },
    /// Duplicate a scene declaration with renamed actors.
    DuplicateScene {
        name: String,
    },
    /// Update the easing of an existing keyframe's assignment.
    SetKeyframeEasing {
        actor: String,
        property: String,
        /// Absolute time in seconds.
        time_s: f64,
        easing: animatix_syntax::easing::Easing,
    },
    /// Delete a keyframe at a specific time.
    DeleteKeyframe {
        actor: String,
        property: String,
        /// Absolute time in seconds.
        time_s: f64,
    },
    /// Move an actor to a new parent container (or to top-level if None).
    /// If the target is not a container, both are wrapped in a new Group.
    Reparent {
        actor: String,
        new_parent: Option<String>,
    },
    /// Extract selected actors into a new scene.
    ExtractScene {
        actor_labels: Vec<String>,
        new_scene_name: String,
    },
    /// Move selected actors to an existing scene.
    MoveToScene {
        actor_labels: Vec<String>,
        target_scene: String,
    },
    /// Move a keyframe to a new time.
    MoveKeyframeTime {
        actor: String,
        property: String,
        old_time_s: f64,
        new_time_s: f64,
    },
    /// Insert an action statement at the exact keyframe for `time_s`.
    InsertAction {
        verb: String,
        targets: Vec<String>,
        args: Vec<Expr>,
        modifiers: Vec<Modifier>,
        time_s: f64,
    },
    /// Set or update a config property value.
    SetConfigProperty {
        key: String,
        value: Expr,
    },
    /// Insert an import statement at the top of the file.
    InsertImport {
        path: String,
    },
}

/// Apply a semantic edit to a statement list.
///
/// Returns `Ok(())` if the edit was applied successfully, or a structured
/// [`SourceEditError`] describing why it failed.
pub fn apply_edit(stmts: &mut Vec<Stmt>, edit: SourceEdit) -> Result<(), super::SourceEditError> {
    match edit {
        SourceEdit::SetProperty { actor, property, value } => {
            super::actor_edits::set_property(stmts, &actor, &property, value)
        }
        SourceEdit::InsertProperty { actor, property, value } => {
            super::actor_edits::insert_property(stmts, &actor, &property, value)
        }
        SourceEdit::InsertKeyframe {
            actor,
            property,
            value,
            time_s,
            prev_time_s,
        } => super::keyframe_edits::insert_keyframe(stmts, &actor, &property, value, time_s, prev_time_s),
        SourceEdit::MergeKeyframe {
            actor,
            property,
            value,
            time_s,
        } => super::keyframe_edits::merge_keyframe(stmts, &actor, &property, value, time_s),
        SourceEdit::ReorderContainerChildren { container, new_order } => {
            super::actor_edits::reorder_container_children(stmts, &container, new_order)
        }
        SourceEdit::InsertActor {
            ty,
            label,
            props,
            container,
            time_s,
        } => super::actor_edits::insert_actor(stmts, &ty, &label, props, container.as_deref(), time_s),
        SourceEdit::RenameActor {
            old_label,
            new_label,
        } => {
            super::actor_edits::rename_all_references(stmts, &old_label, &new_label);
            Ok(())
        }
        SourceEdit::ReorderScenes { new_order } => super::scene_edits::reorder_scenes(stmts, new_order),
        SourceEdit::SetPlayTarget { scene, target } => {
            super::scene_edits::set_play_target(stmts, &scene, target.as_deref())
        }
        SourceEdit::SetTransition { from_scene, transition } => {
            super::scene_edits::set_transition(stmts, &from_scene, transition)
        }
        SourceEdit::SetSceneDuration { scene, duration_s } => {
            super::scene_edits::set_scene_duration(stmts, &scene, duration_s)
        }
        SourceEdit::RenameScene { old_name, new_name } => {
            super::scene_edits::rename_scene(stmts, &old_name, &new_name)
        }
        SourceEdit::AddScene { name } => super::scene_edits::add_scene(stmts, &name),
        SourceEdit::DeleteScene { name } => super::scene_edits::delete_scene(stmts, &name),
        SourceEdit::DuplicateScene { name } => super::scene_edits::duplicate_scene(stmts, &name),
        SourceEdit::SetKeyframeEasing { actor, property, time_s, easing } => {
            super::keyframe_edits::set_keyframe_easing(stmts, &actor, &property, time_s, easing)
        }
        SourceEdit::DeleteKeyframe { actor, property, time_s } => {
            super::keyframe_edits::delete_keyframe(stmts, &actor, &property, time_s)
        }
        SourceEdit::Reparent { actor, new_parent } => {
            super::actor_edits::reparent_actor(stmts, &actor, new_parent.as_deref())
        }
        SourceEdit::ExtractScene {
            actor_labels,
            new_scene_name,
        } => super::scene_edits::extract_scene(stmts, actor_labels, &new_scene_name),
        SourceEdit::MoveToScene {
            actor_labels,
            target_scene,
        } => super::scene_edits::move_to_scene(stmts, actor_labels, &target_scene),
        SourceEdit::MoveKeyframeTime { actor, property, old_time_s, new_time_s } => {
            super::keyframe_edits::move_keyframe_time(stmts, &actor, &property, old_time_s, new_time_s)
        }
        SourceEdit::InsertAction { verb, targets, args, modifiers, time_s } => {
            super::action_edits::insert_action(stmts, &verb, &targets, &args, &modifiers, time_s)
        }
        SourceEdit::SetConfigProperty { key, value } => {
            super::config_edits::set_config_property(stmts, &key, value)
        }
        SourceEdit::InsertImport { path } => {
            super::config_edits::insert_import(stmts, &path)
        }
    }
}

// ---------------------------------------------------------------------------
// Generic AST traversal
// ---------------------------------------------------------------------------

/// Depth-first mutable visitor over every statement.
pub(super) fn walk_stmts_mut(stmts: &mut [Stmt], visitor: &mut dyn FnMut(&mut Stmt)) {
    for stmt in stmts.iter_mut() {
        visitor(stmt);
        match stmt {
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. }
            | Stmt::Scene { body, .. } => {
                walk_stmts_mut(body, visitor);
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                walk_stmts_mut(then_branch, visitor);
                if let Some(else_b) = else_branch {
                    walk_stmts_mut(else_b, visitor);
                }
            }
            Stmt::ForLoop { body, .. } => {
                walk_stmts_mut(body, visitor);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// AST traversal helpers
// ---------------------------------------------------------------------------

/// Find an ActorDecl with the given label anywhere in the statement tree.
pub fn find_actor_decl<'a>(stmts: &'a [Stmt], label: &str) -> Option<&'a Stmt> {
    for stmt in stmts.iter() {
        match stmt {
            Stmt::ActorDecl { label: l, .. } if l == label => return Some(stmt),
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                if let Some(found) = find_actor_decl(body, label) {
                    return Some(found);
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                if let Some(found) = find_actor_decl(then_branch, label) {
                    return Some(found);
                }
                if let Some(else_b) = else_branch {
                    if let Some(found) = find_actor_decl(else_b, label) {
                        return Some(found);
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                if let Some(found) = find_actor_decl(body, label) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

pub(super) fn find_actor_decl_mut<'a>(stmts: &'a mut [Stmt], label: &str) -> Option<&'a mut Stmt> {
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::ActorDecl { label: l, .. } if l == label => return Some(stmt),
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
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
            _ => {}
        }
    }
    None
}

/// Find a mutable reference to an Assignment statement for the given actor
/// and property anywhere in the statement tree.
pub(super) fn find_assignment_mut<'a>(
    stmts: &'a mut [Stmt],
    actor: &str,
    property: &str,
) -> Option<&'a mut Stmt> {
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::Assignment { target, property: prop, .. }
                if target.last().is_some_and(|t| t == actor) && prop == property =>
            {
                return Some(stmt);
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
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

/// Find a scene declaration by name.
pub(super) fn find_scene_mut<'a>(stmts: &'a mut [Stmt], name: &str) -> Option<&'a mut Stmt> {
    stmts.iter_mut().find(|stmt| matches!(stmt, Stmt::Scene { name: scene_name, .. } if scene_name == name))
}

pub(super) fn time_to_seconds(t: &Time) -> f64 {
    match t {
        Time::Seconds(s) => *s,
        Time::Milliseconds(ms) => *ms as f64 / 1000.0,
    }
}

/// Find a property by name inside an actor-like statement.
pub(super) fn find_prop_mut<'a>(stmt: &'a mut Stmt, name: &str) -> Option<&'a mut Property> {
    let props: &mut Vec<Property> = match stmt {
        Stmt::ActorDecl { props, .. } => props,
        _ => return None,
    };
    props.iter_mut().find(|p| p.name == name)
}