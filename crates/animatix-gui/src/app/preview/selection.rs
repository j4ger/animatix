//! Selection system for the preview canvas.
//!
//! Handles hover preview, click cycling through overlapping actors,
//! and right-click context menu for explicit selection.

use std::collections::HashSet;

use animatix::timeline::animation_track::CalloutPlace;
use egui::Pos2;

use crate::app::components::context_menu::{MenuEntry, render_floating_menu};

// ─── Selection State ────────────────────────────────────────────────────────

/// State for the selection system in the preview canvas.
#[derive(Debug, Default)]
pub(crate) struct SelectionState {
    /// Actor currently hovered in preview (for hover highlight).
    pub(crate) hovered_actor: Option<String>,
    /// Actors under the last click position (for click cycling).
    pub(crate) click_candidates: Vec<String>,
    /// Current index into click_candidates for cycling.
    pub(crate) cycle_index: usize,
    /// Last click position in scene coordinates (to detect same-position clicks).
    pub(crate) last_click_scene: Option<kurbo::Point>,
    /// Whether the right-click context menu is open.
    pub(crate) context_menu_open: bool,
    /// Position for the right-click context menu (screen coordinates).
    pub(crate) context_menu_pos: Option<Pos2>,
    /// Actors at the right-click position for the context menu.
    pub(crate) context_menu_actors: Vec<String>,
    /// Marquee selection: screen-space start point (set on drag start over empty canvas).
    pub(crate) marquee_start: Option<Pos2>,
    /// Marquee selection: current screen-space pointer position during drag.
    pub(crate) marquee_current: Option<Pos2>,
    /// Place handle tapped in preview (for callout place highlight).
    pub(crate) tapped_place: Option<CalloutPlace>,
    /// Actor that owns the tapped place handle.
    pub(crate) tapped_place_actor: Option<String>,
}

impl SelectionState {
    /// Clear the tapped-place highlight state.
    pub(crate) fn clear_tapped_place(&mut self) {
        self.tapped_place = None;
        self.tapped_place_actor = None;
    }
}

// ─── Helper Functions ───────────────────────────────────────────────────────

/// Returns all actors at the given scene point, ordered from topmost (last rendered) to bottommost.
pub(crate) fn actors_at_point(
    hit_regions: &[(String, kurbo::Rect)],
    point: kurbo::Point,
) -> Vec<String> {
    hit_regions
        .iter()
        .rev()
        .filter(|(_, bounds)| bounds.contains(point))
        .map(|(label, _)| label.clone())
        .collect()
}

/// Check if two scene points are close enough to be considered "same position".
pub(super) fn is_same_position(a: kurbo::Point, b: kurbo::Point, tolerance: f64) -> bool {
    let dx = (a.x - b.x).abs();
    let dy = (a.y - b.y).abs();
    dx < tolerance && dy < tolerance
}

// ─── Selection Logic ────────────────────────────────────────────────────────

/// Update hover state based on current pointer position.
pub(crate) fn update_hover(
    selection: &mut SelectionState,
    hit_regions: &[(String, kurbo::Rect)],
    pointer_pos: Option<Pos2>,
    screen_to_scene: impl Fn(Pos2) -> kurbo::Point,
    is_dragging: bool,
) {
    if is_dragging {
        selection.hovered_actor = None;
        return;
    }

    if let Some(mouse) = pointer_pos {
        let scene_point = screen_to_scene(mouse);
        let candidates = actors_at_point(hit_regions, scene_point);
        selection.hovered_actor = candidates.first().cloned();
    } else {
        selection.hovered_actor = None;
    }
}

/// Handle right-click to open context menu.
pub(crate) fn handle_right_click(
    selection: &mut SelectionState,
    hit_regions: &[(String, kurbo::Rect)],
    click_pos: Pos2,
    screen_to_scene: impl Fn(Pos2) -> kurbo::Point,
) {
    let scene_point = screen_to_scene(click_pos);
    let candidates = actors_at_point(hit_regions, scene_point);
    if !candidates.is_empty() {
        selection.context_menu_open = true;
        selection.context_menu_pos = Some(click_pos);
        selection.context_menu_actors = candidates;
    }
}

/// Handle left-click with cycling and multi-select support.
///
/// - No modifiers: replace selection with clicked actor (or cycle through overlap).
/// - Shift or Ctrl: toggle clicked actor in selection (add if absent, remove if present).
/// - Empty click: clear selection unless Shift/Ctrl is held.
pub(crate) fn handle_click(
    selection: &mut SelectionState,
    selected_actors: &mut HashSet<String>,
    hit_regions: &[(String, kurbo::Rect)],
    click_pos: Pos2,
    screen_to_scene: impl Fn(Pos2) -> kurbo::Point,
    modifiers: &egui::Modifiers,
) {
    let scene_point = screen_to_scene(click_pos);
    let candidates = actors_at_point(hit_regions, scene_point);

    let multi = modifiers.shift || modifiers.ctrl || modifiers.command;

    if candidates.is_empty() {
        selection.click_candidates = Vec::new();
        selection.cycle_index = 0;
        selection.last_click_scene = None;
        if !multi {
            selected_actors.clear();
        }
        selection.clear_tapped_place();
        return;
    }

    // Check if this is a repeat click at the same position
    let is_repeat = selection
        .last_click_scene
        .is_some_and(|last| is_same_position(scene_point, last, 5.0));

    if is_repeat && selection.click_candidates == candidates {
        // Cycle to next candidate
        selection.cycle_index = (selection.cycle_index + 1) % candidates.len();
    } else {
        // New click position, reset cycle
        selection.click_candidates = candidates;
        selection.cycle_index = 0;
    }

    selection.last_click_scene = Some(scene_point);
    let actor = selection.click_candidates[selection.cycle_index].clone();

    if multi {
        if selected_actors.contains(&actor) {
            selected_actors.remove(&actor);
        } else {
            selected_actors.insert(actor);
        }
    } else {
        selected_actors.clear();
        selected_actors.insert(actor);
    }
}

// ─── Context Menu Drawing ───────────────────────────────────────────────────

/// Draw the right-click context menu for actor selection.
/// Returns the selected actor (if any), whether to close the menu,
/// and the screen-space rect of the menu for outside-click detection.
pub(crate) fn draw_context_menu(
    ui: &mut egui::Ui,
    selection: &SelectionState,
    current_selected: &HashSet<String>,
) -> (Option<String>, bool, Option<egui::Rect>) {
    let menu_pos = selection.context_menu_pos.unwrap_or_default();
    let actors = &selection.context_menu_actors;

    let entries: Vec<MenuEntry> = std::iter::once(MenuEntry::header("Select actor"))
        .chain(std::iter::once(MenuEntry::separator()))
        .chain(actors.iter().enumerate().map(|(i, actor)| {
            let is_selected = current_selected.contains(actor);
            let prefix = if i < 9 {
                format!("{}.", i + 1)
            } else {
                "  ".to_string()
            };
            let label = format!("{} {}", prefix, actor);
            MenuEntry::Item {
                icon: None,
                label,
                shortcut: None,
                checked: is_selected,
                enabled: true,
            }
        }))
        .collect();

    let (clicked_idx, menu_rect) =
        render_floating_menu(ui.ctx(), egui::Id::new("selection_context_menu"), menu_pos, &entries);

    let selected = clicked_idx.and_then(|idx| {
        // Subtract 2 for header + separator
        actors.get(idx.saturating_sub(2)).cloned()
    });
    let close = selected.is_some();

    (selected, close, Some(menu_rect))
}

// Hover/cycle drawing now lives in `overlay_ops.rs` so overlay behavior can be
// tested without a live egui painter.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_same_position_identical() {
        let a = kurbo::Point::new(10.0, 20.0);
        // Strict < tolerance: 0.0 < 0.001 is true
        assert!(is_same_position(a, a, 0.001));
    }

    #[test]
    fn test_is_same_position_within_tolerance() {
        let a = kurbo::Point::new(10.0, 20.0);
        let b = kurbo::Point::new(10.3, 20.4);
        assert!(is_same_position(a, b, 0.5));
    }

    #[test]
    fn test_is_same_position_at_tolerance_boundary() {
        let a = kurbo::Point::new(10.0, 20.0);
        let b = kurbo::Point::new(10.5, 20.5);
        // Strict < tolerance: dx=0.5, dy=0.5, tolerance=0.5 → 0.5 < 0.5 is false
        assert!(!is_same_position(a, b, 0.5));
    }

    #[test]
    fn test_is_same_position_beyond_tolerance() {
        let a = kurbo::Point::new(10.0, 20.0);
        let b = kurbo::Point::new(15.0, 30.0);
        assert!(!is_same_position(a, b, 1.0));
    }

    #[test]
    fn test_is_same_position_dx_within_dy_beyond() {
        let a = kurbo::Point::new(10.0, 20.0);
        let b = kurbo::Point::new(10.1, 25.0);
        // dx=0.1 < 5.0, dy=5.0 < 5.0 is false (equal, not less)
        assert!(!is_same_position(a, b, 5.0));
    }

    #[test]
    fn test_is_same_position_zero_tolerance() {
        let a = kurbo::Point::new(10.0, 20.0);
        let b = kurbo::Point::new(10.0, 20.0);
        // Strict < tolerance: 0.0 < 0.0 is false
        assert!(!is_same_position(a, b, 0.0));
        // But with epsilon > 0, identical points should pass
        assert!(is_same_position(a, b, 1e-12));
        let c = kurbo::Point::new(10.0, 20.001);
        assert!(!is_same_position(a, c, 0.0));
    }
}
