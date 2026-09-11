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

impl<'a> Tree<'a> {
    pub fn new(items: &'a [TreeItem]) -> Self {
        Self {
            items,
            row_height: ROW_M,
            indent_step: SPACE_4,
        }
    }

    /// Row height in pixels (default: [`ROW_M`]).
    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    /// Indent increment per depth level in pixels (default: [`SPACE_4`]).
    pub fn indent_step(mut self, px: f32) -> Self {
        self.indent_step = px;
        self
    }

    /// Render the tree and return [`TreeResponse`].
    ///
    /// `id_source` seeds the `egui::Id` that guards the widget's Memory entries.
    ///
    /// The body is wrapped in a vertical [`egui::ScrollArea`] and only rows
    /// intersecting the scroll viewport are painted / interacted with
    /// (virtualized). The full-height space is still allocated inside the
    /// scroll area so the scrollbar range reflects every row.
    pub fn show(self, ui: &mut egui::Ui, id_source: impl std::hash::Hash) -> TreeResponse {
        let tree_id = Id::new(id_source);
        let row_h = density(ui).scale(self.row_height);
        let indent = density(ui).scale(self.indent_step);
        let scroll_height = ui.available_height();

        let output = egui::ScrollArea::vertical()
            .id_salt(tree_id.with("__scroll"))
            .auto_shrink([false, true])
            .max_height(scroll_height)
            .show_viewport(ui, |ui, viewport| {
                self.show_inner(ui, tree_id, row_h, indent, viewport)
            });

        let (action, response, viewport_response, selected_index) = output.inner;
        TreeResponse {
            action,
            response,
            viewport_response,
            selected_index,
        }
    }

    /// Inner render: allocates the full content rect and paints the visible
    /// window. Split out so the scroll area can wrap it.
    ///
    /// Returns `(action, content_response, viewport_response, selected_index)`.
    fn show_inner(
        self,
        ui: &mut egui::Ui,
        tree_id: Id,
        row_h: f32,
        indent: f32,
        viewport: egui::Rect,
    ) -> (Option<TreeAction>, Response, Response, Option<usize>) {
        let num_items = self.items.len();

        // ── Read / initialise Memory state ──────────────────────────
        let mut sel_idx: Option<usize> = ui
            .ctx()
            .data(|d| d.get_temp::<Option<usize>>(tree_id.with("sel_idx")))
            .unwrap_or(None);

        // Clamp stale selection to the current list length.
        if let Some(idx) = sel_idx {
            if num_items == 0 {
                sel_idx = None;
            } else if idx >= num_items {
                sel_idx = Some(num_items - 1);
            }
        }

        // Type-ahead buffer + last-keystroke time (lives in Memory).
        let mut ta_buffer: String = ui
            .ctx()
            .data(|d| d.get_temp::<String>(tree_id.with("ta_buf")))
            .unwrap_or_default();
        let mut ta_time: f64 =
            ui.ctx().data(|d| d.get_temp::<f64>(tree_id.with("ta_time"))).unwrap_or(0.0);
        let now = ui.input(|i| i.time);
        const TYPE_AHEAD_TIMEOUT: f64 = 0.8;

        // ── Accumulate keyboard events ──────────────────────────────
        let mut toggled_id: Option<TreeId> = None;
        let mut selected_id: Option<TreeId> = None;
        // Set whenever navigation (keyboard or click) changes the selection, so
        // the selected row can be scrolled into view after painting.
        let mut nav_moved: bool;

        // ── Allocate the outer frame & resolve focus ────────────────
        // The outer response owns keyboard focus, so allocate before reading
        // events. Interact under `tree_id` (not an auto id) so focus and the
        // focus lock filter refer to the same stable id.
        let total_height = num_items as f32 * row_h;
        let (_, outer_rect) = ui.allocate_space(egui::vec2(ui.available_width(), total_height));
        let outer_resp = ui.interact(outer_rect, tree_id, Sense::click());
        // Response for the on-screen scroll viewport only. Registered *before*
        // the rows so it never steals their hover/click (later widgets sit on
        // top), but still covers the empty area below the last row.
        let viewport_resp = ui.interact(
            outer_rect.intersect(ui.clip_rect()),
            tree_id.with("__viewport"),
            Sense::hover(),
        );
        let mut has_focus = outer_resp.has_focus();
        if outer_resp.clicked() {
            ui.memory_mut(|m| m.request_focus(tree_id));
            has_focus = true;
        }
        if has_focus {
            // Arrow keys navigate the tree; keep egui's focus traversal from
            // also moving focus to a sibling widget on the same key press.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    tree_id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        tab: false,
                        escape: false,
                    },
                );
            });
        }

        // ── Keyboard events (focused only, consumed) ────────────────
        let sel_before_kb = sel_idx;
        if has_focus {
            ui.input_mut(|i| {
                i.events.retain(|ev| match ev {
                    egui::Event::Key {
                        key: egui::Key::ArrowDown,
                        pressed: true,
                        ..
                    } if num_items > 0 => {
                        let next = sel_idx.map(|i| (i + 1).min(num_items - 1)).unwrap_or(0);
                        sel_idx = Some(next);
                        selected_id = Some(self.items[next].id.clone());
                        false
                    },
                    egui::Event::Key {
                        key: egui::Key::ArrowUp,
                        pressed: true,
                        ..
                    } if num_items > 0 => {
                        let prev = sel_idx.map(|i| i.saturating_sub(1)).unwrap_or(0);
                        sel_idx = Some(prev);
                        selected_id = Some(self.items[prev].id.clone());
                        false
                    },
                    egui::Event::Key {
                        key: egui::Key::Home,
                        pressed: true,
                        ..
                    } if num_items > 0 => {
                        sel_idx = Some(0);
                        selected_id = Some(self.items[0].id.clone());
                        false
                    },
                    egui::Event::Key {
                        key: egui::Key::End,
                        pressed: true,
                        ..
                    } if num_items > 0 => {
                        sel_idx = Some(num_items - 1);
                        selected_id = Some(self.items[num_items - 1].id.clone());
                        false
                    },
                    egui::Event::Key {
                        key: egui::Key::ArrowLeft,
                        pressed: true,
                        ..
                    } => {
                        if let Some(idx) = sel_idx {
                            if let Some(item) = self.items.get(idx) {
                                if item.expanded && item.has_children {
                                    toggled_id = Some(item.id.clone());
                                } else if item.depth > 0 {
                                    // Walk backward to nearest ancestor.
                                    let target_depth = item.depth.saturating_sub(1);
                                    let mut parent_idx = idx;
                                    while parent_idx > 0 {
                                        parent_idx -= 1;
                                        if let Some(cand) = self.items.get(parent_idx) {
                                            if cand.depth == target_depth {
                                                sel_idx = Some(parent_idx);
                                                selected_id = Some(cand.id.clone());
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        false
                    },
                    egui::Event::Key {
                        key: egui::Key::ArrowRight,
                        pressed: true,
                        ..
                    } => {
                        if let Some(idx) = sel_idx {
                            if let Some(item) = self.items.get(idx) {
                                if !item.expanded && item.has_children {
                                    toggled_id = Some(item.id.clone());
                                } else if item.expanded && item.has_children {
                                    let child_idx = idx + 1;
                                    if child_idx < num_items
                                        && self.items[child_idx].depth == item.depth + 1
                                    {
                                        sel_idx = Some(child_idx);
                                        selected_id = Some(self.items[child_idx].id.clone());
                                    }
                                }
                            }
                        }
                        false
                    },
                    egui::Event::Key {
                        key: egui::Key::Enter,
                        pressed: true,
                        ..
                    } => {
                        if let Some(idx) = sel_idx {
                            if let Some(item) = self.items.get(idx) {
                                if item.has_children {
                                    toggled_id = Some(item.id.clone());
                                }
                            }
                        }
                        false
                    },
                    egui::Event::Text(text) => {
                        // Type-ahead: reset buffer on timeout.
                        if now - ta_time > TYPE_AHEAD_TIMEOUT {
                            ta_buffer.clear();
                        }
                        let ch = text.trim();
                        if !ch.is_empty() && !text.contains(|c: char| c.is_control()) {
                            ta_buffer.push_str(ch);
                            ta_buffer.truncate(64);
                            ta_time = now;

                            if !ta_buffer.is_empty() && num_items > 0 {
                                let prefix = ta_buffer.to_lowercase();
                                let start = sel_idx.map(|i| i + 1).unwrap_or(0);
                                let mut found = None;
                                for i in start..num_items {
                                    if self.items[i].label.to_lowercase().starts_with(&prefix) {
                                        found = Some(i);
                                        break;
                                    }
                                }
                                if found.is_none() {
                                    for i in 0..start.min(num_items) {
                                        if self.items[i].label.to_lowercase().starts_with(&prefix) {
                                            found = Some(i);
                                            break;
                                        }
                                    }
                                }
                                if let Some(idx) = found {
                                    sel_idx = Some(idx);
                                    selected_id = Some(self.items[idx].id.clone());
                                }
                            }
                        }
                        false
                    },
                    _ => true,
                });
            });
        }
        nav_moved = sel_idx != sel_before_kb;

        // ── Paint rows (virtualized to the visible viewport) ────────
        if ui.is_rect_visible(outer_rect) {
            // `viewport` is content-relative (min == ZERO at the top), matching
            // row offsets from `outer_rect.min.y`.
            let window = crate::widget::list::visible_row_range(viewport, row_h, num_items);

            // Clip row painting to the intersection of the content rect and the
            // scroll viewport; `painter_at(outer_rect)` alone would clip only to
            // the parent's clip rect and paint outside the scroll view.
            let clip = outer_rect.intersect(ui.clip_rect());
            let painter = ui.painter_at(clip);

            for idx in window.clone() {
                let item = &self.items[idx];
                let row_rect = crate::widget::list::row_rect_for(outer_rect, row_h, idx);
                let is_selected = sel_idx == Some(idx);
                let row_id = tree_id.with(idx);

                let row_resp = ui.interact(row_rect, row_id, Sense::click());
                if row_resp.clicked() {
                    sel_idx = Some(idx);
                    selected_id = Some(item.id.clone());
                    nav_moved = true;
                    // Rows sit above the outer frame in the hit-test order, so a
                    // row click is what claims keyboard focus for the tree.
                    ui.memory_mut(|m| m.request_focus(tree_id));
                }

                let _ = Row::new(&item.label)
                    .height(row_h)
                    .indent(item.depth as f32 * indent)
                    .zebra(idx % 2 == 0)
                    .has_children(item.has_children)
                    .expanded(item.expanded)
                    .selected(is_selected)
                    .show_in_rect(ui, row_rect, row_resp, row_id, &painter);
            }

            // Keyboard navigation moved the selection: bring it into view.
            if nav_moved {
                if let Some(idx) = sel_idx {
                    ui.scroll_to_rect(
                        crate::widget::list::row_rect_for(outer_rect, row_h, idx),
                        None,
                    );
                }
            }
        }

        // ── Persist Memory state ────────────────────────────────────
        ui.ctx().data_mut(|d| {
            d.insert_temp(tree_id.with("sel_idx"), sel_idx);
            d.insert_temp(tree_id.with("has_focus"), has_focus);
            d.insert_temp(tree_id.with("ta_buf"), ta_buffer);
            d.insert_temp(tree_id.with("ta_time"), ta_time);
        });

        (resolve_tree_action(toggled_id, selected_id), outer_resp, viewport_resp, sel_idx)
    }
}

// ── Internal helpers ─────────────────────────────────────────────────

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
mod tests {
    use super::*;

    /// Simulate what a caller would produce: a flat list of 5 items
    /// with expanded flags set according to the given set.
    fn build_flat(expanded: &HashSet<String>) -> Vec<TreeItem> {
        vec![
            TreeItem {
                id: "a".into(),
                label: "A".into(),
                depth: 0,
                has_children: true,
                expanded: expanded.contains("a"),
            },
            TreeItem {
                id: "a1".into(),
                label: "A.1".into(),
                depth: 1,
                has_children: false,
                expanded: false,
            },
            TreeItem {
                id: "a2".into(),
                label: "A.2".into(),
                depth: 1,
                has_children: true,
                expanded: expanded.contains("a2"),
            },
            TreeItem {
                id: "a2i".into(),
                label: "A.2.i".into(),
                depth: 2,
                has_children: false,
                expanded: false,
            },
            TreeItem {
                id: "b".into(),
                label: "B".into(),
                depth: 0,
                has_children: false,
                expanded: false,
            },
        ]
    }

    #[test]
    fn flat_list_includes_children_when_expanded() {
        // When both A and A.2 are in the expanded set, all 5 items are present.
        let expanded: HashSet<String> = ["a".into(), "a2".into()].iter().cloned().collect();
        let flat = build_flat(&expanded);
        assert_eq!(flat.len(), 5);
        assert_eq!(flat[0].id, "a");
        assert_eq!(flat[3].id, "a2i");
        assert_eq!(flat[4].id, "b");
    }

    #[test]
    fn flat_list_all_items_always_present() {
        // build_flat always returns all 5 items; the caller controls which
        // are visible by setting expanded flags on their parents.
        let flat = build_flat(&HashSet::new());
        assert_eq!(flat.len(), 5);
    }

    #[test]
    fn expanded_flags_reflect_expanded_set() {
        let expanded: HashSet<String> = ["a".into()].iter().cloned().collect();
        let flat = build_flat(&expanded);
        assert!(flat[0].expanded); // A in expanded set
        assert!(!flat[2].expanded); // A.2 not in set
    }

    #[test]
    fn root_nodes_are_depth_zero() {
        let flat = build_flat(&HashSet::new());
        let roots: Vec<_> = flat.iter().filter(|i| i.depth == 0).collect();
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].id, "a");
        assert_eq!(roots[1].id, "b");
    }

    #[test]
    fn tree_walk_order_preserved() {
        let flat = build_flat(&HashSet::new());
        assert_eq!(flat[0].id, "a"); // first root
        assert_eq!(flat[4].id, "b"); // last root
    }

    // ── Pure navigation helper tests ────────────────────────────────

    #[test]
    fn nav_next_prev_edge() {
        let items = build_flat(&HashSet::new());
        assert_eq!(super::nav_next_idx(0, &items), Some(1));
        assert_eq!(super::nav_prev_idx(0, &items), None);
        assert_eq!(super::nav_next_idx(items.len() - 1, &items), None);
        assert_eq!(super::nav_prev_idx(items.len() - 1, &items), Some(3));
    }

    #[test]
    fn nav_home_end() {
        let items = build_flat(&HashSet::new());
        assert_eq!(super::nav_home_idx(&items), Some(0));
        assert_eq!(super::nav_end_idx(&items), Some(4));
        assert_eq!(super::nav_home_idx(&[]), None);
        assert_eq!(super::nav_end_idx(&[]), None);
    }

    #[test]
    fn type_ahead_exact_match() {
        let items = build_flat(&HashSet::new());
        assert_eq!(super::type_ahead_match("A.1", 0, &items), Some(1));
    }

    #[test]
    fn type_ahead_case_insensitive() {
        let items = build_flat(&HashSet::new());
        assert_eq!(super::type_ahead_match("b", 0, &items), Some(4));
    }

    #[test]
    fn type_ahead_wraps() {
        let items = build_flat(&HashSet::new());
        // "A.1" (index 1) matches prefix "A" from start=1.
        assert_eq!(super::type_ahead_match("A", 1, &items), Some(1));
        // Searching from 4 (item "B") wraps to index 0 ("A").
        assert_eq!(super::type_ahead_match("A", 4, &items), Some(0));
    }

    #[test]
    fn type_ahead_no_match() {
        let items = build_flat(&HashSet::new());
        assert_eq!(super::type_ahead_match("ZZZ", 0, &items), None);
    }

    #[test]
    fn type_ahead_empty_prefix() {
        let items = build_flat(&HashSet::new());
        assert_eq!(super::type_ahead_match("", 0, &items), None);
    }

    #[test]
    fn tree_builder_row_height() {
        let items: Vec<TreeItem> = vec![];
        let tree = Tree::new(&items).row_height(32.0);
        assert_eq!(tree.row_height, 32.0);
    }

    #[test]
    fn tree_builder_indent_step() {
        let items: Vec<TreeItem> = vec![];
        let tree = Tree::new(&items).indent_step(24.0);
        assert_eq!(tree.indent_step, 24.0);
    }

    // ── Keyboard focus gating ───────────────────────────────────────

    fn screen() -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))
    }

    fn base_input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(screen()),
            ..Default::default()
        }
    }

    fn arrow_down() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::ArrowDown,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        }
    }

    fn run_tree_frame(
        ctx: &egui::Context,
        input: egui::RawInput,
        id: &str,
        items: &[TreeItem],
    ) -> (Option<usize>, Vec<egui::Event>) {
        let mut selected = None;
        let mut remaining = Vec::new();
        let _ = ctx.run_ui(input, |ui| {
            selected = Tree::new(items).show(ui, id).selected_index;
            remaining = ui.input(|i| i.events.clone());
        });
        (selected, remaining)
    }

    #[test]
    fn tree_keyboard_ignored_when_unfocused() {
        let ctx = egui::Context::default();
        let items = build_flat(&HashSet::new());

        let mut input = base_input();
        input.events.push(arrow_down());
        let (sel, remaining) = run_tree_frame(&ctx, input, "unfocused_tree", &items);

        assert_eq!(sel, None, "unfocused tree must not react to arrow keys");
        assert!(
            remaining.iter().any(|e| matches!(
                e,
                egui::Event::Key {
                    key: egui::Key::ArrowDown,
                    ..
                }
            )),
            "unfocused tree must not consume the arrow key"
        );
    }

    #[test]
    fn tree_keyboard_navigates_when_focused_and_consumes_event() {
        let ctx = egui::Context::default();
        let tree_id = Id::new("focused_tree");
        let items = build_flat(&HashSet::new());

        ctx.memory_mut(|m| m.request_focus(tree_id));

        let mut input = base_input();
        input.events.push(arrow_down());
        let (sel, remaining) = run_tree_frame(&ctx, input, "focused_tree", &items);

        assert_eq!(sel, Some(0), "focused tree should select the first item");
        assert!(
            !remaining.iter().any(|e| matches!(
                e,
                egui::Event::Key {
                    key: egui::Key::ArrowDown,
                    ..
                }
            )),
            "focused tree must consume the arrow key it handled"
        );
    }

    /// A tree taller than its viewport must scroll rather than growing to the
    /// full content height, and `viewport_response` must describe only the
    /// visible slice (distinct from the full-content `response`).
    #[test]
    fn tree_scrolls_and_exposes_viewport_response() {
        let ctx = egui::Context::default();
        let items: Vec<TreeItem> = (0..50)
            .map(|i| TreeItem {
                id: format!("n{i}"),
                label: format!("Node {i}"),
                depth: 0,
                has_children: false,
                expanded: false,
            })
            .collect();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 120.0));

        let mut used_height = 0.0;
        let mut content_rect = egui::Rect::NOTHING;
        let mut viewport_rect = egui::Rect::NOTHING;
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                let resp = Tree::new(&items).row_height(24.0).show(ui, "tall_tree");
                content_rect = resp.response.rect;
                viewport_rect = resp.viewport_response.rect;
                used_height = ui.min_rect().height();
            },
        );

        assert!(
            used_height <= 160.0,
            "scrolling tree should be capped to the viewport, got {used_height}"
        );
        assert!(
            content_rect.height() > viewport_rect.height() + 1.0,
            "content response ({}px) should be taller than the viewport response ({}px)",
            content_rect.height(),
            viewport_rect.height()
        );
    }

    /// Regression: the viewport interaction must be registered *before* the row
    /// interactions, otherwise it sits on top of them and swallows their hover.
    #[test]
    fn tree_row_hover_survives_viewport_response() {
        let ctx = egui::Context::default();
        let items = build_flat(&HashSet::new());
        let pointer = screen().left_top() + egui::vec2(30.0, 5.0);

        // Warm-up so widget rects are registered for hit-testing.
        let _ = ctx.run_ui(base_input(), |ui| {
            let _ = Tree::new(&items).show(ui, "hover_tree");
        });

        let mut row_hovered = None;
        let mut input = base_input();
        input.events.push(egui::Event::PointerMoved(pointer));
        let _ = ctx.run_ui(input, |ui| {
            let _ = Tree::new(&items).show(ui, "hover_tree");
            row_hovered =
                ui.ctx().read_response(Id::new("hover_tree").with(0)).map(|r| r.hovered());
        });
        assert_eq!(row_hovered, Some(true), "viewport_response must not steal row hover");
    }

    /// A tree that fits inside the viewport: content and viewport responses
    /// cover the same on-screen region, which is only as tall as the rows.
    #[test]
    fn tree_viewport_is_on_screen_when_content_fits() {
        let ctx = egui::Context::default();
        let items = build_flat(&HashSet::new());
        let mut content_rect = egui::Rect::NOTHING;
        let mut viewport_rect = egui::Rect::NOTHING;
        let mut selected = None;
        let _ = ctx.run_ui(base_input(), |ui| {
            let resp = Tree::new(&items).show(ui, "short_tree");
            content_rect = resp.response.rect;
            viewport_rect = resp.viewport_response.rect;
            selected = resp.selected_index;
        });
        assert_eq!(selected, None);
        // Content is exactly the 5 rows; the viewport response covers the same
        // on-screen region (no scrolling needed, so it isn't padded out).
        assert_eq!(content_rect.height(), 5.0 * ROW_M);
        assert!(
            (viewport_rect.height() - content_rect.height()).abs() <= 1.0,
            "fitting tree: viewport ({}) should match content ({})",
            viewport_rect.height(),
            content_rect.height()
        );
        assert!(
            screen().contains_rect(viewport_rect),
            "viewport response must stay within the screen: {viewport_rect:?}"
        );
    }

    #[test]
    fn tree_row_click_claims_focus() {
        let ctx = egui::Context::default();
        let items = build_flat(&HashSet::new());
        let row_pos = screen().left_top() + egui::vec2(20.0, 5.0);

        // Warm-up frame registers widget rects for hit-testing.
        let _ = ctx.run_ui(base_input(), |ui| {
            let _ = Tree::new(&items).show(ui, "click_tree");
        });

        let mut sel = None;
        let mut input = base_input();
        input.events.push(egui::Event::PointerButton {
            pos: row_pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        input.events.push(egui::Event::PointerButton {
            pos: row_pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        });
        let _ = ctx.run_ui(input, |ui| {
            sel = Tree::new(&items).show(ui, "click_tree").selected_index;
        });

        assert_eq!(sel, Some(0), "clicking a row should select it");
        assert!(
            ctx.memory(|m| m.has_focus(Id::new("click_tree"))),
            "clicking a tree row must claim keyboard focus for the tree"
        );
    }
}
