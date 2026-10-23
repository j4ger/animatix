use super::*;

pub(super) fn insert_keyframe_inner(
    stmts: &mut Vec<Stmt>,
    actor: &str,
    property: &str,
    value: Expr,
    time_s: f64,
    prev_time_s: f64,
) -> Result<(), SourceEditError> {
    let delta_s = time_s - prev_time_s;
    if delta_s < 0.001 {
        return Err(SourceEditError::InvalidKeyframeTime { time_s });
    }

    let (target_segments, source_prop) = assignment_target(actor, property);
    let source_prop = canonical_to_source(&source_prop);

    // Format the time offset.
    let offset = if delta_s < 1.0 {
        Time::Milliseconds((delta_s * 1000.0).round() as u64)
    } else {
        Time::Seconds(delta_s)
    };

    let assignment = Stmt::Assignment {
        target: target_segments,
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
    let insert_idx = find_keyframe_insertion_point(stmts, prev_time_s);

    // Wrap leading declarations in #0s if needed
    let insert_idx = wrap_leading_decls_in_zero_keyframe(stmts, insert_idx, prev_time_s);

    // Adjust next relative keyframe to preserve its absolute time
    adjust_following_relative_keyframe(stmts, insert_idx, delta_s);

    stmts.insert(insert_idx, keyframe);
    Ok(())
}

// ---------------------------------------------------------------------------
// MoveKeyframeTime
// ---------------------------------------------------------------------------

pub(crate) fn move_keyframe_time(
    stmts: &mut [Stmt],
    scene: Option<&str>,
    actor: &str,
    property: &str,
    old_time_s: f64,
    new_time_s: f64,
) -> Result<(), SourceEditError> {
    let target = match scene {
        Some(name) => match scene_body_mut(stmts, name) {
            Some(body) => body,
            None => {
                return Err(SourceEditError::KeyframeNotFound {
                    actor: actor.to_string(),
                    property: property.to_string(),
                    time_s: old_time_s,
                });
            },
        },
        None => stmts,
    };
    move_keyframe_time_inner(target, actor, property, old_time_s, new_time_s)
}

fn move_keyframe_time_inner(
    stmts: &mut [Stmt],
    actor: &str,
    property: &str,
    old_time_s: f64,
    new_time_s: f64,
) -> Result<(), SourceEditError> {
    let source_prop = canonical_to_source(property);
    let mut current_time = 0.0f64;
    let mut found_idx: Option<usize> = None;
    let mut found_old_time = 0.0f64;

    // First pass: find the keyframe index and compute absolute times
    for (i, stmt) in stmts.iter().enumerate() {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                current_time = time_to_seconds(time);
                if (current_time - old_time_s).abs() < 0.001
                    && contains_assignment(body, actor, source_prop)
                {
                    found_idx = Some(i);
                    found_old_time = current_time;
                }
            },
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if (current_time - old_time_s).abs() < 0.001
                    && contains_assignment(body, actor, source_prop)
                {
                    found_idx = Some(i);
                    found_old_time = current_time;
                }
            },
            _ => {},
        }
    }

    let idx = match found_idx {
        Some(i) => i,
        None => {
            return Err(SourceEditError::KeyframeNotFound {
                actor: actor.to_string(),
                property: property.to_string(),
                time_s: old_time_s,
            });
        },
    };

    let delta_s = new_time_s - found_old_time;
    if delta_s.abs() < 0.001 {
        return Ok(()); // No change needed
    }

    // Pre-compute flash indices before any mutation.
    let mut flash_indices = Vec::new();
    if matches!(stmts[idx], Stmt::RelativeKeyframe { .. })
        && idx + 1 < stmts.len()
        && matches!(stmts[idx + 1], Stmt::RelativeKeyframe { .. })
    {
        flash_indices.push(idx + 1);
    }
    if matches!(stmts[idx], Stmt::Keyframe { .. }) {
        for (j, stmt) in stmts[(idx + 1)..].iter().enumerate() {
            if matches!(stmt, Stmt::RelativeKeyframe { .. }) {
                flash_indices.push(idx + 1 + j);
                break;
            }
            if matches!(stmt, Stmt::Keyframe { .. }) {
                break;
            }
        }
    }
    for fi in &flash_indices {
        let t = crate::source_edit::ast_utils::compute_keyframe_abs_time(stmts, *fi);
        crate::source_edit::ast_utils::push_adjust_flash_time(t);
    }

    // Update the found keyframe's time
    match &mut stmts[idx] {
        Stmt::Keyframe { time, .. } => {
            *time = if new_time_s < 1.0 {
                Time::Milliseconds((new_time_s * 1000.0).round() as u64)
            } else {
                Time::Seconds(new_time_s)
            };
        },
        Stmt::RelativeKeyframe { offset, .. } => {
            // For relative keyframes, adjust this offset by delta_s
            let new_offset_s = time_to_seconds(offset) + delta_s;
            if new_offset_s < 0.001 {
                return Err(SourceEditError::InvalidKeyframeTime { time_s: new_time_s });
            }
            *offset = if new_offset_s < 1.0 {
                Time::Milliseconds((new_offset_s * 1000.0).round() as u64)
            } else {
                Time::Seconds(new_offset_s)
            };
            // Adjust next relative keyframe's offset to compensate
            if idx + 1 < stmts.len() {
                if let Stmt::RelativeKeyframe {
                    offset: next_offset,
                    ..
                } = &mut stmts[idx + 1]
                {
                    let next_offset_s = time_to_seconds(next_offset) - delta_s;
                    if next_offset_s >= 0.001 {
                        *next_offset = if next_offset_s < 1.0 {
                            Time::Milliseconds((next_offset_s * 1000.0).round() as u64)
                        } else {
                            Time::Seconds(next_offset_s)
                        };
                    }
                }
            }
        },
        _ => return Err(SourceEditError::InvalidKeyframeTime { time_s: new_time_s }),
    }

    // If we changed an absolute keyframe, adjust the first subsequent
    // relative keyframe's offset so its absolute time stays the same.
    if matches!(stmts[idx], Stmt::Keyframe { .. }) {
        for stmt in stmts[(idx + 1)..].iter_mut() {
            if let Stmt::RelativeKeyframe { offset, .. } = stmt {
                let offset_s = time_to_seconds(offset) - delta_s;
                if offset_s >= 0.001 {
                    *offset = if offset_s < 1.0 {
                        Time::Milliseconds((offset_s * 1000.0).round() as u64)
                    } else {
                        Time::Seconds(offset_s)
                    };
                }
                break; // Only adjust the first subsequent relative keyframe
            }
            if matches!(stmt, Stmt::Keyframe { .. }) {
                break; // Stop at next absolute keyframe
            }
        }
    }

    Ok(())
}

/// Check if any assignment in the statement tree matches the given actor + property.
fn contains_assignment(body: &[Stmt], actor: &str, property: &str) -> bool {
    for stmt in body {
        match stmt {
            Stmt::Assignment {
                target,
                property: prop,
                ..
            } if assignment_matches(target, prop, actor, property) => {
                return true;
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ComponentDef(ComponentDef { body, .. }, _)
            | Stmt::FnDecl { body, .. }
            | Stmt::Block { body, .. } => {
                if contains_assignment(body, actor, property) {
                    return true;
                }
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                if contains_assignment(then_branch, actor, property) {
                    return true;
                }
                if let Some(else_b) = else_branch {
                    if contains_assignment(else_b, actor, property) {
                        return true;
                    }
                }
            },
            Stmt::ForLoop { body, .. } => {
                if contains_assignment(body, actor, property) {
                    return true;
                }
            },
            _ => {},
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
