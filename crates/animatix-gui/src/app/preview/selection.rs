//! Selection system for the preview canvas.
//!
//! Handles hover preview, click cycling through overlapping actors,
//! and right-click context menu for explicit selection.

use std::collections::HashSet;
use super::*;
use crate::app::components::context_menu::{render_floating_menu, MenuEntry};
use crate::app::design_tokens::*;
use egui::{Pos2, Vec2};

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
        return;
    }

    // Check if this is a repeat click at the same position
    let is_repeat = selection
        .last_click_scene
        .map_or(false, |last| is_same_position(scene_point, last, 5.0));

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

    let (clicked_idx, menu_rect) = render_floating_menu(
        ui.ctx(),
        egui::Id::new("selection_context_menu"),
        menu_pos,
        &entries,
    );

    let selected = clicked_idx.and_then(|idx| {
        // Subtract 2 for header + separator
        actors.get(idx.saturating_sub(2)).cloned()
    });
    let close = selected.is_some();

    (selected, close, Some(menu_rect))
}

// ─── Hover Overlay Drawing ──────────────────────────────────────────────────

/// Draw hover highlight for the actor under the cursor.
pub(crate) fn draw_hover_highlight(
    painter: &egui::Painter,
    hovered_actor: &str,
    hover_rect: egui::Rect,
) {
    // Subtle dashed outline for hover
    let hover_color = accent_ghost();
    let dash_len = 4.0;
    let gap_len = 3.0;
    let corners = [
        hover_rect.left_top(),
        hover_rect.right_top(),
        hover_rect.right_bottom(),
        hover_rect.left_bottom(),
    ];

    for i in 0..4 {
        let start = corners[i];
        let end = corners[(i + 1) % 4];
        let total = start.distance(end);
        let mut pos = 0.0;
        while pos < total {
            let t0 = pos / total;
            let t1 = ((pos + dash_len).min(total)) / total;
            let p0 = Pos2::new(
                start.x + (end.x - start.x) * t0,
                start.y + (end.y - start.y) * t0,
            );
            let p1 = Pos2::new(
                start.x + (end.x - start.x) * t1,
                start.y + (end.y - start.y) * t1,
            );
            painter.line_segment([p0, p1], Stroke::new(1.0, hover_color));
            pos += dash_len + gap_len;
        }
    }

    // Tooltip with actor name
    let tooltip_pos = egui::pos2(hover_rect.center().x, hover_rect.top() - 20.0);
    let galley = painter.layout_no_wrap(
        hovered_actor.to_string(),
        egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
        TEXT_PRIMARY,
    );
    let tooltip_size = galley.size();
    let tooltip_rect =
        egui::Rect::from_center_size(tooltip_pos, tooltip_size + Vec2::new(8.0, 4.0));

    painter.rect_filled(
        tooltip_rect,
        RADIUS_M,
        badge_bg(),
    );
    painter.rect_stroke(
        tooltip_rect,
        RADIUS_M,
        Stroke::new(1.0, BORDER),
        egui::StrokeKind::Outside,
    );
    painter.galley(
        tooltip_rect.left_center() + Vec2::new(4.0, -tooltip_size.y / 2.0),
        galley,
        TEXT_PRIMARY,
    );
}

// ─── Cycle Indicator Drawing ────────────────────────────────────────────────

/// Draw the cycle indicator showing "2/5" near the cursor.
pub(crate) fn draw_cycle_indicator(
    painter: &egui::Painter,
    mouse_pos: Pos2,
    cycle_index: usize,
    total_candidates: usize,
) {
    if total_candidates <= 1 {
        return;
    }

    let indicator_text = format!("{}/{}", cycle_index + 1, total_candidates);
    let indicator_pos = egui::pos2(mouse_pos.x + 16.0, mouse_pos.y - 8.0);

    let galley = painter.layout_no_wrap(
        indicator_text,
        egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
        TEXT_PRIMARY,
    );
    let size = galley.size();
    let rect = egui::Rect::from_center_size(indicator_pos, size + Vec2::new(6.0, 3.0));

    painter.rect_filled(
        rect,
        RADIUS_M,
        accent_strong(),
    );
    painter.galley(
        rect.left_center() + Vec2::new(3.0, -size.y / 2.0),
        galley,
        TEXT_PRIMARY,
    );
}
