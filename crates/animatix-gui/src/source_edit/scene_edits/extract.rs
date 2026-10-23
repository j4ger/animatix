use super::*;

pub(crate) fn extract_scene(
    stmts: &mut Vec<Stmt>,
    actor_labels: Vec<String>,
    new_scene_name: &str,
) -> Result<(), SourceEditError> {
    if actor_labels.is_empty() {
        return Err(SourceEditError::EmptyActorList);
    }

    // Determine which scene the actors are being extracted from.
    // We find the first scene that contains any of the requested labels.
    let source_scene_name = find_containing_scene_name(stmts, &actor_labels);

    // Collect the actor statements we want to extract.
    let mut extracted: Vec<Stmt> = Vec::new();
    extract_actors_from_stmts(stmts, &actor_labels, &mut extracted);

    if extracted.is_empty() {
        return Err(SourceEditError::Generic("No actors found to extract".to_string()));
    }

    // Create the new scene.
    let new_scene = Stmt::Scene {
        name: new_scene_name.to_string(),
        config: vec![],
        body: extracted,
        span: None,
    };

    // Add play statement linking to the new scene — insert into the *source*
    // scene's body so the edge is local to where the actors came from.
    let play_stmt = Stmt::Play {
        scene_name: new_scene_name.to_string(),
        transition: None,
        span: None,
    };

    // Insert the new scene after the last scene in the file.
    stmts.push(new_scene);

    // Insert the play statement into the source scene's body.
    if let Some(source_name) = source_scene_name {
        if let Some(Stmt::Scene { body, .. }) = find_scene_mut(stmts, &source_name) {
            body.push(play_stmt);
        } else {
            // Fallback: source scene not found (shouldn't happen), append at top level.
            stmts.push(play_stmt);
        }
    } else {
        // Actors were at top level (not inside any scene) — append play at top level.
        stmts.push(play_stmt);
    }

    Ok(())
}

/// Move selected actors to an existing scene.
pub(crate) fn move_to_scene(
    stmts: &mut Vec<Stmt>,
    actor_labels: Vec<String>,
    target_scene: &str,
) -> Result<(), SourceEditError> {
    if actor_labels.is_empty() {
        return Err(SourceEditError::EmptyActorList);
    }

    // Collect the actor statements we want to move.
    let mut moved: Vec<Stmt> = Vec::new();
    extract_actors_from_stmts(stmts, &actor_labels, &mut moved);

    if moved.is_empty() {
        return Err(SourceEditError::Generic("No actors found to move".to_string()));
    }

    // Find the target scene and append the moved actors.
    if let Some(Stmt::Scene { body, .. }) = find_scene_mut(stmts, target_scene) {
        body.extend(moved);
        return Ok(());
    }

    Err(SourceEditError::SceneNotFound {
        scene: target_scene.to_string(),
    })
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
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. } => {
                extract_actors_from_stmts(body, labels, out);
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                extract_actors_from_stmts(then_branch, labels, out);
                if let Some(else_b) = else_branch {
                    extract_actors_from_stmts(else_b, labels, out);
                }
            },
            Stmt::ForLoop { body, .. } => {
                extract_actors_from_stmts(body, labels, out);
            },
            Stmt::ComponentDef(def, _) => {
                extract_actors_from_stmts(&mut def.body, labels, out);
            },
            _ => {},
        }

        i += 1;
    }
}

/// Find the name of the scene that contains any of the given actor labels.
/// Returns `None` if actors are at the top level (not inside any scene).
fn find_containing_scene_name(stmts: &[Stmt], labels: &[String]) -> Option<String> {
    for stmt in stmts {
        if let Stmt::Scene { name, body, .. } = stmt {
            if scene_body_has_any_label(body, labels) {
                return Some(name.clone());
            }
        }
    }
    None
}

/// Check if a statement tree contains any actor declaration with the given labels.
fn scene_body_has_any_label(stmts: &[Stmt], labels: &[String]) -> bool {
    let mut found = false;
    animatix_syntax::walk::walk_stmts(stmts, &mut |stmt| {
        if let Stmt::ActorDecl { label, .. } = stmt {
            if labels.contains(label) {
                found = true;
            }
        }
    });
    found
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
