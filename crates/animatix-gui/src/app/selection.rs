//! Selection system for the preview canvas.
//!
//! Handles hover preview, click cycling through overlapping actors,
//! and right-click context menu for explicit selection.

use super::*;

// ─── Selection State ────────────────────────────────────────────────────────

/// State for the selection system in the preview canvas.
#[derive(Debug, Default)]
pub(super) struct SelectionState {
    /// Actor currently hovered in preview (for hover highlight).
    pub(super) hovered_actor: Option<String>,
    /// Actors under the last click position (for click cycling).
    pub(super) click_candidates: Vec<String>,
    /// Current index into click_candidates for cycling.
    pub(super) cycle_index: usize,
    /// Last click position in scene coordinates (to detect same-position clicks).
    pub(super) last_click_scene: Option<kurbo::Point>,
    /// Whether the right-click context menu is open.
    pub(super) context_menu_open: bool,
    /// Position for the right-click context menu (screen coordinates).
    pub(super) context_menu_pos: Option<Pos2>,
    /// Actors at the right-click position for the context menu.
    pub(super) context_menu_actors: Vec<String>,
}

// ─── Helper Functions ───────────────────────────────────────────────────────

/// Returns all actors at the given scene point, ordered from topmost (last rendered) to bottommost.
pub(super) fn actors_at_point(
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
pub(super) fn update_hover(
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
pub(super) fn handle_right_click(
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

/// Handle left-click with cycling support.
/// Returns the actor to select, or None to deselect.
pub(super) fn handle_click(
    selection: &mut SelectionState,
    hit_regions: &[(String, kurbo::Rect)],
    click_pos: Pos2,
    screen_to_scene: impl Fn(Pos2) -> kurbo::Point,
) -> Option<String> {
    let scene_point = screen_to_scene(click_pos);
    let candidates = actors_at_point(hit_regions, scene_point);

    if candidates.is_empty() {
        selection.click_candidates = Vec::new();
        selection.cycle_index = 0;
        selection.last_click_scene = None;
        return None;
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
    Some(selection.click_candidates[selection.cycle_index].clone())
}

// ─── Context Menu Drawing ───────────────────────────────────────────────────

/// Draw the right-click context menu for actor selection.
/// Returns the selected actor (if any) and whether to close the menu.
pub(super) fn draw_context_menu(
    ui: &mut egui::Ui,
    selection: &SelectionState,
    current_selected: &Option<String>,
) -> (Option<String>, bool) {
    let menu_pos = selection.context_menu_pos.unwrap_or_default();
    let actors = &selection.context_menu_actors;
    let mut selected_from_menu = None;
    let mut close_menu = false;

    egui::Area::new(egui::Id::new("selection_context_menu"))
        .fixed_pos(menu_pos)
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(30, 33, 40))
                .stroke(Stroke::new(1.0, Color32::from_rgb(60, 65, 75)))
                .corner_radius(4.0)
                .inner_margin(4.0)
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Select actor:")
                            .small()
                            .color(Color32::from_rgb(150, 158, 175)),
                    );
                    ui.separator();

                    for (i, actor) in actors.iter().enumerate() {
                        let is_selected = current_selected.as_ref() == Some(actor);
                        let text = if is_selected {
                            RichText::new(format!("● {}", actor))
                                .color(Color32::from_rgb(84, 110, 255))
                        } else {
                            RichText::new(format!("  {}", actor))
                                .color(Color32::from_rgb(200, 200, 210))
                        };

                        let btn = egui::Button::new(text)
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE);

                        if ui
                            .add_sized([ui.available_width(), 20.0], btn)
                            .clicked()
                        {
                            selected_from_menu = Some(actor.clone());
                            close_menu = true;
                        }

                        // Show index hint
                        if i < 9 {
                            ui.painter().text(
                                egui::pos2(ui.max_rect().right() - 8.0, ui.min_rect().center().y),
                                egui::Align2::RIGHT_CENTER,
                                format!("{}", i + 1),
                                egui::TextStyle::Small.resolve(ui.style()),
                                Color32::from_rgb(100, 100, 110),
                            );
                        }
                    }

                    ui.separator();
                    if ui
                        .add_sized(
                            [ui.available_width(), 18.0],
                            egui::Button::new(
                                RichText::new("Cancel")
                                    .small()
                                    .color(Color32::from_rgb(120, 120, 130)),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::NONE),
                        )
                        .clicked()
                    {
                        close_menu = true;
                    }
                });
        });

    (selected_from_menu, close_menu)
}

// ─── Hover Overlay Drawing ──────────────────────────────────────────────────

/// Draw hover highlight for the actor under the cursor.
pub(super) fn draw_hover_highlight(
    painter: &egui::Painter,
    hovered_actor: &str,
    hover_rect: egui::Rect,
) {
    // Subtle dashed outline for hover
    let hover_color = Color32::from_rgba_unmultiplied(84, 110, 255, 80);
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
        egui::TextStyle::Small.resolve(painter.ctx().global_style().as_ref()),
        Color32::WHITE,
    );
    let tooltip_size = galley.size();
    let tooltip_rect =
        egui::Rect::from_center_size(tooltip_pos, tooltip_size + Vec2::new(8.0, 4.0));

    painter.rect_filled(tooltip_rect, 3.0, Color32::from_rgba_unmultiplied(30, 33, 40, 220));
    painter.rect_stroke(
        tooltip_rect,
        3.0,
        Stroke::new(1.0, Color32::from_rgb(60, 65, 75)),
        egui::StrokeKind::Outside,
    );
    painter.galley(
        tooltip_rect.left_center() + Vec2::new(4.0, -tooltip_size.y / 2.0),
        galley,
        Color32::WHITE,
    );
}

// ─── Cycle Indicator Drawing ────────────────────────────────────────────────

/// Draw the cycle indicator showing "2/5" near the cursor.
pub(super) fn draw_cycle_indicator(
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
        egui::TextStyle::Small.resolve(painter.ctx().global_style().as_ref()),
        Color32::WHITE,
    );
    let size = galley.size();
    let rect = egui::Rect::from_center_size(indicator_pos, size + Vec2::new(6.0, 3.0));

    painter.rect_filled(rect, 3.0, Color32::from_rgba_unmultiplied(84, 110, 255, 200));
    painter.galley(
        rect.left_center() + Vec2::new(3.0, -size.y / 2.0),
        galley,
        Color32::WHITE,
    );
}
