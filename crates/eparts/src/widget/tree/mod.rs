//! Tree-view widget (H1 — flat-entry model, generic, no animatix imports).
//!
//! ## Flat-entry model
//!
//! The caller owns all domain data and rebuilds a *flattened* `Vec<TreeItem>` each
//! frame. Each `TreeItem` carries depth, expanded flag, and a stable identity. The
//! Tree widget renders the visible rows, handles arrow-key navigation, and returns
//! [`TreeAction`] events so the caller can update its own state.
//!
//! ```text
//! caller state:
//!   expanded: HashSet<Id>   // which nodes are open
//!   selected: Option<Id>    // currently selected node id
//!
//! each frame:
//!   1. walk the domain tree → flat Vec<TreeItem> (include children of expanded nodes)
//!   2. Tree::show(ui, &flat_list, id) → TreeResponse { action, ... }
//!   3. handle action: toggle expanded / update selected
//!   4. rebuild flat list (step 1) for next frame
//! ```
//!
//! ## Keyboard navigation
//!
//! | Key          | Action                                      |
//! |--------------|---------------------------------------------|
//! | `↑` / `↓`   | Move selection up/down                      |
//! | `Home`       | Select first visible item                   |
//! | `End`        | Select last visible item                    |
//! | `Left`       | Collapse (if expanded) / move to parent     |
//! | `Right`      | Expand (if collapsed) / move to first child |
//! | `Enter`      | Toggle expand/collapse                      |
//! | type-ahead   | Jump to next item matching typed prefix     |
//!
//! ## Usage
//!
//! ```ignore
//! let mut expanded: HashSet<String> = HashSet::new();
//! let mut selected: Option<String> = None;
//!
//! // Build flat list each frame:
//! let flat_items = build_flat_list(&root_node, &expanded);
//!
//! let resp = eparts::Tree::new(&flat_items)
//!     .show(ui, egui::Id::new("my_tree"));
//!
//! if let Some(action) = resp.action {
//!     match action {
//!         eparts::TreeAction::Toggled(id) => {
//!             if expanded.contains(&id) { expanded.remove(&id); }
//!             else { expanded.insert(id); }
//!         }
//!         eparts::TreeAction::Selected(id) => { selected = Some(id); }
//!     }
//! }
//! ```

#[allow(unused_imports)]
use std::collections::HashSet;

use egui::{Id, Response, Sense};

use crate::tokens::spatial::{ROW_M, SPACE_4, density};
use crate::widget::row::Row;

// ── Public types ─────────────────────────────────────────────────────

/// A stable identity for a tree node (caller-supplied, unique across the tree).
pub type TreeId = String;

/// One flattened, visible row in the tree.
///
/// The caller builds this list each frame by walking the domain tree and
/// including children of any node whose `expanded` flag is set.
pub struct TreeItem {
    /// Stable identity (any display type; `String` keeps this crate free of animatix types).
    pub id: TreeId,
    /// Display label.
    pub label: String,
    /// Visual indent level (0 for root).
    pub depth: u32,
    /// Whether this node has children.
    pub has_children: bool,
    /// Whether this node is currently expanded (children are visible below it).
    pub expanded: bool,
}

/// Action emitted by the [`Tree`] widget when the user interacts.
pub enum TreeAction {
    /// A node was toggled — caller should flip its expanded state and rebuild the flat list.
    Toggled(TreeId),
    /// A node was selected — caller should update its selection.
    Selected(TreeId),
}

/// Response returned by [`Tree::show`].
pub struct TreeResponse {
    /// Action emitted this frame, if any.
    pub action: Option<TreeAction>,
    /// The egui `Response` for the tree's whole allocated region (attach context
    /// menus etc.). This is the *full content* rect — taller than the widget when
    /// the tree scrolls — so it must not be used for "is the pointer over the
    /// visible tree" hit tests; use [`Self::viewport_response`] for that.
    pub response: Response,
    /// The egui `Response` for the visible scroll viewport only.
    ///
    /// Added in addition to [`Self::response`] (whose meaning is unchanged for
    /// backward compatibility). Use this when the distinction matters — e.g.
    /// context menus or drop targets that should only react to the on-screen area.
    pub viewport_response: Response,
    /// Index of the selected item within the flat list, or `None`.
    pub selected_index: Option<usize>,
}

// ── Builder ──────────────────────────────────────────────────────────

/// A generic, flat-entry tree widget.
///
/// All state lives either in caller-owned structs (expanded set, selection) or
/// in egui Memory. The widget itself is constructed fresh each frame.
pub struct Tree<'a> {
    items: &'a [TreeItem],
    row_height: f32,
    indent_step: f32,
}

fn resolve_tree_action(
    toggled_id: Option<TreeId>,
    selected_id: Option<TreeId>,
) -> Option<TreeAction> {
    if let Some(id) = toggled_id {
        return Some(TreeAction::Toggled(id));
    }
    if let Some(id) = selected_id {
        return Some(TreeAction::Selected(id));
    }
    None
}

pub fn nav_next_idx(current: usize, items: &[TreeItem]) -> Option<usize> {
    items.get(current + 1).map(|_| current + 1)
}

/// Return the previous valid index before `current`, or `None` at start-of-list.
pub fn nav_prev_idx(current: usize, _items: &[TreeItem]) -> Option<usize> {
    if current > 0 { Some(current - 1) } else { None }
}

/// Return `Some(0)` if the list is non-empty, else `None`.
pub fn nav_home_idx(items: &[TreeItem]) -> Option<usize> {
    if items.is_empty() { None } else { Some(0) }
}

/// Return the last valid index, or `None` if the list is empty.
pub fn nav_end_idx(items: &[TreeItem]) -> Option<usize> {
    items.last().map(|_| items.len() - 1)
}

/// Find the next item whose label starts with `prefix` (case-insensitive),
/// searching forward from `start_idx` and wrapping to the beginning.
///
/// Returns `None` when no item matches or the list is empty.
pub fn type_ahead_match(prefix: &str, start_idx: usize, items: &[TreeItem]) -> Option<usize> {
    if prefix.is_empty() || items.is_empty() {
        return None;
    }
    let lower = prefix.to_lowercase();
    let n = items.len();
    if let Some((i, _)) = items[start_idx..n]
        .iter()
        .enumerate()
        .find(|(_, item)| item.label.to_lowercase().starts_with(&lower))
    {
        return Some(start_idx + i);
    }
    if let Some((i, _)) = items[0..start_idx.min(n)]
        .iter()
        .enumerate()
        .find(|(_, item)| item.label.to_lowercase().starts_with(&lower))
    {
        return Some(i);
    }
    None
}

#[cfg(test)]
mod tests;
mod tree_impl;
