//! AST-based source editing — core types, dispatch, and shared traversal helpers.
//!
//! Replaces the old byte-span surgery model with semantic
//! edits applied directly to the AST. After mutation, the entire AST is
//! re-serialized via [`animatix_syntax::to_source::stmts_to_source`].

use animatix_syntax::ast::{Expr, Modifier, Property, Stmt, Transition};

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
        /// `Some(name)` scopes the edit to a composition scene.
        scene: Option<String>,
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
        /// `Some(name)` scopes the edit to a composition scene.
        scene: Option<String>,
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
    /// Delete an actor declaration by label.
    DeleteActor { label: String },
    /// Duplicate an actor declaration with a new label.
    DuplicateActor {
        original_label: String,
        new_label: String,
    },
    /// Paste actor declarations and their keyframed assignments.
    PasteActors {
        /// Pairs of (original label, new label) already resolved for uniqueness.
        clipboard: Vec<(String, String)>,
        /// Time offset applied to absolute keyframes.
        time_s: f64,
    },
    /// Remove a property from an actor declaration. Does not remove keyframed assignments.
    RemoveProperty { actor: String, property: String },
    /// Reorder top-level scene declarations.
    ReorderScenes { new_order: Vec<String> },
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
    RenameScene { old_name: String, new_name: String },
    /// Add a new empty scene declaration.
    AddScene { name: String },
    /// Delete a scene declaration and all play references to it.
    DeleteScene { name: String },
    /// Duplicate a scene declaration with renamed actors.
    DuplicateScene { name: String },
    /// Update the easing of an existing keyframe's assignment.
    SetKeyframeEasing {
        /// `Some(name)` scopes the edit to a composition scene.
        scene: Option<String>,
        actor: String,
        property: String,
        /// Absolute time in seconds.
        time_s: f64,
        easing: animatix_syntax::easing::Easing,
    },
    /// Delete a keyframe at a specific time.
    DeleteKeyframe {
        /// `Some(name)` scopes the edit to a composition scene.
        scene: Option<String>,
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
        /// `Some(name)` scopes the edit to a composition scene.
        scene: Option<String>,
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
    SetConfigProperty { key: String, value: Expr },
    /// Resize an action block's duration (and optionally move its start time).
    ResizeAction {
        verb: String,
        targets: Vec<String>,
        old_start_s: f64,
        new_start_s: f64,
        new_duration_s: f64,
    },
    /// Insert a parsed snippet (AST fragment) at the appropriate location.
    InsertSnippet {
        /// The parsed AST fragment to insert.
        stmts: Vec<Stmt>,
        /// Optional target time for insertion inside a keyframe.
        time_s: Option<f64>,
        /// Optional container label for insertion as children.
        container: Option<String>,
    },
}

/// Apply a semantic edit to a statement list.
///
/// Returns `Ok(())` if the edit was applied successfully, or a structured
/// [`SourceEditError`] describing why it failed.
pub fn apply_edit(stmts: &mut Vec<Stmt>, edit: SourceEdit) -> Result<(), super::SourceEditError> {
    match edit {
        SourceEdit::SetProperty {
            actor,
            property,
            value,
        } => super::actor_edits::set_property(stmts, &actor, &property, value),
        SourceEdit::InsertProperty {
            actor,
            property,
            value,
        } => super::actor_edits::insert_property(stmts, &actor, &property, value),
        SourceEdit::InsertKeyframe {
            scene,
            actor,
            property,
            value,
            time_s,
            prev_time_s,
        } => super::keyframe_edits::insert_keyframe(
            stmts,
            scene.as_deref(),
            &actor,
            &property,
            value,
            time_s,
            prev_time_s,
        ),
        SourceEdit::MergeKeyframe {
            scene,
            actor,
            property,
            value,
            time_s,
        } => super::keyframe_edits::merge_keyframe(
            stmts,
            scene.as_deref(),
            &actor,
            &property,
            value,
            time_s,
        ),
        SourceEdit::ReorderContainerChildren {
            container,
            new_order,
        } => super::actor_edits::reorder_container_children(stmts, &container, new_order),
        SourceEdit::InsertActor {
            ty,
            label,
            props,
            container,
            time_s,
        } => super::actor_edits::insert_actor(
            stmts,
            &ty,
            &label,
            props,
            container.as_deref(),
            time_s,
        ),
        SourceEdit::RenameActor {
            old_label,
            new_label,
        } => {
            super::actor_edits::rename_all_references(stmts, &old_label, &new_label);
            Ok(())
        },
        SourceEdit::DeleteActor { label } => super::actor_edits::delete_actor(stmts, &label),
        SourceEdit::DuplicateActor {
            original_label,
            new_label,
        } => super::actor_edits::duplicate_actor(stmts, &original_label, &new_label),
        SourceEdit::PasteActors { clipboard, time_s } => {
            super::actor_edits::paste_actors(stmts, &clipboard, time_s)
        },
        SourceEdit::RemoveProperty { actor, property } => {
            super::actor_edits::remove_property(stmts, &actor, &property)
        },
        SourceEdit::ReorderScenes { new_order } => {
            super::scene_edits::reorder_scenes(stmts, new_order)
        },
        SourceEdit::SetPlayTarget { scene, target } => {
            super::scene_edits::set_play_target(stmts, &scene, target.as_deref())
        },
        SourceEdit::SetTransition {
            from_scene,
            transition,
        } => super::scene_edits::set_transition(stmts, &from_scene, transition),
        SourceEdit::SetSceneDuration { scene, duration_s } => {
            super::scene_edits::set_scene_duration(stmts, &scene, duration_s)
        },
        SourceEdit::RenameScene { old_name, new_name } => {
            super::scene_edits::rename_scene(stmts, &old_name, &new_name)
        },
        SourceEdit::AddScene { name } => super::scene_edits::add_scene(stmts, &name),
        SourceEdit::DeleteScene { name } => super::scene_edits::delete_scene(stmts, &name),
        SourceEdit::DuplicateScene { name } => super::scene_edits::duplicate_scene(stmts, &name),
        SourceEdit::SetKeyframeEasing {
            scene,
            actor,
            property,
            time_s,
            easing,
        } => super::keyframe_edits::set_keyframe_easing(
            stmts,
            scene.as_deref(),
            &actor,
            &property,
            time_s,
            easing,
        ),
        SourceEdit::DeleteKeyframe {
            scene,
            actor,
            property,
            time_s,
        } => super::keyframe_edits::delete_keyframe(
            stmts,
            scene.as_deref(),
            &actor,
            &property,
            time_s,
        ),
        SourceEdit::Reparent { actor, new_parent } => {
            super::actor_edits::reparent_actor(stmts, &actor, new_parent.as_deref())
        },
        SourceEdit::ExtractScene {
            actor_labels,
            new_scene_name,
        } => super::scene_edits::extract_scene(stmts, actor_labels, &new_scene_name),
        SourceEdit::MoveToScene {
            actor_labels,
            target_scene,
        } => super::scene_edits::move_to_scene(stmts, actor_labels, &target_scene),
        SourceEdit::MoveKeyframeTime {
            scene,
            actor,
            property,
            old_time_s,
            new_time_s,
        } => super::keyframe_edits::move_keyframe_time(
            stmts,
            scene.as_deref(),
            &actor,
            &property,
            old_time_s,
            new_time_s,
        ),
        SourceEdit::InsertAction {
            verb,
            targets,
            args,
            modifiers,
            time_s,
        } => super::action_edits::insert_action(stmts, &verb, &targets, &args, &modifiers, time_s),
        SourceEdit::SetConfigProperty { key, value } => {
            super::config_edits::set_config_property(stmts, &key, value)
        },
        SourceEdit::ResizeAction {
            verb,
            targets,
            old_start_s,
            new_start_s,
            new_duration_s,
        } => super::action_edits::resize_action(
            stmts,
            &verb,
            &targets,
            old_start_s,
            new_start_s,
            new_duration_s,
        ),
        SourceEdit::InsertSnippet {
            stmts: fragment,
            time_s,
            container,
        } => super::actor_edits::insert_snippet(stmts, fragment, time_s, container.as_deref()),
    }
}

// Re-export shared traversal from animatix_syntax::walk
pub use animatix_syntax::walk::find_actor_decl;
pub(super) use animatix_syntax::walk::{
    find_actor_decl_mut, find_assignment_mut, find_prop_mut, find_scene_mut, time_to_seconds,
    walk_stmts_mut,
};

#[cfg(test)]
mod variant_coverage_guardrails {
    /// When adding a new variant to `SourceEdit`, update:
    /// - `apply_edit` in this file
    /// - Any handler that constructs or matches SourceEdit variants
    #[test]
    fn apply_edit_covers_all_source_edit_variants() {
        // Count the number of SourceEdit variants
        // This test serves as a reminder to update apply_edit
        // when a new SourceEdit variant is added.
        // The actual exhaustive check is done by the compiler.
    }
}
