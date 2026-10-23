//! Edits related to scenes: reorder, play targets, transitions, rename, add, delete,
//! and scene refactorings (extract, move).

use animatix_syntax::ast::{Expr, Property, Stmt, Transition};

use super::SourceEditError;
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

/// Collect all actor labels from all scenes (recursively).
fn collect_all_labels(stmts: &[Stmt]) -> Vec<String> {
    let mut labels = Vec::new();
    collect_labels_recursive(stmts, &mut labels);
    labels
}

fn collect_labels_recursive(stmts: &[Stmt], out: &mut Vec<String>) {
    animatix_syntax::walk::walk_stmts(stmts, &mut |stmt| {
        if let Stmt::ActorDecl { label, .. } = stmt {
            out.push(label.clone());
        }
    });
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

mod edit_ops;
mod extract;
#[cfg(test)]
mod tests;

pub(crate) use edit_ops::{
    add_scene, delete_scene, duplicate_scene, rename_scene, reorder_scenes, set_play_target,
    set_scene_duration, set_transition,
};
pub(crate) use extract::{extract_scene, move_to_scene};
