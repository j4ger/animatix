//! Renaming an actor and every reference to it.

use animatix_syntax::ast::{InlineItem, Stmt};
use animatix_syntax::walk::{walk_inline_items_mut, walk_stmts_mut};

/// Rename all references to `old_label` into `new_label` throughout the AST.
pub(crate) fn rename_all_references(stmts: &mut [Stmt], old_label: &str, new_label: &str) {
    rename_in_stmt(stmts, old_label, new_label);
}

/// Rename actor references in a statement list using shared walk primitives.
///
/// Uses `walk_stmts_mut` for recursive statement traversal and
/// `walk_inline_items_mut` for inline item children of ActorDecl nodes.
fn rename_in_stmt(stmts: &mut [Stmt], old_label: &str, new_label: &str) {
    walk_stmts_mut(stmts, &mut |stmt| match stmt {
        Stmt::ActorDecl {
            label, children, ..
        } => {
            if label == old_label {
                *label = new_label.to_string();
            }
            walk_inline_items_mut(children, &mut |item| {
                if let InlineItem::Labeled { label, .. } = item {
                    if label == old_label {
                        *label = new_label.to_string();
                    }
                }
            });
        },
        Stmt::Assignment { target, .. } => {
            for part in target.iter_mut() {
                if part.label_str() == old_label {
                    *part = animatix_syntax::ast::TargetSegment::Static(new_label.to_string());
                }
            }
        },
        Stmt::Action(action, _) => {
            for t in action.targets.iter_mut() {
                if t == old_label {
                    *t = new_label.to_string();
                }
            }
        },
        _ => {},
    });
}
