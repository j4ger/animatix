//! Edits related to scenes: reorder, play targets, transitions, rename, add, delete,
//! and scene refactorings (extract, move).

use animatix::ast::{Stmt, Transition};

use super::apply::{find_scene_mut, walk_stmts_mut};

// ---------------------------------------------------------------------------
// Scene helpers
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// ReorderScenes
// ---------------------------------------------------------------------------

pub(super) fn reorder_scenes(stmts: &mut Vec<Stmt>, new_order: Vec<String>) -> bool {
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

// ---------------------------------------------------------------------------
// SetPlayTarget
// ---------------------------------------------------------------------------

pub(super) fn set_play_target(stmts: &mut [Stmt], scene: &str, target: Option<&str>) -> bool {
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

// ---------------------------------------------------------------------------
// SetTransition
// ---------------------------------------------------------------------------

pub(super) fn set_transition(stmts: &mut [Stmt], from_scene: &str, transition: Option<Transition>) -> bool {
    let scene_stmt = match find_scene_mut(stmts, from_scene) {
        Some(stmt) => stmt,
        None => return false,
    };
    let Stmt::Scene { body, .. } = scene_stmt else { return false; };

    if let Some(Stmt::Play { transition: play_transition, .. }) = body.iter_mut().find(|stmt| matches!(stmt, Stmt::Play { .. })) {
        *play_transition = transition;
        return true;
    }

    false
}

// ---------------------------------------------------------------------------
// RenameScene
// ---------------------------------------------------------------------------

pub(super) fn rename_scene(stmts: &mut [Stmt], old_name: &str, new_name: &str) -> bool {
    if old_name == new_name {
        return true;
    }
    if stmts.iter().any(|stmt| matches!(stmt, Stmt::Scene { name, .. } if name == new_name)) {
        return false;
    }

    let mut renamed = false;
    walk_stmts_mut(stmts, &mut |stmt| {
        renamed |= rename_scene_in_stmt(stmt, old_name, new_name);
    });
    renamed
}

/// Rename scene references inside a single statement (non-recursive).
fn rename_scene_in_stmt(stmt: &mut Stmt, old_name: &str, new_name: &str) -> bool {
    let mut renamed = false;
    match stmt {
        Stmt::Scene { name, .. } => {
            if name == old_name {
                *name = new_name.into();
                renamed = true;
            }
        }
        Stmt::Play { scene_name, .. } => {
            if scene_name == old_name {
                *scene_name = new_name.into();
                renamed = true;
            }
        }
        _ => {}
    }
    renamed
}

// ---------------------------------------------------------------------------
// AddScene / DeleteScene
// ---------------------------------------------------------------------------

pub(super) fn add_scene(stmts: &mut Vec<Stmt>, name: &str) -> bool {
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

pub(super) fn delete_scene(stmts: &mut Vec<Stmt>, name: &str) -> bool {
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
// Scene Refactorings
// ---------------------------------------------------------------------------

/// Extract selected actors into a new scene.
///
/// 1. Find and remove the actor declarations from their current location.
/// 2. Create a new scene containing those actors.
/// 3. Append the new scene after the current scene (or at top level).
/// 4. Add a `play` statement linking to the new scene.
pub(super) fn extract_scene(stmts: &mut Vec<Stmt>, actor_labels: Vec<String>, new_scene_name: &str) -> bool {
    if actor_labels.is_empty() {
        return false;
    }

    // Collect the actor statements we want to extract.
    let mut extracted: Vec<Stmt> = Vec::new();
    extract_actors_from_stmts(stmts, &actor_labels, &mut extracted);

    if extracted.is_empty() {
        return false;
    }

    // Create the new scene.
    let new_scene = Stmt::Scene {
        name: new_scene_name.to_string(),
        config: vec![],
        body: extracted,
        span: None,
    };

    // Add play statement linking to the new scene.
    let play_stmt = Stmt::Play {
        scene_name: new_scene_name.to_string(),
        transition: None,
        span: None,
    };

    stmts.push(new_scene);
    stmts.push(play_stmt);
    true
}

/// Move selected actors to an existing scene.
pub(super) fn move_to_scene(stmts: &mut Vec<Stmt>, actor_labels: Vec<String>, target_scene: &str) -> bool {
    if actor_labels.is_empty() {
        return false;
    }

    // Collect the actor statements we want to move.
    let mut moved: Vec<Stmt> = Vec::new();
    extract_actors_from_stmts(stmts, &actor_labels, &mut moved);

    if moved.is_empty() {
        return false;
    }

    // Find the target scene and append the moved actors.
    if let Some(Stmt::Scene { body, .. }) = find_scene_mut(stmts, target_scene) {
        body.extend(moved);
        return true;
    }

    false
}

/// Recursively find and remove actor declarations matching `labels` from `stmts`,
/// pushing them into `out`.
fn extract_actors_from_stmts(stmts: &mut Vec<Stmt>, labels: &[String], out: &mut Vec<Stmt>) {
    let mut i = 0;
    while i < stmts.len() {
        // Check if this statement should be removed (actor match).
        let should_remove = if let Stmt::ActorDecl { label, .. } = &stmts[i] {
            labels.contains(label)
        } else {
            false
        };

        if should_remove {
            out.push(stmts.remove(i));
            continue;
        }

        // Otherwise, recurse into children mutably.
        match &mut stmts[i] {
            Stmt::Scene { body, .. } => {
                extract_actors_from_stmts(body, labels, out);
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. } => {
                extract_actors_from_stmts(body, labels, out);
            }
            Stmt::Conditional { then_branch, else_branch, .. } => {
                extract_actors_from_stmts(then_branch, labels, out);
                if let Some(else_b) = else_branch {
                    extract_actors_from_stmts(else_b, labels, out);
                }
            }
            Stmt::ForLoop { body, .. } => {
                extract_actors_from_stmts(body, labels, out);
            }
            Stmt::ComponentDef(def, _) => {
                extract_actors_from_stmts(&mut def.body, labels, out);
            }
            _ => {}
        }

        i += 1;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::apply::{SourceEdit, apply_edit};
    use animatix::ast::{Stmt, Transition};
    use animatix::parser::parser;
    use animatix::to_source::stmts_to_source;
    use chumsky::Parser;

    fn parse(source: &str) -> Vec<Stmt> {
        parser().parse(source).into_result().expect("failed to parse test source")
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