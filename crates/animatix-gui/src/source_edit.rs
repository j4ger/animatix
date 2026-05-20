//! AST-based source editing for the GUI inspector.
//!
//! Replaces the old byte-span surgery model with semantic
//! edits applied directly to the AST. After mutation, the entire AST is
//! re-serialized via [`animatix::to_source::stmts_to_source`].

use animatix::ast::{ComponentDef, Expr, InlineItem, Property, Stmt, Time, Transition};

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
    /// Update the easing of an existing keyframe's assignment.
    SetKeyframeEasing {
        actor: String,
        property: String,
        /// Absolute time in seconds.
        time_s: f64,
        easing: animatix::easing::Easing,
    },
    /// Move an actor to a new parent container (or to top-level if None).
    /// If the target is not a container, both are wrapped in a new Group.
    Reparent {
        actor: String,
        new_parent: Option<String>,
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
        SourceEdit::MergeKeyframe {
            actor,
            property,
            value,
            time_s,
        } => merge_keyframe(stmts, &actor, &property, value, time_s),
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
        SourceEdit::ReorderScenes { new_order } => reorder_scenes(stmts, new_order),
        SourceEdit::SetPlayTarget { scene, target } => {
            set_play_target(stmts, &scene, target.as_deref())
        }
        SourceEdit::SetTransition { from_scene, transition } => {
            set_transition(stmts, &from_scene, transition)
        }
        SourceEdit::RenameScene { old_name, new_name } => {
            rename_scene(stmts, &old_name, &new_name)
        }
        SourceEdit::AddScene { name } => add_scene(stmts, &name),
        SourceEdit::DeleteScene { name } => delete_scene(stmts, &name),
        SourceEdit::SetKeyframeEasing { actor, property, time_s, easing } => {
            set_keyframe_easing(stmts, &actor, &property, time_s, easing)
        }
        SourceEdit::Reparent { actor, new_parent } => {
            reparent_actor(stmts, &actor, new_parent.as_deref())
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
// MergeKeyframe
// ---------------------------------------------------------------------------

fn merge_keyframe(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    value: Expr,
    time_s: f64,
) -> bool {
    let source_prop = canonical_to_source(property);
    let mut current_time = 0.0f64;

    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                current_time = time_to_seconds(time);
                if (current_time - time_s).abs() < 0.001 {
                    return update_assignment(body, actor, source_prop, value);
                }
            }
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if (current_time - time_s).abs() < 0.001 {
                    return update_assignment(body, actor, source_prop, value);
                }
            }
            _ => {}
        }
    }

    false
}

fn update_assignment(body: &mut [Stmt], actor: &str, property: &str, value: Expr) -> bool {
    for stmt in body.iter_mut() {
        match stmt {
            Stmt::Assignment { target, property: prop, value: val, .. }
                if target.iter().any(|t| t == actor) && prop == property =>
            {
                *val = value;
                return true;
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                if update_assignment(body, actor, property, value.clone()) {
                    return true;
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                if update_assignment(then_branch, actor, property, value.clone()) {
                    return true;
                }
                if let Some(else_b) = else_branch {
                    if update_assignment(else_b, actor, property, value.clone()) {
                        return true;
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                if update_assignment(body, actor, property, value.clone()) {
                    return true;
                }
            }
            _ => {}
        }
    }

    false
}

// ---------------------------------------------------------------------------
// SetKeyframeEasing
// ---------------------------------------------------------------------------

fn set_keyframe_easing(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    time_s: f64,
    easing: animatix::easing::Easing,
) -> bool {
    let source_prop = canonical_to_source(property);
    let easing_name = match easing {
        animatix::easing::Easing::Linear => "linear",
        animatix::easing::Easing::EaseIn => "easein",
        animatix::easing::Easing::EaseOut => "easeout",
        animatix::easing::Easing::EaseInOut => "easeinout",
        animatix::easing::Easing::Bounce => "bounce",
        animatix::easing::Easing::Elastic => "elastic",
        animatix::easing::Easing::Back => "back",
        animatix::easing::Easing::Expo => "expo",
    };
    let easing_expr = animatix::ast::Expr::Ident(easing_name.to_string());

    // Walk through keyframes looking for the match
    let mut current_time = 0.0f64;

    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                current_time = time_to_seconds(time);
                if (current_time - time_s).abs() < 0.001 {
                    return update_assignment_easing(body, actor, source_prop, &easing_expr);
                }
            }
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if (current_time - time_s).abs() < 0.001 {
                    return update_assignment_easing(body, actor, source_prop, &easing_expr);
                }
            }
            _ => {}
        }
    }

    false
}

/// Walk into an assignment at the given time and set its easing modifier.
fn update_assignment_easing(body: &mut [Stmt], actor: &str, property: &str, easing_expr: &animatix::ast::Expr) -> bool {
    for stmt in body.iter_mut() {
        match stmt {
            Stmt::Assignment { target, property: prop, modifiers, .. }
                if target.iter().any(|t| t == actor) && prop == property =>
            {
                // Find existing ease modifier or add new one
                if let Some(existing) = modifiers.iter_mut().find(|m| m.name.as_deref() == Some("ease")) {
                    existing.value = easing_expr.clone();
                } else {
                    modifiers.push(animatix::ast::Modifier {
                        name: Some("ease".into()),
                        value: easing_expr.clone(),
                    });
                }
                return true;
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::ComponentAction { body, .. } => {
                if update_assignment_easing(body, actor, property, easing_expr) {
                    return true;
                }
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                if update_assignment_easing(then_branch, actor, property, easing_expr) {
                    return true;
                }
                if let Some(else_b) = else_branch {
                    if update_assignment_easing(else_b, actor, property, easing_expr) {
                        return true;
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                if update_assignment_easing(body, actor, property, easing_expr) {
                    return true;
                }
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
        easing: None,
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
// Scene helpers
// ---------------------------------------------------------------------------

fn find_scene_mut<'a>(stmts: &'a mut [Stmt], name: &str) -> Option<&'a mut Stmt> {
    stmts.iter_mut().find(|stmt| matches!(stmt, Stmt::Scene { name: scene_name, .. } if scene_name == name))
}

fn scene_names(stmts: &[Stmt]) -> Vec<String> {
    stmts
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Scene { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn duplicate_name_in_order(order: &[String]) -> Option<String> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    order.iter().find_map(|name| {
        if !seen.insert(name) {
            Some(name.clone())
        } else {
            None
        }
    })
}

fn reorder_scenes(stmts: &mut Vec<Stmt>, new_order: Vec<String>) -> bool {
    if duplicate_name_in_order(&new_order).is_some() {
        return false;
    }

    let existing = scene_names(stmts);
    if existing.len() != new_order.len() || existing.iter().any(|name| !new_order.iter().any(|n| n == name)) {
        return false;
    }

    let first_scene_idx = match stmts.iter().position(|stmt| matches!(stmt, Stmt::Scene { .. })) {
        Some(idx) => idx,
        None => return false,
    };

    let mut scenes = Vec::new();
    let mut prelude = stmts.drain(..first_scene_idx).collect::<Vec<_>>();
    let mut tail = Vec::new();
    for stmt in stmts.drain(..) {
        match stmt {
            Stmt::Scene { .. } => scenes.push(stmt),
            other => tail.push(other),
        }
    }

    let mut by_name = std::collections::BTreeMap::new();
    for scene in scenes {
        if let Stmt::Scene { name, .. } = &scene {
            by_name.insert(name.clone(), scene);
        }
    }

    let mut reordered = Vec::new();
    reordered.append(&mut prelude);
    for name in new_order {
        if let Some(scene) = by_name.remove(&name) {
            reordered.push(scene);
        } else {
            return false;
        }
    }
    reordered.extend(tail);
    *stmts = reordered;
    true
}

fn set_play_target(stmts: &mut [Stmt], scene: &str, target: Option<&str>) -> bool {
    let scene_stmt = match find_scene_mut(stmts, scene) {
        Some(stmt) => stmt,
        None => return false,
    };

    let Stmt::Scene { body, .. } = scene_stmt else { return false; };

    match target {
        Some(target_scene) => {
            let mut updated = false;
            for stmt in body.iter_mut() {
                if let Stmt::Play { scene_name, .. } = stmt {
                    if !updated {
                        *scene_name = target_scene.to_string();
                        updated = true;
                    }
                }
            }
            if !updated {
                body.push(Stmt::Play {
                    scene_name: target_scene.to_string(),
                    transition: None,
                    span: None,
                });
            } else {
                let mut seen = false;
                body.retain(|stmt| match stmt {
                    Stmt::Play { .. } => {
                        if seen {
                            false
                        } else {
                            seen = true;
                            true
                        }
                    }
                    _ => true,
                });
            }
            true
        }
        None => {
            let before = body.len();
            body.retain(|stmt| !matches!(stmt, Stmt::Play { .. }));
            before != body.len()
        }
    }
}

fn set_transition(stmts: &mut [Stmt], from_scene: &str, transition: Option<Transition>) -> bool {
    let scene_stmt = match find_scene_mut(stmts, from_scene) {
        Some(stmt) => stmt,
        None => return false,
    };
    let Stmt::Scene { body, .. } = scene_stmt else { return false; };

    if let Some(play) = body.iter_mut().find(|stmt| matches!(stmt, Stmt::Play { .. })) {
        if let Stmt::Play { transition: play_transition, .. } = play {
            *play_transition = transition;
            return true;
        }
    }

    false
}

fn rename_scene(stmts: &mut [Stmt], old_name: &str, new_name: &str) -> bool {
    if old_name == new_name {
        return true;
    }
    if stmts.iter().any(|stmt| matches!(stmt, Stmt::Scene { name, .. } if name == new_name)) {
        return false;
    }

    let mut renamed = false;
    for stmt in stmts.iter_mut() {
        renamed |= rename_scene_in_stmt(stmt, old_name, new_name);
    }
    renamed
}

fn rename_scene_in_stmt(stmt: &mut Stmt, old_name: &str, new_name: &str) -> bool {
    let mut renamed = false;
    match stmt {
        Stmt::Scene { name, body, .. } => {
            if name == old_name {
                *name = new_name.into();
                renamed = true;
            }
            for child in body.iter_mut() {
                renamed |= rename_scene_in_stmt(child, old_name, new_name);
            }
        }
        Stmt::Play { scene_name, .. } => {
            if scene_name == old_name {
                *scene_name = new_name.into();
                renamed = true;
            }
        }
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::ComponentDef(ComponentDef { body, .. }, _)
        | Stmt::ComponentAction { body, .. } => {
            for child in body.iter_mut() {
                renamed |= rename_scene_in_stmt(child, old_name, new_name);
            }
        }
        Stmt::Conditional { then_branch, else_branch, .. } => {
            for child in then_branch.iter_mut() {
                renamed |= rename_scene_in_stmt(child, old_name, new_name);
            }
            if let Some(else_branch) = else_branch {
                for child in else_branch.iter_mut() {
                    renamed |= rename_scene_in_stmt(child, old_name, new_name);
                }
            }
        }
        Stmt::ForLoop { body, .. } => {
            for child in body.iter_mut() {
                renamed |= rename_scene_in_stmt(child, old_name, new_name);
            }
        }
        _ => {}
    }
    renamed
}

fn add_scene(stmts: &mut Vec<Stmt>, name: &str) -> bool {
    if stmts.iter().any(|stmt| matches!(stmt, Stmt::Scene { name: scene_name, .. } if scene_name == name)) {
        return false;
    }
    stmts.push(Stmt::Scene {
        name: name.into(),
        config: vec![],
        body: vec![],
        span: None,
    });
    true
}

fn delete_scene(stmts: &mut Vec<Stmt>, name: &str) -> bool {
    let mut removed = false;
    // 1. Remove the Scene declaration and any Play statements targeting it
    stmts.retain(|stmt| match stmt {
        Stmt::Scene { name: scene_name, .. } => {
            if scene_name == name {
                removed = true;
                false
            } else {
                true
            }
        }
        Stmt::Play { scene_name, .. } => scene_name != name,
        _ => true,
    });
    // 2. Also remove Play statements from within remaining Scene bodies
    for stmt in stmts.iter_mut() {
        if let Stmt::Scene { body, .. } = stmt {
            body.retain(|child| !matches!(child, Stmt::Play { scene_name, .. } if scene_name == name));
        }
    }
    removed
}

// ---------------------------------------------------------------------------
// Reparent
// ---------------------------------------------------------------------------

fn reparent_actor(stmts: &mut Vec<Stmt>, actor: &str, new_parent: Option<&str>) -> bool {
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
    match stmt {
        Stmt::ActorDecl { label: l, .. } if l == label => true,
        Stmt::Text { label: Some(l), .. } if l == label => true,
        Stmt::Math { label: Some(l), .. } if l == label => true,
        Stmt::Code { label: Some(l), .. } if l == label => true,
        Stmt::Svg { label: Some(l), .. } if l == label => true,
        Stmt::Image { label: Some(l), .. } if l == label => true,
        _ => false,
    }
}

fn stmt_to_inline_item(stmt: Stmt) -> InlineItem {
    match stmt {
        Stmt::ActorDecl { label, ty, props, modifiers, children, .. } => InlineItem::Labeled {
            label,
            ty,
            props,
            modifiers,
            children,
        },
        Stmt::Text { label, props, .. } => InlineItem::Labeled {
            label: label.unwrap_or_default(),
            ty: "Text".into(),
            props,
            modifiers: vec![],
            children: vec![],
        },
        Stmt::Math { label, props, .. } => InlineItem::Labeled {
            label: label.unwrap_or_default(),
            ty: "Math".into(),
            props,
            modifiers: vec![],
            children: vec![],
        },
        Stmt::Code { label, props, .. } => InlineItem::Labeled {
            label: label.unwrap_or_default(),
            ty: "Code".into(),
            props,
            modifiers: vec![],
            children: vec![],
        },
        Stmt::Svg { label, .. } => InlineItem::Labeled {
            label: label.unwrap_or_default(),
            ty: "Svg".into(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
        },
        Stmt::Image { label, .. } => InlineItem::Labeled {
            label: label.unwrap_or_default(),
            ty: "Image".into(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
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
            label,
            ty,
            props,
            modifiers,
            children,
            span: None,
        },
        InlineItem::Anonymous { ty, props, modifiers, children } => Stmt::ActorDecl {
            is_pub: false,
            label: format!("__anon_root_{}", index),
            ty,
            props,
            modifiers,
            children,
            span: None,
        },
        InlineItem::SlotFill { slot, items, .. } => Stmt::ActorDecl {
            is_pub: false,
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
                label: "__slot_marker".into(),
                ty: "Group".into(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            }
        }
        _ => unreachable!("unexpected InlineItem variant"),
    }
}

fn extract_inline_item(stmts: &mut Vec<Stmt>, label: &str) -> Option<InlineItem> {
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
        InlineItem::SlotMarker { .. } => None,
        _ => None,
    }
}

fn inline_item_has_label(item: &InlineItem, label: &str) -> bool {
    match item {
        InlineItem::Labeled { label: l, .. } if l == label => true,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// AST traversal helpers
// ---------------------------------------------------------------------------

/// Find a mutable reference to an ActorDecl (or Text/Math/Code/Svg/Image) with
/// the given label anywhere in the statement tree.
pub fn find_actor_decl<'a>(stmts: &'a [Stmt], label: &str) -> Option<&'a Stmt> {
    for stmt in stmts.iter() {
        match stmt {
            Stmt::ActorDecl { label: l, .. } if l == label => return Some(stmt),
            Stmt::Text { label: Some(l), .. } if l == label => return Some(stmt),
            Stmt::Math { label: Some(l), .. } if l == label => return Some(stmt),
            Stmt::Code { label: Some(l), .. } if l == label => return Some(stmt),
            Stmt::Svg { label: Some(l), .. } if l == label => return Some(stmt),
            Stmt::Image { label: Some(l), .. } if l == label => return Some(stmt),
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
            Stmt::ActorDecl { children, .. } => {
                // Inline items are not Stmts, so we can't return them here.
                // This is a limitation — nested children aren't editable via
                // this path. For now, we only support top-level actors.
                let _ = children;
            }
            _ => {}
        }
    }
    None
}

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
pub(crate) fn rename_all_references(stmts: &mut [Stmt], old_label: &str, new_label: &str) {
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
    use animatix::to_source::stmts_to_source;
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
circle: Ellipse, radius: 50"#);

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
    fn merge_keyframe_updates_existing_assignment() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#1s
btn.color = red
btn.position = (10, 20)"#);

        let edit = SourceEdit::MergeKeyframe {
            actor: "btn".into(),
            property: "color".into(),
            value: Expr::Ident("blue".into()),
            time_s: 1.0,
        };
        assert!(apply_edit(&mut stmts, edit));

        let mut found = false;
        if let Stmt::Keyframe { body, .. } = &stmts[1] {
            for stmt in body {
                if let Stmt::Assignment { property, value, .. } = stmt {
                    if property == "color" {
                        assert_eq!(*value, Expr::Ident("blue".into()));
                        found = true;
                    }
                }
            }
        }
        assert!(found);
    }

    #[test]
    fn merge_keyframe_uses_relative_time() {
        let mut stmts = parse(r#"#0s
btn: Rect, size: (100, 200)

#+500ms
btn.color = red"#);

        let edit = SourceEdit::MergeKeyframe {
            actor: "btn".into(),
            property: "color".into(),
            value: Expr::Ident("green".into()),
            time_s: 0.5,
        };
        assert!(apply_edit(&mut stmts, edit));

        if let Stmt::RelativeKeyframe { body, .. } = &stmts[1] {
            if let Stmt::Assignment { value, .. } = &body[0] {
                assert_eq!(*value, Expr::Ident("green".into()));
            } else {
                panic!("Expected Assignment");
            }
        } else {
            panic!("Expected RelativeKeyframe");
        }
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
        let container = find_actor_decl_mut(&mut stmts, "row1").unwrap();
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

    #[test]
    fn add_scene_appends_new_scene() {
        let mut stmts = parse("import \"foo\"\n\n# Intro\nplay Outro");
        assert!(apply_edit(&mut stmts, SourceEdit::AddScene { name: "Outro".into() }));
        assert!(matches!(stmts.last(), Some(Stmt::Scene { name, body, .. }) if name == "Outro" && body.is_empty()));
    }

    #[test]
    fn reorder_scenes_changes_scene_order() {
        let mut stmts = parse("import \"foo\"\n\n# Intro\nplay Middle\n\n# Middle\nplay Outro\n\n# Outro");
        assert!(apply_edit(
            &mut stmts,
            SourceEdit::ReorderScenes { new_order: vec!["Outro".into(), "Intro".into(), "Middle".into()] }
        ));
        let scene_names: Vec<_> = stmts.iter().filter_map(|s| match s { Stmt::Scene { name, .. } => Some(name.as_str()), _ => None }).collect();
        assert_eq!(scene_names, vec!["Outro", "Intro", "Middle"]);
        // The import is wrapped in a Keyframe by the parser, so the prelude starts with Keyframe
        assert!(matches!(stmts.first(), Some(Stmt::Keyframe { .. })));
    }

    #[test]
    fn set_play_target_creates_and_removes_play() {
        let mut stmts = parse("# Intro");
        assert!(apply_edit(
            &mut stmts,
            SourceEdit::SetPlayTarget { scene: "Intro".into(), target: Some("Outro".into()) }
        ));
        assert!(stmts_to_source(&stmts).contains("play Outro"));
        assert!(apply_edit(
            &mut stmts,
            SourceEdit::SetPlayTarget { scene: "Intro".into(), target: None }
        ));
        assert!(!stmts_to_source(&stmts).contains("play "));
    }

    #[test]
    fn set_transition_updates_play_statement() {
        let mut stmts = parse("# Intro\nplay Outro\n\n# Outro");
        assert!(apply_edit(
            &mut stmts,
            SourceEdit::SetTransition {
                from_scene: "Intro".into(),
                transition: Some(Transition { id: "fade".into(), duration_ms: 300, easing: animatix::easing::Easing::Linear }),
            }
        ));
        assert!(stmts_to_source(&stmts).contains("play Outro [fade, 300ms]"));
    }

    #[test]
    fn rename_scene_updates_play_references() {
        let mut stmts = parse("# Intro\nplay Outro\n\n# Outro");
        assert!(apply_edit(
            &mut stmts,
            SourceEdit::RenameScene { old_name: "Outro".into(), new_name: "Finale".into() }
        ));
        let src = stmts_to_source(&stmts);
        assert!(src.contains("# Finale"));
        assert!(src.contains("play Finale"));
        assert!(!src.contains("Outro"));
    }

    #[test]
    fn delete_scene_removes_declaration_and_play_references() {
        let mut stmts = parse("# Intro\nplay Outro\n\n# Middle\n\n# Outro");
        assert!(apply_edit(
            &mut stmts,
            SourceEdit::DeleteScene { name: "Outro".into() }
        ));
        let src = stmts_to_source(&stmts);
        assert!(!src.contains("# Outro"));
        assert!(!src.contains("play Outro"));
        assert!(src.contains("# Intro"));
        assert!(src.contains("# Middle"));
    }

    #[test]
    fn delete_scene_returns_false_when_scene_missing() {
        let mut stmts = parse("# Intro");
        assert!(!apply_edit(
            &mut stmts,
            SourceEdit::DeleteScene { name: "Missing".into() }
        ));
    }

    #[test]
    fn scene_edits_fail_for_missing_or_duplicate_names() {
        let mut stmts = parse("# Intro\nplay Outro\n\n# Outro");
        assert!(!apply_edit(
            &mut stmts,
            SourceEdit::RenameScene { old_name: "Missing".into(), new_name: "X".into() }
        ));
        assert!(!apply_edit(&mut stmts, SourceEdit::AddScene { name: "Intro".into() }));
        assert!(!apply_edit(
            &mut stmts,
            SourceEdit::ReorderScenes { new_order: vec!["Intro".into(), "Intro".into()] }
        ));
        assert!(!apply_edit(
            &mut stmts,
            SourceEdit::SetPlayTarget { scene: "Missing".into(), target: Some("X".into()) }
        ));
    }
}
