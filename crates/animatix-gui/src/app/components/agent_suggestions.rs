//! Agent Inline Suggestions
//!
//! Agent surfaces in four shapes:
//! - Inline suggestion: text hint near a property
//! - Lightweight toast: brief notification
//! - Diff card: show code diff; accept / reject
//! - Command bar: complex request entry (see nl_command_bar.rs)

use crate::app::theme::*;
use egui::{Color32, FontId, Pos2, Rect, RichText, Stroke, Vec2};

/// A lightweight toast notification.
pub struct Toast {
    pub message: String,
    pub icon: &'static str,
    pub color: Color32,
    /// Time remaining in seconds.
    pub ttl_s: f32,
}

impl Toast {
    pub fn new(message: impl Into<String>, icon: &'static str, color: Color32) -> Self {
        Self {
            message: message.into(),
            icon,
            color,
            ttl_s: 5.0,
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, egui_phosphor::regular::INFO, ACCENT_BLUE)
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self::new(message, egui_phosphor::regular::CHECK, GREEN)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message, egui_phosphor::regular::WARNING, AMBER)
    }
}

/// Render a stack of toasts in the bottom-right corner.
pub fn render_toasts(ui: &mut egui::Ui, toasts: &mut Vec<Toast>, dt: f32) {
    if toasts.is_empty() {
        return;
    }

    let screen = ui.ctx().viewport_rect();
    let mut y_offset = 0.0f32;

    toasts.retain_mut(|toast| {
        toast.ttl_s -= dt;
        if toast.ttl_s <= 0.0 {
            return false;
        }

        let fade = (toast.ttl_s / 1.0).min(1.0);
        let alpha = (fade * 255.0) as u8;

        let text = RichText::new(format!("{} {}", toast.icon, toast.message))
            .size(FONT_SIZE_S)
            .color(TEXT_PRIMARY);
        let galley = ui.painter().layout_no_wrap(
            text.text().to_string(),
            FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            TEXT_PRIMARY,
        );
        let padding = Vec2::new(SPACE_M, SPACE_S);
        let size = galley.size() + padding * 2.0;

        let pos = Pos2::new(
            screen.max.x - size.x - SPACE_L,
            screen.max.y - size.y - SPACE_L - y_offset,
        );
        let rect = Rect::from_min_size(pos, size);

        let bg_color = Color32::from_rgba_unmultiplied(
            BG_PANEL.r(), BG_PANEL.g(), BG_PANEL.b(),
            (180.0 * fade) as u8,
        );
        let border_color = Color32::from_rgba_unmultiplied(
            toast.color.r(), toast.color.g(), toast.color.b(),
            (alpha / 3).max(40),
        );

        ui.painter().rect_filled(rect, RADIUS_M, bg_color);
        ui.painter().rect_stroke(rect, RADIUS_M, Stroke::new(1.0, border_color), egui::StrokeKind::Outside);
        ui.painter().text(
            rect.min + padding,
            egui::Align2::LEFT_TOP,
            text.text(),
            FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            TEXT_PRIMARY,
        );

        y_offset += size.y + SPACE_S;
        true
    });
}

/// An inline suggestion shown near a property or actor.
pub struct InlineSuggestion {
    pub target_actor: String,
    pub target_property: Option<String>,
    pub message: String,
    pub accept_label: String,
    pub reject_label: String,
}

/// Render an inline suggestion anchored to a screen position.
pub fn render_inline_suggestion(
    ui: &mut egui::Ui,
    suggestion: &InlineSuggestion,
    anchor: Pos2,
    on_accept: impl FnOnce(),
    on_reject: impl FnOnce(),
) {
    let padding = Vec2::new(SPACE_M, SPACE_S);
    let text = RichText::new(format!("{} {}", egui_phosphor::regular::SPARKLE, suggestion.message))
        .size(FONT_SIZE_S)
        .color(TEXT_SECONDARY);
    let galley = ui.painter().layout_no_wrap(
        text.text().to_string(),
        FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
        TEXT_SECONDARY,
    );

    let btn_h = ROW_S;
    let btn_w = 40.0f32;
    let content_w = galley.size().x.max(btn_w * 2.0 + SPACE_S);
    let size = Vec2::new(
        content_w + padding.x * 2.0,
        galley.size().y + padding.y * 2.0 + btn_h + SPACE_S,
    );

    let rect = Rect::from_min_size(anchor, size);

    // Background
    ui.painter().rect_filled(rect, RADIUS_M, BG_SURFACE);
    ui.painter().rect_stroke(rect, RADIUS_M, Stroke::new(1.0, ACCENT_BLUE), egui::StrokeKind::Outside);

    // Message
    ui.painter().text(
        rect.min + padding,
        egui::Align2::LEFT_TOP,
        text.text(),
        FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
        TEXT_SECONDARY,
    );

    // Buttons
    let btn_y = rect.max.y - padding.y - btn_h;
    let accept_rect = Rect::from_min_size(
        Pos2::new(rect.min.x + padding.x, btn_y),
        Vec2::new(btn_w, btn_h),
    );
    let reject_rect = Rect::from_min_size(
        Pos2::new(rect.min.x + padding.x + btn_w + SPACE_S, btn_y),
        Vec2::new(btn_w, btn_h),
    );

    let accept_id = ui.id().with("suggest_accept");
    let reject_id = ui.id().with("suggest_reject");
    let accept_resp = ui.interact(accept_rect, accept_id, egui::Sense::click());
    let reject_resp = ui.interact(reject_rect, reject_id, egui::Sense::click());

    // Accept button (green)
    let accept_bg = if accept_resp.hovered() { GREEN } else { Color32::from_rgba_unmultiplied(GREEN.r(), GREEN.g(), GREEN.b(), 60) };
    ui.painter().rect_filled(accept_rect, RADIUS_S, accept_bg);
    ui.painter().text(
        accept_rect.center(),
        egui::Align2::CENTER_CENTER,
        &suggestion.accept_label,
        FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
        if accept_resp.hovered() { TEXT_PRIMARY } else { GREEN },
    );

    // Reject button (red)
    let reject_bg = if reject_resp.hovered() { RED } else { Color32::from_rgba_unmultiplied(RED.r(), RED.g(), RED.b(), 60) };
    ui.painter().rect_filled(reject_rect, RADIUS_S, reject_bg);
    ui.painter().text(
        reject_rect.center(),
        egui::Align2::CENTER_CENTER,
        &suggestion.reject_label,
        FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
        if reject_resp.hovered() { TEXT_PRIMARY } else { RED },
    );

    if accept_resp.clicked() {
        on_accept();
    }
    if reject_resp.clicked() {
        on_reject();
    }
}

/// A diff card showing before/after code snippets.
pub struct DiffCard {
    pub title: String,
    pub before: String,
    pub after: String,
}

/// Render a diff card as a floating panel.
pub fn render_diff_card(
    ui: &mut egui::Ui,
    card: &DiffCard,
    anchor: Pos2,
    on_accept: impl FnOnce(),
    on_reject: impl FnOnce(),
) {
    let padding = Vec2::new(SPACE_M, SPACE_S);
    let line_h = FONT_SIZE_M + 4.0;
    let before_lines = card.before.lines().count().max(1);
    let after_lines = card.after.lines().count().max(1);
    let content_h = line_h * (before_lines + after_lines + 3) as f32;
    let size = Vec2::new(360.0, content_h + padding.y * 2.0 + ROW_M + SPACE_S);

    let rect = Rect::from_min_size(anchor, size);

    // Background
    ui.painter().rect_filled(rect, RADIUS_L, BG_PANEL);
    ui.painter().rect_stroke(rect, RADIUS_L, Stroke::new(1.0, BORDER), egui::StrokeKind::Outside);

    // Title
    ui.painter().text(
        Pos2::new(rect.min.x + padding.x, rect.min.y + padding.y),
        egui::Align2::LEFT_TOP,
        format!("{} {}", egui_phosphor::regular::FILES, card.title),
        FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
        TEXT_PRIMARY,
    );

    // Before / After labels
    let code_y = rect.min.y + padding.y + FONT_SIZE_M + SPACE_S;
    let col_w = (size.x - padding.x * 3.0) / 2.0;

    // Before column
    let before_rect = Rect::from_min_size(
        Pos2::new(rect.min.x + padding.x, code_y),
        Vec2::new(col_w, line_h * before_lines as f32 + SPACE_S),
    );
    ui.painter().rect_filled(before_rect, RADIUS_S, Color32::from_rgba_unmultiplied(RED.r(), RED.g(), RED.b(), 20));
    ui.painter().text(
        Pos2::new(before_rect.min.x + SPACE_S, before_rect.min.y + 2.0),
        egui::Align2::LEFT_TOP,
        "Before",
        FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
        RED,
    );
    ui.painter().text(
        Pos2::new(before_rect.min.x + SPACE_S, before_rect.min.y + FONT_SIZE_XS + 4.0),
        egui::Align2::LEFT_TOP,
        &card.before,
        FontId::new(FONT_SIZE_XS, egui::FontFamily::Monospace),
        TEXT_SECONDARY,
    );

    // After column
    let after_rect = Rect::from_min_size(
        Pos2::new(rect.min.x + padding.x * 2.0 + col_w, code_y),
        Vec2::new(col_w, line_h * after_lines as f32 + SPACE_S),
    );
    ui.painter().rect_filled(after_rect, RADIUS_S, Color32::from_rgba_unmultiplied(GREEN.r(), GREEN.g(), GREEN.b(), 20));
    ui.painter().text(
        Pos2::new(after_rect.min.x + SPACE_S, after_rect.min.y + 2.0),
        egui::Align2::LEFT_TOP,
        "After",
        FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
        GREEN,
    );
    ui.painter().text(
        Pos2::new(after_rect.min.x + SPACE_S, after_rect.min.y + FONT_SIZE_XS + 4.0),
        egui::Align2::LEFT_TOP,
        &card.after,
        FontId::new(FONT_SIZE_XS, egui::FontFamily::Monospace),
        TEXT_SECONDARY,
    );

    // Accept / Reject buttons at bottom
    let btn_y = rect.max.y - padding.y - ROW_S;
    let btn_rect = Rect::from_min_size(
        Pos2::new(rect.center().x - 60.0, btn_y),
        Vec2::new(120.0, ROW_S),
    );
    let btn_id = ui.id().with("diff_accept");
    let btn_resp = ui.interact(btn_rect, btn_id, egui::Sense::click());
    let btn_bg = if btn_resp.hovered() { GREEN } else { Color32::from_rgba_unmultiplied(GREEN.r(), GREEN.g(), GREEN.b(), 60) };
    ui.painter().rect_filled(btn_rect, RADIUS_S, btn_bg);
    ui.painter().text(
        btn_rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{} Accept", egui_phosphor::regular::CHECK),
        FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
        if btn_resp.hovered() { TEXT_PRIMARY } else { GREEN },
    );

    if btn_resp.clicked() {
        on_accept();
    }

    // Reject (X) button in top-right corner
    let x_rect = Rect::from_min_size(
        Pos2::new(rect.max.x - ROW_S - 4.0, rect.min.y + 4.0),
        Vec2::splat(ROW_S),
    );
    let x_id = ui.id().with("diff_reject");
    let x_resp = ui.interact(x_rect, x_id, egui::Sense::click());
    ui.painter().text(
        x_rect.center(),
        egui::Align2::CENTER_CENTER,
        egui_phosphor::regular::X,
        FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
        if x_resp.hovered() { RED } else { TEXT_MUTED },
    );
    if x_resp.clicked() {
        on_reject();
    }
}
