//! Reusable editable input widgets for the inspector panel.
//!
//! Each widget is a standalone function that takes `&mut egui::Ui` and returns
//! `Option<T>` when the value changes. Widgets are compact (18px row height)
//! and use the inspector's dark theme palette.

use egui::{Color32, RichText, Stroke, Vec2};

// ─── Theme Constants ────────────────────────────────────────────────────────

const LABEL_COLOR: Color32 = Color32::from_rgb(110, 118, 135);
const VALUE_COLOR: Color32 = Color32::from_rgb(200, 206, 220);
const ACCENT_COLOR: Color32 = Color32::from_rgb(137, 200, 235);
const BORDER_COLOR: Color32 = Color32::from_rgb(40, 44, 52);
const FIELD_BG: Color32 = Color32::from_rgb(24, 27, 33);
const FIELD_BG_HOVER: Color32 = Color32::from_rgb(30, 34, 42);
const BG_WIDGET: Color32 = Color32::from_rgb(32, 36, 44);
const AMBER: Color32 = Color32::from_rgb(255, 196, 92);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(228, 232, 243);
const TEXT_SECONDARY: Color32 = Color32::from_rgb(150, 158, 175);
const TEXT_MUTED: Color32 = Color32::from_rgb(90, 96, 110);
const ROW_HEIGHT: f32 = 20.0;
const FIELD_HEIGHT: f32 = 18.0;

// ─── Tree Row ───────────────────────────────────────────────────────────────

const TREE_ROW_HEIGHT: f32 = 22.0;
const TREE_INDENT_PX: f32 = 14.0;
const TREE_CHEVRON_WIDTH: f32 = 14.0;
const TREE_ICON_WIDTH: f32 = 14.0;
const TREE_GAP: f32 = 2.0;
const TREE_HOVER_BG: Color32 = Color32::from_rgb(24, 27, 33);
const TREE_SELECTED_BG: Color32 = Color32::from_rgb(32, 36, 44);
const TREE_ACCENT: Color32 = Color32::from_rgb(84, 110, 255);

/// Renders a single tree row in a VS Code–style sidebar.
///
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

    // ── Background (full-width, VS Code style) ─────────────────────────────
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

    // ── Left accent border for selected rows ───────────────────────────────
    if is_selected {
        let accent_rect = egui::Rect::from_min_size(
            row_rect.min,
            Vec2::new(2.0, row_rect.height()),
        );
        ui.painter().rect_filled(accent_rect, 0.0, TREE_ACCENT);
    }

    let baseline_y = row_rect.center().y;
    let mut cursor_x = row_rect.min.x + 4.0 + depth as f32 * TREE_INDENT_PX;

    // ── Chevron ────────────────────────────────────────────────────────────
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
            egui::TextStyle::Small.resolve(ui.style()),
            chevron_color,
        );
    }

    if chevron_response.clicked() {
        on_toggle();
    }

    cursor_x += TREE_CHEVRON_WIDTH;

    // ── Icon ───────────────────────────────────────────────────────────────
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
            egui::TextStyle::Small.resolve(ui.style()),
            icon_color,
        );
        cursor_x += TREE_ICON_WIDTH + TREE_GAP;
    } else {
        cursor_x += TREE_GAP * 2.0;
    }

    // ── Label (left-aligned, painter text — never centered) ────────────────
    let label_color = label_color.unwrap_or_else(|| {
        if is_selected {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        }
    });

    let font_id = egui::TextStyle::Small.resolve(ui.style());
    ui.painter().text(
        egui::pos2(cursor_x, baseline_y),
        egui::Align2::LEFT_CENTER,
        label,
        font_id,
        label_color,
    );

    // Return true if row body was clicked (excluding chevron)
    row_response.clicked() && !chevron_response.clicked()
}

// ─── Float Input ────────────────────────────────────────────────────────────

/// Single number field with optional suffix and drag-to-adjust.
/// Returns `Some(value)` if changed.
pub fn float_input(
    ui: &mut egui::Ui,
    label: &str,
    value: f32,
    suffix: &str,
    has_keyframes: bool,
) -> Option<f32> {
    let available = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(available, ROW_HEIGHT), egui::Sense::hover());

    // Keyframe indicator
    let label_x = if has_keyframes {
        let dot_rect = egui::Rect::from_center_size(
            egui::pos2(rect.min.x + 6.0, rect.center().y),
            Vec2::new(5.0, 5.0),
        );
        ui.painter().rect_filled(dot_rect, 1.5, AMBER);
        rect.min.x + 14.0
    } else {
        rect.min.x + 8.0
    };

    // Label
    ui.painter().text(
        egui::pos2(label_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Small.resolve(ui.style()),
        LABEL_COLOR,
    );

    // Editable field
    let field_width = 70.0;
    let field_rect = egui::Rect::from_min_size(
        egui::pos2(rect.max.x - field_width - 4.0, rect.min.y + 1.0),
        Vec2::new(field_width, FIELD_HEIGHT),
    );

    let id = ui.id().with(("float", label));
    let mut text = format_value_text(value, suffix);
    let bg = if ui.rect_contains_pointer(field_rect) {
        FIELD_BG_HOVER
    } else {
        FIELD_BG
    };
    ui.painter().rect_filled(field_rect, 4.0, bg);
    ui.painter().rect_stroke(field_rect, 4.0, Stroke::new(1.0, BORDER_COLOR), egui::StrokeKind::Outside);

    // Drag-to-adjust on the field area
    let drag_response = ui.interact(field_rect, id.with("drag"), egui::Sense::drag());
    if drag_response.dragged() {
        let delta = drag_response.drag_delta().x;
        let step = if ui.input(|i| i.modifiers.shift) { 10.0 } else { 1.0 };
        let new_val = value + delta * 0.1 * step;
        let new_val = if suffix == "°" {
            (new_val * 10.0).round() / 10.0
        } else {
            (new_val * 100.0).round() / 100.0
        };
        text = format_value_text(new_val, suffix);
        paint_value_text(ui, field_rect, &text);
        return Some(new_val);
    }

    // Click to edit as text
    let click_response = ui.interact(field_rect, id.with("click"), egui::Sense::click());
    if click_response.clicked() {
        ui.memory_mut(|m| m.request_focus(id));
    }

    let is_focused = ui.memory(|m| m.has_focus(id));
    if is_focused {
        let edit = egui::TextEdit::singleline(&mut text)
            .font(egui::TextStyle::Small)
            .desired_width(field_width - 8.0)
            .margin(Vec2::new(4.0, 2.0))
            .text_color(VALUE_COLOR)
            .background_color(FIELD_BG);
        let response = ui.put(field_rect, edit);
        if response.lost_focus() {
            if let Ok(new_val) = text.trim_end_matches(|c: char| !c.is_ascii_digit() && c != '.' && c != '-').parse::<f32>() {
                return Some(new_val);
            }
        }
        None
    } else {
        paint_value_text(ui, field_rect, &text);
        None
    }
}

fn format_value_text(value: f32, suffix: &str) -> String {
    if value == value.floor() && value.abs() < 10000.0 {
        format!("{:.0}{}", value, suffix)
    } else {
        format!("{:.1}{}", value, suffix)
    }
}

fn paint_value_text(ui: &mut egui::Ui, field_rect: egui::Rect, text: &str) {
    ui.painter().text(
        egui::pos2(field_rect.max.x - 6.0, field_rect.center().y),
        egui::Align2::RIGHT_CENTER,
        text,
        egui::TextStyle::Small.resolve(ui.style()),
        VALUE_COLOR,
    );
}

// ─── Vec2 Input ─────────────────────────────────────────────────────────────

/// Two linked number fields side by side with drag-to-adjust.
/// Returns `Some((x, y))` if either value changed.
pub fn vec2_input(
    ui: &mut egui::Ui,
    label: &str,
    x: f32,
    y: f32,
    has_keyframes: bool,
) -> Option<(f32, f32)> {
    let available = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(available, ROW_HEIGHT), egui::Sense::hover());

    // Keyframe indicator
    let label_x = if has_keyframes {
        let dot_rect = egui::Rect::from_center_size(
            egui::pos2(rect.min.x + 6.0, rect.center().y),
            Vec2::new(5.0, 5.0),
        );
        ui.painter().rect_filled(dot_rect, 1.5, AMBER);
        rect.min.x + 14.0
    } else {
        rect.min.x + 8.0
    };

    // Label
    ui.painter().text(
        egui::pos2(label_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Small.resolve(ui.style()),
        LABEL_COLOR,
    );

    // Two fields
    let field_w = 55.0;
    let gap = 3.0;
    let total_w = field_w * 2.0 + gap;
    let start_x = rect.max.x - total_w - 4.0;

    let field_y_rect = egui::Rect::from_min_size(
        egui::pos2(start_x, rect.min.y + 1.0),
        Vec2::new(field_w, FIELD_HEIGHT),
    );
    let field_x_rect = egui::Rect::from_min_size(
        egui::pos2(start_x + field_w + gap, rect.min.y + 1.0),
        Vec2::new(field_w, FIELD_HEIGHT),
    );

    let mut result = None;

    // Y field (left)
    if let Some(new_y) = vec2_single_field(ui, label, "y", y, field_y_rect) {
        result = Some((x, new_y));
    }

    // X field (right)
    if let Some(new_x) = vec2_single_field(ui, label, "x", x, field_x_rect) {
        match &mut result {
            Some((rx, _)) => *rx = new_x,
            None => result = Some((new_x, y)),
        }
    }

    result
}

fn vec2_single_field(
    ui: &mut egui::Ui,
    label: &str,
    axis: &str,
    value: f32,
    field_rect: egui::Rect,
) -> Option<f32> {
    let id = ui.id().with(("vec2", label, axis));
    let bg = if ui.rect_contains_pointer(field_rect) {
        FIELD_BG_HOVER
    } else {
        FIELD_BG
    };
    ui.painter().rect_filled(field_rect, 4.0, bg);
    ui.painter().rect_stroke(field_rect, 4.0, Stroke::new(1.0, BORDER_COLOR), egui::StrokeKind::Outside);

    let mut text = format_num(value);

    // Drag-to-adjust
    let drag_response = ui.interact(field_rect, id.with("drag"), egui::Sense::drag());
    if drag_response.dragged() {
        let delta = drag_response.drag_delta().x;
        let step = if ui.input(|i| i.modifiers.shift) { 10.0 } else { 1.0 };
        let new_val = ((value + delta * 0.1 * step) * 100.0).round() / 100.0;
        text = format_num(new_val);
        paint_value_text(ui, field_rect, &text);
        return Some(new_val);
    }

    // Click to edit
    let click_response = ui.interact(field_rect, id.with("click"), egui::Sense::click());
    if click_response.clicked() {
        ui.memory_mut(|m| m.request_focus(id));
    }

    let is_focused = ui.memory(|m| m.has_focus(id));
    if is_focused {
        let edit = egui::TextEdit::singleline(&mut text)
            .font(egui::TextStyle::Small)
            .desired_width(field_rect.width() - 8.0)
            .margin(Vec2::new(4.0, 2.0))
            .text_color(VALUE_COLOR)
            .background_color(FIELD_BG);
        let response = ui.put(field_rect, edit);
        if response.lost_focus() {
            if let Ok(new_val) = text.trim().parse::<f32>() {
                return Some(new_val);
            }
        }
        None
    } else {
        paint_value_text(ui, field_rect, &text);
        None
    }
}

fn format_num(v: f32) -> String {
    if v == v.floor() && v.abs() < 10000.0 {
        format!("{:.0}", v)
    } else {
        format!("{:.1}", v)
    }
}

// ─── Color Input ────────────────────────────────────────────────────────────

/// Color swatch + hex text field. Returns `Some(rgba)` if color changed.
pub fn color_input(
    ui: &mut egui::Ui,
    label: &str,
    rgba: [f32; 4],
    has_keyframes: bool,
) -> Option<[f32; 4]> {
    let available = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(available, ROW_HEIGHT), egui::Sense::hover());

    // Keyframe indicator
    let label_x = if has_keyframes {
        let dot_rect = egui::Rect::from_center_size(
            egui::pos2(rect.min.x + 6.0, rect.center().y),
            Vec2::new(5.0, 5.0),
        );
        ui.painter().rect_filled(dot_rect, 1.5, AMBER);
        rect.min.x + 14.0
    } else {
        rect.min.x + 8.0
    };

    // Label
    ui.painter().text(
        egui::pos2(label_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Small.resolve(ui.style()),
        LABEL_COLOR,
    );

    // Color swatch
    let swatch_size = 12.0;
    let swatch_x = rect.max.x - 80.0;
    let swatch_rect = egui::Rect::from_center_size(
        egui::pos2(swatch_x, rect.center().y),
        Vec2::new(swatch_size, swatch_size),
    );
    let color = Color32::from_rgba_premultiplied(
        (rgba[0] * 255.0) as u8,
        (rgba[1] * 255.0) as u8,
        (rgba[2] * 255.0) as u8,
        (rgba[3] * 255.0) as u8,
    );
    ui.painter().rect_filled(swatch_rect, 4.0, color);
    ui.painter().rect_stroke(
        swatch_rect,
        4.0,
        Stroke::new(1.0, BORDER_COLOR),
        egui::StrokeKind::Outside,
    );

    // Hex text field
    let field_width = 62.0;
    let field_rect = egui::Rect::from_min_size(
        egui::pos2(rect.max.x - field_width - 2.0, rect.min.y + 1.0),
        Vec2::new(field_width, FIELD_HEIGHT),
    );

    let id = ui.id().with(("color", label));
    let mut hex = color_to_hex(&rgba);

    let bg = if ui.rect_contains_pointer(field_rect) {
        FIELD_BG_HOVER
    } else {
        FIELD_BG
    };
    ui.painter().rect_filled(field_rect, 4.0, bg);
    ui.painter().rect_stroke(field_rect, 4.0, Stroke::new(1.0, BORDER_COLOR), egui::StrokeKind::Outside);

    let click_response = ui.interact(field_rect, id.with("click"), egui::Sense::click());
    if click_response.clicked() {
        ui.memory_mut(|m| m.request_focus(id));
    }

    let is_focused = ui.memory(|m| m.has_focus(id));
    if is_focused {
        let edit = egui::TextEdit::singleline(&mut hex)
            .font(egui::TextStyle::Small)
            .desired_width(field_width - 8.0)
            .margin(Vec2::new(4.0, 2.0))
            .text_color(ACCENT_COLOR)
            .background_color(FIELD_BG);
        let response = ui.put(field_rect, edit);
        if response.lost_focus() {
            if let Some(new_rgba) = hex_to_color(&hex) {
                return Some(new_rgba);
            }
        }
        None
    } else {
        ui.painter().text(
            egui::pos2(field_rect.max.x - 6.0, field_rect.center().y),
            egui::Align2::RIGHT_CENTER,
            &hex,
            egui::TextStyle::Small.resolve(ui.style()),
            ACCENT_COLOR,
        );
        None
    }
}

fn color_to_hex(rgba: &[f32; 4]) -> String {
    let r = (rgba[0] * 255.0).round() as u8;
    let g = (rgba[1] * 255.0).round() as u8;
    let b = (rgba[2] * 255.0).round() as u8;
    if rgba[3] >= 0.99 {
        format!("#{:02x}{:02x}{:02x}", r, g, b)
    } else {
        let a = (rgba[3] * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, a)
    }
}

fn hex_to_color(hex: &str) -> Option<[f32; 4]> {
    let hex = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
    let (r, g, b, a) = match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            (r, g, b, 255u8)
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            (r, g, b, a)
        }
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            (r, g, b, 255u8)
        }
        _ => return None,
    };
    Some([
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ])
}

// ─── Slider Input ───────────────────────────────────────────────────────────

/// Horizontal slider with value label. Returns `Some(value)` if changed.
pub fn slider_input(
    ui: &mut egui::Ui,
    label: &str,
    value: f32,
    min: f32,
    max: f32,
    has_keyframes: bool,
) -> Option<f32> {
    let available = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(available, ROW_HEIGHT), egui::Sense::hover());

    // Keyframe indicator
    let label_x = if has_keyframes {
        let dot_rect = egui::Rect::from_center_size(
            egui::pos2(rect.min.x + 6.0, rect.center().y),
            Vec2::new(5.0, 5.0),
        );
        ui.painter().rect_filled(dot_rect, 1.5, AMBER);
        rect.min.x + 14.0
    } else {
        rect.min.x + 8.0
    };

    // Label
    ui.painter().text(
        egui::pos2(label_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Small.resolve(ui.style()),
        LABEL_COLOR,
    );

    // Value label (right)
    let value_text = format!("{:.2}", value);
    let value_x = rect.max.x - 6.0;
    ui.painter().text(
        egui::pos2(value_x, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        &value_text,
        egui::TextStyle::Small.resolve(ui.style()),
        VALUE_COLOR,
    );

    // Slider track
    let slider_left = label_x + 70.0;
    let slider_right = value_x - 40.0;
    let slider_y = rect.center().y;
    let track_height = 4.0;

    // Track background
    let track_rect = egui::Rect::from_min_max(
        egui::pos2(slider_left, slider_y - track_height / 2.0),
        egui::pos2(slider_right, slider_y + track_height / 2.0),
    );
    ui.painter().rect_filled(track_rect, 4.0, Color32::from_rgb(40, 44, 52));

    // Filled portion
    let fraction = ((value - min) / (max - min)).clamp(0.0, 1.0);
    let filled_width = track_rect.width() * fraction;
    let filled_rect = egui::Rect::from_min_size(
        track_rect.min,
        Vec2::new(filled_width, track_height),
    );
    ui.painter().rect_filled(filled_rect, 4.0, Color32::from_rgb(84, 110, 255));

    // Thumb
    let thumb_x = slider_left + filled_width;
    let thumb_rect = egui::Rect::from_center_size(
        egui::pos2(thumb_x, slider_y),
        Vec2::new(8.0, 12.0),
    );
    ui.painter().rect_filled(thumb_rect, 4.0, Color32::from_rgb(200, 206, 220));

    // Interactive area (wider than visible track for easier clicking)
    let interact_rect = egui::Rect::from_min_max(
        egui::pos2(slider_left - 4.0, rect.min.y),
        egui::pos2(slider_right + 4.0, rect.max.y),
    );

    let id = ui.id().with(("slider", label));
    let response = ui.interact(interact_rect, id, egui::Sense::drag());

    if response.dragged() || response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            let new_fraction = ((pos.x - slider_left) / (slider_right - slider_left)).clamp(0.0, 1.0);
            let new_value = min + new_fraction * (max - min);
            let new_value = (new_value * 100.0).round() / 100.0;
            return Some(new_value);
        }
    }

    None
}

// ─── Text Input ─────────────────────────────────────────────────────────────

/// Single-line text field. Returns `Some(text)` if changed.
pub fn text_input(
    ui: &mut egui::Ui,
    label: &str,
    text: &str,
    has_keyframes: bool,
) -> Option<String> {
    let available = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(available, ROW_HEIGHT), egui::Sense::hover());

    // Keyframe indicator
    let label_x = if has_keyframes {
        let dot_rect = egui::Rect::from_center_size(
            egui::pos2(rect.min.x + 6.0, rect.center().y),
            Vec2::new(5.0, 5.0),
        );
        ui.painter().rect_filled(dot_rect, 1.5, AMBER);
        rect.min.x + 14.0
    } else {
        rect.min.x + 8.0
    };

    // Label
    ui.painter().text(
        egui::pos2(label_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Small.resolve(ui.style()),
        LABEL_COLOR,
    );

    // Text field
    let field_width = (available - 90.0).max(60.0);
    let field_rect = egui::Rect::from_min_size(
        egui::pos2(rect.max.x - field_width - 4.0, rect.min.y + 1.0),
        Vec2::new(field_width, FIELD_HEIGHT),
    );

    let id = ui.id().with(("text", label));
    let mut text_buf = text.to_string();

    let bg = if ui.rect_contains_pointer(field_rect) {
        FIELD_BG_HOVER
    } else {
        FIELD_BG
    };
    ui.painter().rect_filled(field_rect, 4.0, bg);
    ui.painter().rect_stroke(field_rect, 4.0, Stroke::new(1.0, BORDER_COLOR), egui::StrokeKind::Outside);

    let click_response = ui.interact(field_rect, id.with("click"), egui::Sense::click());
    if click_response.clicked() {
        ui.memory_mut(|m| m.request_focus(id));
    }

    let is_focused = ui.memory(|m| m.has_focus(id));
    if is_focused {
        let edit = egui::TextEdit::singleline(&mut text_buf)
            .font(egui::TextStyle::Small)
            .desired_width(field_width - 8.0)
            .margin(Vec2::new(4.0, 2.0))
            .text_color(ACCENT_COLOR)
            .background_color(FIELD_BG);
        let response = ui.put(field_rect, edit);
        if response.lost_focus() && text_buf != text {
            return Some(text_buf);
        }
        None
    } else {
        let display = if text.len() > 25 {
            format!("{}…", &text[..25])
        } else {
            text.to_string()
        };
        ui.painter().text(
            egui::pos2(field_rect.min.x + 6.0, field_rect.center().y),
            egui::Align2::LEFT_CENTER,
            &display,
            egui::TextStyle::Small.resolve(ui.style()),
            ACCENT_COLOR,
        );
        None
    }
}

// ─── Enum Selector ──────────────────────────────────────────────────────────

/// Dropdown/combo box for enums. Returns `Some(value)` if selection changed.
pub fn enum_selector(
    ui: &mut egui::Ui,
    label: &str,
    current: &str,
    variants: &[&str],
    has_keyframes: bool,
) -> Option<String> {
    let available = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(available, ROW_HEIGHT), egui::Sense::hover());

    // Keyframe indicator
    let label_x = if has_keyframes {
        let dot_rect = egui::Rect::from_center_size(
            egui::pos2(rect.min.x + 6.0, rect.center().y),
            Vec2::new(5.0, 5.0),
        );
        ui.painter().rect_filled(dot_rect, 1.5, AMBER);
        rect.min.x + 14.0
    } else {
        rect.min.x + 8.0
    };

    // Label
    ui.painter().text(
        egui::pos2(label_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Small.resolve(ui.style()),
        LABEL_COLOR,
    );

    // Combo box
    let field_width = 90.0;
    let field_rect = egui::Rect::from_min_size(
        egui::pos2(rect.max.x - field_width - 4.0, rect.min.y + 1.0),
        Vec2::new(field_width, FIELD_HEIGHT),
    );

    let id = ui.id().with(("enum", label));

    // Paint background
    let bg = if ui.rect_contains_pointer(field_rect) {
        FIELD_BG_HOVER
    } else {
        FIELD_BG
    };
    ui.painter().rect_filled(field_rect, 4.0, bg);
    ui.painter().rect_stroke(field_rect, 4.0, Stroke::new(1.0, BORDER_COLOR), egui::StrokeKind::Outside);

    // Paint current value + dropdown arrow
    ui.painter().text(
        egui::pos2(field_rect.min.x + 6.0, field_rect.center().y),
        egui::Align2::LEFT_CENTER,
        current,
        egui::TextStyle::Small.resolve(ui.style()),
        ACCENT_COLOR,
    );
    ui.painter().text(
        egui::pos2(field_rect.max.x - 6.0, field_rect.center().y),
        egui::Align2::RIGHT_CENTER,
        "▾",
        egui::TextStyle::Small.resolve(ui.style()),
        LABEL_COLOR,
    );

    // Popup combo
    let mut changed = None;

    egui::ComboBox::from_id_salt(id.with("combo"))
        .selected_text(current)
        .width(field_width)
        .show_ui(ui, |ui| {
            for variant in variants {
                let is_selected = *variant == current;
                if ui
                    .selectable_label(is_selected, *variant)
                    .clicked()
                {
                    changed = Some(variant.to_string());
                }
            }
        });

    changed
}

// ─── Read-only Display Row ──────────────────────────────────────────────────

/// Renders a read-only property row (fallback for unsupported property types).
pub fn readonly_row(ui: &mut egui::Ui, label: &str, value: &str, has_keyframes: bool) {
    let available = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(available, ROW_HEIGHT), egui::Sense::hover());

    // Keyframe indicator
    let label_x = if has_keyframes {
        let dot_rect = egui::Rect::from_center_size(
            egui::pos2(rect.min.x + 6.0, rect.center().y),
            Vec2::new(5.0, 5.0),
        );
        ui.painter().rect_filled(dot_rect, 1.5, AMBER);
        rect.min.x + 14.0
    } else {
        rect.min.x + 8.0
    };

    // Label
    ui.painter().text(
        egui::pos2(label_x, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Small.resolve(ui.style()),
        LABEL_COLOR,
    );

    // Value (right-aligned)
    ui.painter().text(
        egui::pos2(rect.max.x - 6.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        value,
        egui::TextStyle::Small.resolve(ui.style()),
        VALUE_COLOR,
    );
}

// ─── Add Keyframe Button ────────────────────────────────────────────────────

/// Renders a small "add keyframe" button. Returns true if clicked.
#[allow(dead_code)]
pub fn add_keyframe_button(ui: &mut egui::Ui) -> bool {
    ui.add(egui::Button::new(RichText::new("+KF").size(9.0).color(AMBER)))
        .clicked()
}

// ─── Pill Tab Bar ───────────────────────────────────────────────────────────

/// Reusable pill-style segmented tab bar.
///
/// `tabs` is a slice of `(tab_value, icon, label)` tuples.
/// Returns `Some(new_tab)` if a different tab was clicked, else `None`.
///
/// Visual: BG_SURFACE background, active tab gets a BG_WIDGET pill,
/// inactive tabs show a subtle hover tint.
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
    ui.painter().rect_filled(bar_rect, 4.0, FIELD_BG);

    let mut clicked_tab = None;

    for (idx, (tab, icon, label)) in tabs.iter().enumerate() {
        let is_active = active_tab == *tab;
        let x = bar_rect.min.x + idx as f32 * (tab_w + gap);
        let tab_rect =
            egui::Rect::from_min_size(egui::pos2(x, bar_rect.min.y), Vec2::new(tab_w, tab_h));

        let response = ui.interact(tab_rect, ui.id().with(("pill_tab", idx)), egui::Sense::click());

        if is_active {
            let pill = tab_rect.shrink2(Vec2::new(2.0, 2.0));
            ui.painter().rect_filled(pill, 4.0, BG_WIDGET);
        } else if response.hovered() {
            // Subtle hover tint on inactive tabs
            let hover_bg = Color32::from_rgb(36, 40, 50);
            let pill = tab_rect.shrink2(Vec2::new(2.0, 2.0));
            ui.painter().rect_filled(pill, 4.0, hover_bg);
        }

        let text_color = if is_active {
            VALUE_COLOR
        } else {
            LABEL_COLOR
        };
        let text = format!("{}  {}", icon, label);
        ui.painter().text(
            tab_rect.center(),
            egui::Align2::CENTER_CENTER,
            text,
            egui::TextStyle::Small.resolve(ui.style()),
            text_color,
        );

        if response.clicked() {
            clicked_tab = Some(*tab);
        }
    }

    clicked_tab
}
