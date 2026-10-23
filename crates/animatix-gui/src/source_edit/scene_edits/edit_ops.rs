use super::*;

pub(crate) fn reorder_scenes(
    stmts: &mut Vec<Stmt>,
    new_order: Vec<String>,
) -> Result<(), SourceEditError> {
    if let Some(duplicate) = duplicate_name_in_order(&new_order) {
        return Err(SourceEditError::DuplicateSceneName { name: duplicate });
    }

    let existing = scene_names(stmts);
    if existing.len() != new_order.len()
        || existing.iter().any(|name| !new_order.iter().any(|n| n == name))
    {
        return Err(SourceEditError::Generic(
            "Scene order does not match existing scenes".to_string(),
        ));
    }

    // Separate scenes from non-scenes, preserving original interleaving positions
    let original: Vec<Stmt> = std::mem::take(stmts);
    let scene_map: std::collections::HashMap<String, Stmt> = original
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Scene { name, .. } => Some((name.clone(), stmt.clone())),
            _ => None,
        })
        .collect();

    let mut scene_iter = new_order.into_iter();
    let mut reordered = Vec::with_capacity(original.len());
    for original_stmt in &original {
        match original_stmt {
            Stmt::Scene { .. } => {
                if let Some(name) = scene_iter.next() {
                    if let Some(scene) = scene_map.get(&name) {
                        reordered.push(scene.clone());
                    }
                }
            },
            other => reordered.push(other.clone()),
        }
    }

    *stmts = reordered;
    Ok(())
}

// ---------------------------------------------------------------------------
// SetPlayTarget
// ---------------------------------------------------------------------------

pub(crate) fn set_play_target(
    stmts: &mut [Stmt],
    scene: &str,
    target: Option<&str>,
) -> Result<(), SourceEditError> {
    let scene_stmt = match find_scene_mut(stmts, scene) {
        Some(stmt) => stmt,
        None => {
            return Err(SourceEditError::SceneNotFound {
                scene: scene.to_string(),
            });
        },
    };

    let Stmt::Scene { body, .. } = scene_stmt else {
        return Err(SourceEditError::SceneNotFound {
            scene: scene.to_string(),
        });
    };

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
                    },
                    _ => true,
                });
            }
            Ok(())
        },
        None => {
            let before = body.len();
            body.retain(|stmt| !matches!(stmt, Stmt::Play { .. }));
            if before != body.len() {
                Ok(())
            } else {
                Err(SourceEditError::Generic("No play target to remove".to_string()))
            }
        },
    }
}

// ---------------------------------------------------------------------------
// SetTransition
// ---------------------------------------------------------------------------

pub(crate) fn set_transition(
    stmts: &mut [Stmt],
    from_scene: &str,
    transition: Option<Transition>,
) -> Result<(), SourceEditError> {
    let scene_stmt = match find_scene_mut(stmts, from_scene) {
        Some(stmt) => stmt,
        None => {
            return Err(SourceEditError::SceneNotFound {
                scene: from_scene.to_string(),
            });
        },
    };
    let Stmt::Scene { body, .. } = scene_stmt else {
        return Err(SourceEditError::SceneNotFound {
            scene: from_scene.to_string(),
        });
    };

    if let Some(Stmt::Play {
        transition: play_transition,
        ..
    }) = body.iter_mut().find(|stmt| matches!(stmt, Stmt::Play { .. }))
    {
        *play_transition = transition;
        return Ok(());
    }

    Err(SourceEditError::Generic("No play statement to set transition on".to_string()))
}

// ---------------------------------------------------------------------------
// SetSceneDuration
// ---------------------------------------------------------------------------

pub(crate) fn set_scene_duration(
    stmts: &mut [Stmt],
    scene: &str,
    duration_s: Option<f64>,
) -> Result<(), SourceEditError> {
    let scene_stmt = match find_scene_mut(stmts, scene) {
        Some(stmt) => stmt,
        None => {
            return Err(SourceEditError::SceneNotFound {
                scene: scene.to_string(),
            });
        },
    };
    let Stmt::Scene { config, .. } = scene_stmt else {
        return Err(SourceEditError::SceneNotFound {
            scene: scene.to_string(),
        });
    };

    // Find existing duration property in config
    let duration_idx = config.iter().position(|p| p.name == "duration");

    match (duration_s, duration_idx) {
        // Setting a new duration
        (Some(val), Some(idx)) => {
            // Update existing
            config[idx].value = Expr::Num(val);
            Ok(())
        },
        (Some(val), None) => {
            // Insert new
            config.push(Property {
                name: "duration".to_string(),
                value: Expr::Num(val),
                value_span: None,
                trailing_comment: None,
            });
            Ok(())
        },
        // Removing duration
        (None, Some(idx)) => {
            config.remove(idx);
            Ok(())
        },
        (None, None) => {
            // Nothing to do
            Ok(())
        },
    }
}

// ---------------------------------------------------------------------------
// RenameScene
// ---------------------------------------------------------------------------

pub(crate) fn rename_scene(
    stmts: &mut [Stmt],
    old_name: &str,
    new_name: &str,
) -> Result<(), SourceEditError> {
    if old_name == new_name {
        return Ok(());
    }
    if stmts
        .iter()
        .any(|stmt| matches!(stmt, Stmt::Scene { name, .. } if name == new_name))
    {
        return Err(SourceEditError::DuplicateSceneName {
            name: new_name.to_string(),
        });
    }

    let mut renamed = false;
    walk_stmts_mut(stmts, &mut |stmt| {
        renamed |= rename_scene_in_stmt(stmt, old_name, new_name);
    });
    if renamed {
        Ok(())
    } else {
        Err(SourceEditError::SceneNotFound {
            scene: old_name.to_string(),
        })
    }
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
        },
        Stmt::Play { scene_name, .. } => {
            if scene_name == old_name {
                *scene_name = new_name.into();
                renamed = true;
            }
        },
        _ => {},
    }
    renamed
}

// ---------------------------------------------------------------------------
// AddScene / DeleteScene
// ---------------------------------------------------------------------------

pub(crate) fn add_scene(stmts: &mut Vec<Stmt>, name: &str) -> Result<(), SourceEditError> {
    if stmts
        .iter()
        .any(|stmt| matches!(stmt, Stmt::Scene { name: scene_name, .. } if scene_name == name))
    {
        return Err(SourceEditError::DuplicateSceneName {
            name: name.to_string(),
        });
    }
    stmts.push(Stmt::Scene {
        name: name.into(),
        config: vec![],
        body: vec![],
        span: None,
    });
    Ok(())
}

pub(crate) fn duplicate_scene(stmts: &mut Vec<Stmt>, name: &str) -> Result<(), SourceEditError> {
    let original = stmts.iter().position(
        |stmt| matches!(stmt, Stmt::Scene { name: scene_name, .. } if scene_name == name),
    );

    let Some(idx) = original else {
        return Err(SourceEditError::SceneNotFound {
            scene: name.to_string(),
        });
    };

    let mut new_scene = stmts[idx].clone();
    let new_name = {
        let base = format!("{}_copy", name);
        let mut candidate = base.clone();
        let existing: std::collections::HashSet<String> = scene_names(stmts).into_iter().collect();
        if existing.contains(&candidate) {
            for i in 1.. {
                candidate = format!("{}_{}", base, i);
                if !existing.contains(&candidate) {
                    break;
                }
            }
        }
        candidate
    };

    if let Stmt::Scene {
        name: scene_name, ..
    } = &mut new_scene
    {
        *scene_name = new_name.clone();
    }

    // Rename actor labels inside the duplicated scene to avoid conflicts
    // with labels in ANY scene (not just the duplicated one).
    let all_existing_labels = collect_all_labels(stmts);
    let mut used_labels: std::collections::HashSet<String> =
        all_existing_labels.into_iter().collect();
    walk_stmts_mut(std::slice::from_mut(&mut new_scene), &mut |stmt| {
        if let Stmt::ActorDecl { label, .. } = stmt {
            let base = label.clone();
            let mut candidate = format!("{}_0", base);
            let mut counter = 0usize;
            while used_labels.contains(&candidate) {
                counter += 1;
                candidate = format!("{}_{}", base, counter);
            }
            used_labels.insert(candidate.clone());
            *label = candidate;
        }
    });

    stmts.insert(idx + 1, new_scene);
    Ok(())
}

pub(crate) fn delete_scene(stmts: &mut Vec<Stmt>, name: &str) -> Result<(), SourceEditError> {
    let mut removed = false;
    // 1. Remove the Scene declaration and any Play statements targeting it
    stmts.retain(|stmt| match stmt {
        Stmt::Scene {
            name: scene_name, ..
        } => {
            if scene_name == name {
                removed = true;
                false
            } else {
                true
            }
        },
        Stmt::Play { scene_name, .. } => scene_name != name,
        _ => true,
    });
    // 2. Also remove Play statements from within remaining Scene bodies
    for stmt in stmts.iter_mut() {
        if let Stmt::Scene { body, .. } = stmt {
            body.retain(
                |child| !matches!(child, Stmt::Play { scene_name, .. } if scene_name == name),
            );
        }
    }
    if removed {
        Ok(())
    } else {
        Err(SourceEditError::SceneNotFound {
            scene: name.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Scene Refactorings
// ---------------------------------------------------------------------------
