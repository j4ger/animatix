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

use crate::tokens::spatial::{ROW_M, density};
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

// ── Internal helpers ─────────────────────────────────────────────────

fn row_at(
    pointer_pos: Option<egui::Pos2>,
    outer_rect: egui::Rect,
    row_height: f32,
) -> Option<usize> {
    let pos = pointer_pos?;
    if !outer_rect.contains(pos) {
        return None;
    }
    let rel_y = pos.y - outer_rect.min.y;
    if rel_y < 0.0 {
        return None;
    }
    Some((rel_y / row_height).floor() as usize)
}

/// The screen rect of row `idx` within a full-content `outer_rect`.
pub(crate) fn row_rect_for(outer_rect: egui::Rect, row_height: f32, idx: usize) -> egui::Rect {
    let y = outer_rect.min.y + idx as f32 * row_height;
    egui::Rect::from_min_max(
        egui::pos2(outer_rect.min.x, y),
        egui::pos2(outer_rect.max.x, y + row_height),
    )
}

/// Row indices intersecting a scroll viewport.
///
/// `viewport` is content-relative (`viewport.min == ZERO` at the top of the
/// content, per [`egui::ScrollArea::show_viewport`]). One row of overscan is
/// added on each side so partially scrolled rows still paint.
pub(crate) fn visible_row_range(
    viewport: egui::Rect,
    row_height: f32,
    count: usize,
) -> std::ops::Range<usize> {
    if count == 0 || row_height <= 0.0 {
        return 0..0;
    }
    let first = ((viewport.min.y / row_height).floor().max(0.0) as usize).min(count);
    let last = (((viewport.max.y / row_height).ceil() as usize) + 1).min(count);
    first..last.max(first)
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
    if current + 1 < item_count {
        Some(current + 1)
    } else {
        None
    }
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
    if item_count == 0 {
        None
    } else {
        Some(item_count - 1)
    }
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
    if let Some((i, _)) = items[start..n]
        .iter()
        .enumerate()
        .find(|(_, s)| s.to_lowercase().starts_with(&lower))
    {
        return Some(start + i);
    }
    if let Some((i, _)) = items[0..start.min(n)]
        .iter()
        .enumerate()
        .find(|(_, s)| s.to_lowercase().starts_with(&lower))
    {
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

mod list_impl;
mod searchable;
#[cfg(test)]
mod tests;
pub use searchable::SearchableListResponse;
