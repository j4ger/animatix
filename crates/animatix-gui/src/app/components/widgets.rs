//! Low-level reusable widgets.
//!
//! Custom painter-based inputs have been removed in favor of native egui
//! widgets wrapped in `components::Field`. This module now only holds
//! primitives that egui doesn't provide out of the box.

use egui::{Color32, Vec2};

use crate::app::theme::*;

// ─── Tree Row (legacy — migrate callers to components::Row) ───────────────

const TREE_ROW_HEIGHT: f32 = 22.0;
const TREE_INDENT_PX: f32 = 14.0;
const TREE_CHEVRON_WIDTH: f32 = 14.0;
const TREE_ICON_WIDTH: f32 = 14.0;
const TREE_GAP: f32 = 2.0;
const TREE_HOVER_BG: Color32 = BG_HOVER;
const TREE_SELECTED_BG: Color32 = BG_WIDGET;
const TREE_ACCENT: Color32 = ACCENT_BLUE;

/// Renders a single tree row in a VS Code–style sidebar.
///
/// **Deprecated**: Prefer `components::Row` for new code.
/// Returns true if the row body was clicked (chevron clicks are excluded).
pub fn tree_row(
    ui: &mut egui::Ui,
    row_id: egui::Id,
    depth: usize,
    has_children: bool,
    is_expanded: bool,
    is_selected: bool,
    icon: Option<&'static str>,
    label: &str,
    label_color: Option<Color32>,
    on_toggle: impl FnOnce(),
) -> bool {
    let available_width = ui.available_width();
    let (row_rect, row_response) = ui.allocate_exact_size(
        Vec2::new(available_width, TREE_ROW_HEIGHT),
        egui::Sense::click(),
    );

    let bg = if is_selected {
        TREE_SELECTED_BG
    } else if row_response.hovered() {
        TREE_HOVER_BG
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(row_rect, 0.0, bg);
    }

    if is_selected {
        let accent_rect = egui::Rect::from_min_size(row_rect.min, Vec2::new(2.0, row_rect.height()));
        ui.painter().rect_filled(accent_rect, 0.0, TREE_ACCENT);
    }

    let baseline_y = row_rect.center().y;
    let mut cursor_x = row_rect.min.x + 4.0 + depth as f32 * TREE_INDENT_PX;

    let chevron_rect = egui::Rect::from_min_size(
        egui::pos2(cursor_x, row_rect.min.y),
        Vec2::new(TREE_CHEVRON_WIDTH, TREE_ROW_HEIGHT),
    );
    let chevron_response =
        ui.interact(chevron_rect, row_id.with("chevron"), egui::Sense::click());

    if has_children {
        let chevron_icon = if is_expanded {
            egui_phosphor::regular::CARET_DOWN
        } else {
            egui_phosphor::regular::CARET_RIGHT
        };
        let chevron_color = if chevron_response.hovered() {
            TEXT_SECONDARY
        } else {
            TEXT_MUTED
        };
        ui.painter().text(
            egui::pos2(chevron_rect.center().x, baseline_y),
            egui::Align2::CENTER_CENTER,
            chevron_icon,
            egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
            chevron_color,
        );
    }

    if chevron_response.clicked() {
        on_toggle();
    }

    cursor_x += TREE_CHEVRON_WIDTH;

    if let Some(icon_str) = icon {
        cursor_x += TREE_GAP;
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(cursor_x, row_rect.min.y),
            Vec2::new(TREE_ICON_WIDTH, TREE_ROW_HEIGHT),
        );
        let default_icon_color = if is_selected { TEXT_PRIMARY } else { TEXT_MUTED };
        let icon_color = label_color.unwrap_or(default_icon_color);
        ui.painter().text(
            egui::pos2(icon_rect.center().x, baseline_y),
            egui::Align2::CENTER_CENTER,
            icon_str,
            egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
            icon_color,
        );
        cursor_x += TREE_ICON_WIDTH + TREE_GAP;
    } else {
        cursor_x += TREE_GAP * 2.0;
    }

    let label_color = label_color.unwrap_or_else(|| {
        if is_selected {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        }
    });

    ui.painter().text(
        egui::pos2(cursor_x, baseline_y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
        label_color,
    );

    row_response.clicked() && !chevron_response.clicked()
}

// ─── Pill Tab Bar ─────────────────────────────────────────────────────────

/// Reusable pill-style segmented tab bar.
///
/// `tabs` is a slice of `(tab_value, icon, label)` tuples.
/// Returns `Some(new_tab)` if a different tab was clicked, else `None`.
pub fn pill_tab_bar<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    active_tab: T,
    tabs: &[(T, &'static str, &'static str)],
) -> Option<T> {
    let available = ui.available_width();
    let tab_h = 26.0;
    let gap = 2.0;
    let tab_w = (available - gap * (tabs.len().saturating_sub(1)) as f32) / tabs.len() as f32;

    let bar_rect = ui
        .allocate_exact_size(Vec2::new(available, tab_h), egui::Sense::hover())
        .0;
    ui.painter().rect_filled(bar_rect, RADIUS_M, BG_WIDGET);

    let mut clicked_tab = None;

    for (idx, (tab, icon, label)) in tabs.iter().enumerate() {
        let is_active = active_tab == *tab;
        let x = bar_rect.min.x + idx as f32 * (tab_w + gap);
        let tab_rect =
            egui::Rect::from_min_size(egui::pos2(x, bar_rect.min.y), Vec2::new(tab_w, tab_h));

        let response = ui.interact(tab_rect, ui.id().with(("pill_tab", idx)), egui::Sense::click());

        if is_active {
            let pill = tab_rect.shrink2(Vec2::new(2.0, 2.0));
            ui.painter().rect_filled(pill, RADIUS_M, BG_SURFACE);
        } else if response.hovered() {
            let hover_bg = BG_WIDGET;
            let pill = tab_rect.shrink2(Vec2::new(2.0, 2.0));
            ui.painter().rect_filled(pill, RADIUS_M, hover_bg);
        }

        let text_color = if is_active { TEXT_PRIMARY } else { TEXT_MUTED };
        let font_id = egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional);
        let full_text = format!("{}  {}", icon, label);
        let galley = ui.painter().layout_no_wrap(full_text.clone(), font_id.clone(), text_color);
        let show_label = galley.size().x + 12.0 <= tab_w; // 12px padding
        let display_text = if show_label { full_text } else { icon.to_string() };
        ui.painter().text(
            tab_rect.center(),
            egui::Align2::CENTER_CENTER,
            display_text,
            font_id,
            text_color,
        );

        if response.clicked() {
            clicked_tab = Some(*tab);
        }
    }

    clicked_tab
}
