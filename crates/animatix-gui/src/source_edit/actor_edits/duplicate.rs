//! Actor duplication and clipboard paste.

use animatix_syntax::ast::Stmt;
use animatix_syntax::walk::find_actor_decl;

use super::super::SourceEditError;
use super::super::ast_utils::{find_keyframes_for_actor, shift_keyframe_times};
use super::rename::rename_all_references;

/// Duplicate an actor declaration with `new_label`, inserting it after the
/// original top-level declaration when possible.
pub(in crate::source_edit) fn duplicate_actor(
    stmts: &mut Vec<Stmt>,
    original_label: &str,
    new_label: &str,
) -> Result<(), SourceEditError> {
    let original_stmt = find_actor_decl(stmts, original_label).cloned().ok_or_else(|| {
        SourceEditError::ActorNotFound {
            actor: original_label.to_string(),
        }
    })?;

    let mut new_stmt = original_stmt;
    match &mut new_stmt {
        Stmt::ActorDecl { label, .. } => *label = new_label.to_string(),
        _ => {
            return Err(SourceEditError::Generic(
                "duplicate target is not an actor declaration".to_string(),
            ));
        },
    }

    if let Some(pos) = stmts
        .iter()
        .position(|s| matches!(s, Stmt::ActorDecl { label, .. } if label == original_label))
    {
        stmts.insert(pos + 1, new_stmt);
    } else {
        stmts.push(new_stmt);
    }
    Ok(())
}

/// Paste actor declarations and their keyframed assignments.
///
/// `clipboard` contains pre-resolved `(original_label, new_label)` pairs.
/// Keyframe assignments referencing an original actor are copied, renamed, and
/// shifted by `time_s` before being appended to the statement list.
pub(in crate::source_edit) fn paste_actors(
    stmts: &mut Vec<Stmt>,
    clipboard: &[(String, String)],
    time_s: f64,
) -> Result<(), SourceEditError> {
    if clipboard.is_empty() {
        return Err(SourceEditError::EmptyActorList);
    }

    let mut pasted = 0usize;
    for (original_label, new_label) in clipboard {
        let Some(original_stmt) = find_actor_decl(stmts, original_label).cloned() else {
            continue;
        };
        let mut new_stmt = original_stmt;
        match &mut new_stmt {
            Stmt::ActorDecl { label, .. } => *label = new_label.clone(),
            _ => continue,
        }

        if let Some(pos) = stmts
            .iter()
            .position(|s| matches!(s, Stmt::ActorDecl { label, .. } if label == original_label))
        {
            stmts.insert(pos + 1, new_stmt);
        } else {
            stmts.push(new_stmt);
        }

        for mut kf in find_keyframes_for_actor(stmts, original_label) {
            rename_all_references(std::slice::from_mut(&mut kf), original_label, new_label);
            shift_keyframe_times(std::slice::from_mut(&mut kf), time_s);
            stmts.push(kf);
        }
        pasted += 1;
    }

    if pasted == 0 {
        Err(SourceEditError::ActorNotFound {
            actor: clipboard[0].0.clone(),
        })
    } else {
        Ok(())
    }
}
