//! Selectable list widgets (H2 + H3 — keyboard nav, type-ahead, searchable).
//!
//! ## H2 — List
//!
//! A vertical selectable list of string items with arrow-key navigation, Home/End,
//! type-ahead (buffer resets after 0.8 s of inactivity), and Enter-to-confirm.
//! Selection and type-ahead state live in egui Memory keyed on the widget's `Id`.
//!
//! ## H3 — SearchableList
//!
//! Wraps a [`TextField`] filter on top of [`List`]. Filters items by case-insensitive
//! substring and returns the *original* (unfiltered) index on confirm.

use std::time::Duration;

use egui::{Id, Sense};

use crate::tokens::spatial::{density, ROW_M};
use crate::widget::input::TextField;
use crate::widget::row::Row;

// ── Public types ─────────────────────────────────────────────────────

/// Action emitted by [`List`] when the user interacts with an item.
pub enum ListAction {
    /// An item was clicked.
    Clicked(usize),
    /// An item was confirmed (Enter key).
    Confirmed(usize),
}

/// Response returned by [`List::show`].
pub struct ListResponse {
    /// Action emitted this frame, if any.
    pub action: Option<ListAction>,
    /// Index of the currently selected item, or `None`.
    pub selected_index: Option<usize>,
}

// ── List ─────────────────────────────────────────────────────────────

/// A vertical selectable list of string items.
///
/// Keyboard controls:
/// - `↑` / `↓` — move selection
/// - `Home` / `End` — first / last item
/// - `Enter` — confirm
/// - Type-ahead — jump to next item matching accumulated prefix (0.8 s timeout)
pub struct List<'a> {
    items: &'a [&'a str],
    row_height: f32,
    type_ahead_timeout: Duration,
}

impl<'a> List<'a> {
    pub fn new(items: &'a [&'a str]) -> Self {
        Self {
            items,
            row_height: ROW_M,
            type_ahead_timeout: Duration::from_secs_f64(0.8),
        }
    }

    /// Row height in pixels (default: [`ROW_M`]).
    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    /// Duration after which the type-ahead buffer resets (default: 800 ms).
    pub fn type_ahead_timeout(mut self, d: Duration) -> Self {
        self.type_ahead_timeout = d;
        self
    }

    /// Render the list.
    pub fn show(self, ui: &mut egui::Ui, id_source: impl std::hash::Hash) -> ListResponse {
        let list_id = Id::new(id_source);
        let num_items = self.items.len();
        let row_h = density(ui).scale(self.row_height);
        let timeout_secs = self.type_ahead_timeout.as_secs_f64();

        // ── Read / initialise Memory ────────────────────────────────
        let mut sel_idx: Option<usize> = ui
            .ctx()
            .data(|d| d.get_temp::<Option<usize>>(list_id.with("sel_idx")))
            .unwrap_or(None);
        if let Some(i) = sel_idx {
            if num_items > 0 && i >= num_items {
                sel_idx = Some(num_items - 1);
            }
        }

        let mut ta_buffer: String = ui
            .ctx()
            .data(|d| d.get_temp::<String>(list_id.with("ta_buf")))
            .unwrap_or_default();
        let mut ta_time: f64 = ui
            .ctx()
            .data(|d| d.get_temp::<f64>(list_id.with("ta_time")))
            .unwrap_or(0.0);
        let now = ui.input(|i| i.time);

        // ── Keyboard events ─────────────────────────────────────────
        let mut action: Option<ListAction> = None;

        ui.input(|i| {
            for ev in &i.events {
                match ev {
                    egui::Event::Key { key: egui::Key::ArrowDown, pressed: true, .. } => {
                        if num_items > 0 {
                            sel_idx = Some(sel_idx.map(|i| (i + 1).min(num_items - 1)).unwrap_or(0));
                        }
                    }
                    egui::Event::Key { key: egui::Key::ArrowUp, pressed: true, .. } => {
                        if num_items > 0 {
                            sel_idx = Some(sel_idx.map(|i| i.saturating_sub(1)).unwrap_or(0));
                        }
                    }
                    egui::Event::Key { key: egui::Key::Home, pressed: true, .. } => {
                        if num_items > 0 { sel_idx = Some(0); }
                    }
                    egui::Event::Key { key: egui::Key::End, pressed: true, .. } => {
                        if num_items > 0 { sel_idx = Some(num_items - 1); }
                    }
                    egui::Event::Key { key: egui::Key::Enter, pressed: true, .. } => {
                        if let Some(idx) = sel_idx {
                            action = Some(ListAction::Confirmed(idx));
                        }
                    }
                    egui::Event::Text(text) => {
                        if now - ta_time > timeout_secs {
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
                                let found = self.items[start..num_items]
                                    .iter()
                                    .enumerate()
                                    .find(|(_, s)| s.to_lowercase().starts_with(&prefix))
                                    .map(|(i, _)| start + i);
                                let found = found.or_else(|| {
                                    self.items[0..start.min(num_items)]
                                        .iter()
                                        .enumerate()
                                        .find(|(_, s)| s.to_lowercase().starts_with(&prefix))
                                        .map(|(i, _)| i)
                                });
                                sel_idx = found;
                            }
                        }
                    }
                    _ => {}
                }
            }
        });

        // ── Allocate & paint ────────────────────────────────────────
        let total_height = num_items as f32 * row_h;
        let (outer_rect, outer_resp) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), total_height), Sense::click());

        if outer_resp.clicked() {
            if let Some(clicked_idx) = row_at(outer_resp.interact_pointer_pos(), outer_rect, row_h) {
                if clicked_idx < num_items {
                    sel_idx = Some(clicked_idx);
                    if action.is_none() {
                        action = Some(ListAction::Clicked(clicked_idx));
                    }
                }
            }
        }

        if !ui.is_rect_visible(outer_rect) {
            persist_list_state(ui.ctx(), list_id, sel_idx, &ta_buffer, ta_time);
            return ListResponse { action, selected_index: sel_idx };
        }

        let painter = ui.painter_at(outer_rect);

        for (idx, label) in self.items.iter().enumerate() {
            let y = outer_rect.min.y + idx as f32 * row_h;
            let row_rect = egui::Rect::from_min_max(
                egui::pos2(outer_rect.min.x, y),
                egui::pos2(outer_rect.max.x, y + row_h),
            );
            let is_selected = sel_idx == Some(idx);
            let row_resp = ui.interact(row_rect, list_id.with(idx), Sense::click());

            let _ = Row::new(label)
                .height(row_h)
                .selected(is_selected)
                .show_in_rect(ui, row_rect, row_resp, list_id.with(idx), &painter);
        }

        persist_list_state(ui.ctx(), list_id, sel_idx, &ta_buffer, ta_time);

        ListResponse { action, selected_index: sel_idx }
    }
}

// ── SearchableList ───────────────────────────────────────────────────

/// A filterable, selectable list: [`TextField`] filter on top + [`List`] below.
///
/// Filtering is case-insensitive substring matching. The `confirmed` / `selected_index`
/// fields in the response carry the **original** (unfiltered) index in the caller's slice.
pub struct SearchableList<'a> {
    items: &'a [&'a str],
    placeholder: Option<&'a str>,
    row_height: f32,
    type_ahead_timeout: Duration,
}

impl<'a> SearchableList<'a> {
    pub fn new(items: &'a [&'a str]) -> Self {
        Self {
            items,
            placeholder: None,
            row_height: ROW_M,
            type_ahead_timeout: Duration::from_secs_f64(0.8),
        }
    }

    /// Placeholder text for the filter field.
    pub fn placeholder(mut self, text: &'a str) -> Self {
        self.placeholder = Some(text);
        self
    }

    /// Row height in pixels.
    pub fn row_height(mut self, h: f32) -> Self {
        self.row_height = h;
        self
    }

    /// Type-ahead timeout duration.
    pub fn type_ahead_timeout(mut self, d: Duration) -> Self {
        self.type_ahead_timeout = d;
        self
    }

    /// Render the searchable list.
    pub fn show(self, ui: &mut egui::Ui, id_source: impl std::hash::Hash) -> SearchableListResponse {
        let root_id = Id::new(id_source);

        // Filter + selection state in Memory.
        let filter: String = ui
            .ctx()
            .data(|d| d.get_temp::<String>(root_id.with("filter")))
            .unwrap_or_default();

        let sel_filtered: Option<usize> = ui
            .ctx()
            .data(|d| d.get_temp::<Option<usize>>(root_id.with("sel_filt")))
            .unwrap_or(None);

        // ── Filter TextField ────────────────────────────────────────
        ui.horizontal(|ui| {
            let mut f = filter.clone();
            let tf = TextField::new(&mut f)
                .placeholder(self.placeholder.unwrap_or("Filter…"))
                .cleanable(true)
                .desired_width(ui.available_width());
            let _resp = tf.show(ui);
        });

        // ── Compute filtered items ──────────────────────────────────
        let lower_filter = filter.to_lowercase();
        let filtered: Vec<(usize, &str)> = self
            .items
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, label)| label.to_lowercase().contains(&lower_filter))
            .collect();

        let filtered_labels: Vec<&str> = filtered.iter().map(|(_, l)| *l).collect();

        // Selection defaults: first filtered item, or None.
        let default_sel: Option<usize> = if filtered_labels.is_empty() {
            None
        } else {
            Some(0)
        };

        // Clamp a persisted selection that may be stale after filter change.
        let sel_filtered: Option<usize> = sel_filtered
            .filter(|i| filtered_labels.is_empty() || *i < filtered_labels.len())
            .or(default_sel);

        // Persist the (possibly clamped) selection so the inner List picks it up.
        ui.ctx().data_mut(|d| {
            d.insert_temp(root_id.with("sel_filt"), sel_filtered);
        });

        // ── Inner List ──────────────────────────────────────────────
        let list_resp = List::new(&filtered_labels)
            .row_height(self.row_height)
            .type_ahead_timeout(self.type_ahead_timeout)
            .show(ui, root_id.with("inner"));

        // Map filtered index → original index.
        let confirmed_original = list_resp.action.and_then(|a| match a {
            ListAction::Confirmed(fi) | ListAction::Clicked(fi) => filtered.get(fi).map(|(oi, _)| *oi),
        });
        let selected_original = list_resp
            .selected_index
            .and_then(|fi| filtered.get(fi).map(|(oi, _)| *oi));

        // ── Persist filter ──────────────────────────────────────────
        ui.ctx().data_mut(|d| {
            d.insert_temp(root_id.with("filter"), filter);
        });

        SearchableListResponse {
            confirmed: confirmed_original,
            selected_index: selected_original,
            filtered_count: filtered_labels.len(),
        }
    }
}

/// Response from [`SearchableList::show`].
pub struct SearchableListResponse {
    /// Original (unfiltered) index of the confirmed item, or `None`.
    pub confirmed: Option<usize>,
    /// Original (unfiltered) index of the currently selected item, or `None`.
    pub selected_index: Option<usize>,
    /// Number of items matching the current filter.
    pub filtered_count: usize,
}

// ── Internal helpers ─────────────────────────────────────────────────

fn row_at(pointer_pos: Option<egui::Pos2>, outer_rect: egui::Rect, row_height: f32) -> Option<usize> {
    let pos = pointer_pos?;
    if !outer_rect.contains(pos) { return None; }
    let rel_y = pos.y - outer_rect.min.y;
    if rel_y < 0.0 { return None; }
    Some((rel_y / row_height).floor() as usize)
}

fn persist_list_state(
    ctx: &egui::Context,
    list_id: Id,
    sel_idx: Option<usize>,
    ta_buffer: &str,
    ta_time: f64,
) {
    ctx.data_mut(|d| {
        d.insert_temp(list_id.with("sel_idx"), sel_idx);
        d.insert_temp(list_id.with("ta_buf"), ta_buffer.to_owned());
        d.insert_temp(list_id.with("ta_time"), ta_time);
    });
}

// ── Pure helper functions (testable without egui) ────────────────────

/// Return the next index in a list of `item_count` items, or `None`.
pub fn next_index(current: usize, item_count: usize) -> Option<usize> {
    if current + 1 < item_count { Some(current + 1) } else { None }
}

/// Return the previous index, or `None` if already at the start.
pub fn prev_index(current: usize) -> Option<usize> {
    if current > 0 { Some(current - 1) } else { None }
}

/// Return `Some(0)` if the list has items, else `None`.
pub fn home_index(item_count: usize) -> Option<usize> {
    if item_count == 0 { None } else { Some(0) }
}

/// Return the last valid index, or `None` for an empty list.
pub fn end_index(item_count: usize) -> Option<usize> {
    if item_count == 0 { None } else { Some(item_count - 1) }
}

/// Case-insensitive prefix match over a `&[&str]`.
///
/// Searches forward from `start` and wraps to the beginning.
/// Returns `None` when `prefix` is empty or no item matches.
pub fn type_ahead_prefix_match(prefix: &str, start: usize, items: &[&str]) -> Option<usize> {
    if prefix.is_empty() || items.is_empty() {
        return None;
    }
    let lower = prefix.to_lowercase();
    let n = items.len();
    if let Some((i, _)) = items[start..n].iter().enumerate().find(|(_, s)| {
        s.to_lowercase().starts_with(&lower)
    }) {
        return Some(start + i);
    }
    if let Some((i, _)) = items[0..start.min(n)].iter().enumerate().find(|(_, s)| {
        s.to_lowercase().starts_with(&lower)
    }) {
        return Some(i);
    }
    None
}

/// Case-insensitive substring filter. Returns `(original_index, &str)` pairs.
pub fn filter_items<'a>(filter: &str, items: &[&'a str]) -> Vec<(usize, &'a str)> {
    if filter.is_empty() {
        return items.iter().copied().enumerate().collect();
    }
    let lower = filter.to_lowercase();
    items
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, s)| s.to_lowercase().contains(&lower))
        .collect()
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const ITEMS: [&str; 5] = ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"];

    #[test]
    fn next_index_basic() {
        assert_eq!(next_index(0, 5), Some(1));
        assert_eq!(next_index(4, 5), None);
    }

    #[test]
    fn prev_index_basic() {
        assert_eq!(prev_index(0), None);
        assert_eq!(prev_index(4), Some(3));
    }

    #[test]
    fn home_end() {
        assert_eq!(home_index(5), Some(0));
        assert_eq!(end_index(5), Some(4));
        assert_eq!(home_index(0), None);
        assert_eq!(end_index(0), None);
    }

    #[test]
    fn type_ahead_exact() {
        assert_eq!(type_ahead_prefix_match("Beta", 0, &ITEMS), Some(1));
    }

    #[test]
    fn type_ahead_case_insensitive() {
        assert_eq!(type_ahead_prefix_match("gamma", 0, &ITEMS), Some(2));
        assert_eq!(type_ahead_prefix_match("DELTA", 0, &ITEMS), Some(3));
    }

    #[test]
    fn type_ahead_wraps() {
        // "Alpha" is at index 0; searching from 1 should wrap.
        assert_eq!(type_ahead_prefix_match("Alpha", 1, &ITEMS), Some(0));
    }

    #[test]
    fn type_ahead_no_match() {
        assert_eq!(type_ahead_prefix_match("ZZZ", 0, &ITEMS), None);
    }

    #[test]
    fn type_ahead_empty_prefix() {
        assert_eq!(type_ahead_prefix_match("", 0, &ITEMS), None);
    }

    #[test]
    fn filter_items_empty_returns_all() {
        let r = filter_items("", &ITEMS);
        assert_eq!(r.len(), 5);
        assert_eq!(r[0], (0, "Alpha"));
    }

    #[test]
    fn filter_items_substring() {
        // "ta" appears in Beta (B**ta**) and Delta (De**ta**)
        let r = filter_items("ta", &ITEMS);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], (1, "Beta"));
        assert_eq!(r[1], (3, "Delta"));
    }

    #[test]
    fn filter_items_case_insensitive() {
        let r = filter_items("DELTA", &ITEMS);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0], (3, "Delta"));
    }

    #[test]
    fn filter_items_no_match() {
        assert!(filter_items("ZZZ", &ITEMS).is_empty());
    }

    #[test]
    fn filter_preserves_original_indices() {
        // "a" is in Alpha (0), Beta (1), Gamma (2), Delta (3)
        let r = filter_items("a", &ITEMS);
        assert_eq!(r.len(), 4);
        assert_eq!(r[0], (0, "Alpha"));
        assert_eq!(r[1], (1, "Beta"));
        assert_eq!(r[2], (2, "Gamma"));
        assert_eq!(r[3], (3, "Delta"));
    }

    #[test]
    fn builder_row_height() {
        let list = List::new(&ITEMS).row_height(30.0);
        assert_eq!(list.row_height, 30.0);
    }

    #[test]
    fn builder_type_ahead_timeout() {
        let d = Duration::from_secs(2);
        let list = List::new(&ITEMS).type_ahead_timeout(d);
        assert_eq!(list.type_ahead_timeout, d);
    }
}
