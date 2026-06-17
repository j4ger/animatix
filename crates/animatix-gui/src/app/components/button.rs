use egui::{Color32, Response, Sense, Vec2};

use crate::app::design_tokens::semantic::{accent, border, surface, text};
use crate::app::design_tokens::spatial::{RADIUS_M, RADIUS_S, ROW_L, ROW_M, SPACE_M, STROKE_WIDTH};
use crate::app::design_tokens::typography::{FONT_SIZE_M, FONT_SIZE_S};

/// A small square icon button with hover highlight.
pub fn icon_button(
    ui: &mut egui::Ui,
    icon: &'static str,
    tooltip: &str,
) -> Response {
    let size = Vec2::new(ROW_L, ROW_L);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, RADIUS_M, surface::HOVER);
    }

    let icon_color = if response.hovered() {
        text::PRIMARY
    } else {
        text::SECONDARY
    };

    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::TextStyle::Body.resolve(ui.style()),
        icon_color,
    );

    if !tooltip.is_empty() {
        return response.on_hover_text(tooltip);
    }
    response
}

/// Variant of [`icon_button`] that uses a custom icon color.
pub fn icon_button_colored(
    ui: &mut egui::Ui,
    icon: &'static str,
    tooltip: &str,
    color: Color32,
    hover_color: Color32,
) -> Response {
    let size = Vec2::new(ROW_L, ROW_L);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, RADIUS_M, surface::HOVER);
    }

    let icon_color = if response.hovered() { hover_color } else { color };

    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::TextStyle::Body.resolve(ui.style()),
        icon_color,
    );

    if !tooltip.is_empty() {
        return response.on_hover_text(tooltip);
    }
    response
}

/// Returns the play/pause icon character based on playback state.
pub fn play_pause_icon(is_playing: bool) -> &'static str {
    if is_playing {
        egui_phosphor::regular::PAUSE
    } else {
        egui_phosphor::regular::PLAY
    }
}

/// A play/pause toggle button.
pub fn play_pause_button(ui: &mut egui::Ui, is_playing: bool) -> Response {
    let icon = play_pause_icon(is_playing);
    icon_button(ui, icon, "Play/Pause (Space)")
}

/// A toolbar toggle button with icon, optional label, tooltip, and active-state styling.
pub fn toolbar_toggle_button(
    ui: &mut egui::Ui,
    icon: &'static str,
    label: Option<&'static str>,
    tooltip: &'static str,
    is_active: bool,
    show_label: bool,
) -> Response {
    let has_label = label.is_some() && show_label;
    let font_id = egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional);
    let icon_font = egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional);

    let icon_galley = ui.painter().layout_no_wrap(icon.to_string(), icon_font.clone(), text::PRIMARY);
    let mut width = icon_galley.size().x + SPACE_M * 2.0;
    let mut label_galley = None;
    if let Some(l) = label.filter(|_| has_label) {
        let galley = ui.painter().layout_no_wrap(format!("  {}", l), font_id.clone(), text::PRIMARY);
        width += galley.size().x;
        label_galley = Some(galley);
    }
    let height = ROW_M;
    let size = Vec2::new(width.max(ROW_L), height);

    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    let bg = if is_active {
        surface::ACTIVE
    } else if response.hovered() || response.is_pointer_button_down_on() {
        surface::HOVER
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, RADIUS_M, bg);
    }

    if is_active {
        let accent_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + 4.0, rect.max.y - 2.0),
            Vec2::new(rect.width() - 8.0, 2.0),
        );
        ui.painter().rect_filled(accent_rect, RADIUS_S, accent::PRIMARY);
    }

    let icon_color = if is_active {
        accent::PRIMARY
    } else if response.hovered() {
        text::PRIMARY
    } else {
        text::SECONDARY
    };

    let mut cursor_x = rect.min.x + SPACE_M;
    let baseline_y = rect.center().y;

    ui.painter().text(
        egui::pos2(cursor_x + icon_galley.size().x / 2.0, baseline_y),
        egui::Align2::CENTER_CENTER,
        icon,
        icon_font,
        icon_color,
    );
    cursor_x += icon_galley.size().x;

    if let Some(galley) = label_galley {
        let label_color = if is_active { accent::PRIMARY } else if response.hovered() { text::PRIMARY } else { text::SECONDARY };
        ui.painter().galley(
            egui::pos2(cursor_x, baseline_y - galley.size().y / 2.0),
            galley,
            label_color,
        );
    }

    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            RADIUS_M,
            egui::Stroke::new(STROKE_WIDTH, accent::PRIMARY),
            egui::StrokeKind::Inside,
        );
    }

    response.on_hover_text(tooltip)
}

/// A toolbar action button (momentary command, not a toggle).
pub fn toolbar_action_button(
    ui: &mut egui::Ui,
    icon: &'static str,
    label: Option<&'static str>,
    tooltip: &'static str,
    show_label: bool,
) -> Response {
    let has_label = label.is_some() && show_label;
    let font_id = egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional);
    let icon_font = egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional);

    let icon_galley = ui.painter().layout_no_wrap(icon.to_string(), icon_font.clone(), text::PRIMARY);
    let mut width = icon_galley.size().x + SPACE_M * 2.0;
    let mut label_galley = None;
    if let Some(l) = label.filter(|_| has_label) {
        let galley = ui.painter().layout_no_wrap(format!("  {}", l), font_id.clone(), text::PRIMARY);
        width += galley.size().x;
        label_galley = Some(galley);
    }
    let height = ROW_M;
    let size = Vec2::new(width.max(ROW_L), height);

    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    let bg = if response.is_pointer_button_down_on() {
        surface::ACTIVE
    } else if response.hovered() {
        surface::HOVER
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(rect, RADIUS_M, bg);
    }

    let icon_color = if response.hovered() || response.is_pointer_button_down_on() {
        text::PRIMARY
    } else {
        text::SECONDARY
    };

    let mut cursor_x = rect.min.x + SPACE_M;
    let baseline_y = rect.center().y;

    ui.painter().text(
        egui::pos2(cursor_x + icon_galley.size().x / 2.0, baseline_y),
        egui::Align2::CENTER_CENTER,
        icon,
        icon_font,
        icon_color,
    );
    cursor_x += icon_galley.size().x;

    if let Some(galley) = label_galley {
        let label_color = if response.hovered() || response.is_pointer_button_down_on() {
            text::PRIMARY
        } else {
            text::SECONDARY
        };
        ui.painter().galley(
            egui::pos2(cursor_x, baseline_y - galley.size().y / 2.0),
            galley,
            label_color,
        );
    }

    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.shrink(1.0),
            RADIUS_M,
            egui::Stroke::new(STROKE_WIDTH, accent::PRIMARY),
            egui::StrokeKind::Inside,
        );
    }

    response.on_hover_text(tooltip)
}

/// A small vertical separator for toolbar button groups.
pub fn toolbar_separator(ui: &mut egui::Ui) {
    let height = ROW_M - 4.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, height), Sense::hover());
    ui.painter().line_segment(
        [egui::pos2(rect.center().x, rect.min.y), egui::pos2(rect.center().x, rect.max.y)],
        egui::Stroke::new(STROKE_WIDTH, border::DEFAULT),
    );
}
