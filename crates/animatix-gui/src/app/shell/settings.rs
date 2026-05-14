use egui::{Color32, RichText, Stroke, Vec2};

use crate::app::components;
use crate::app::theme::*;

use crate::app::GuiShell;

impl GuiShell {
    pub(crate) fn settings_dialog_ui(&mut self, ui: &mut egui::Ui) {
        let screen_rect = ui.ctx().viewport_rect();

        // Dark semi-transparent backdrop
        ui.painter().rect_filled(
            screen_rect,
            0.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, 120),
        );

        // Capture clicks on backdrop to close
        let backdrop_response = ui.interact(
            screen_rect,
            ui.id().with("settings_backdrop"),
            egui::Sense::click(),
        );
        if backdrop_response.clicked() {
            self.settings_open = false;
        }

        // Close on Escape
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
        }

        // Centered dialog
        let dialog_w = 420.0;
        let dialog_h = 280.0;
        let dialog_rect = egui::Rect::from_center_size(
            screen_rect.center(),
            egui::Vec2::new(dialog_w, dialog_h),
        );

        // Dialog background
        ui.painter().rect_filled(dialog_rect, RADIUS_XL, BG_BASE);
        ui.painter().rect_stroke(
            dialog_rect,
            RADIUS_XL,
            Stroke::new(1.0, BORDER),
            egui::StrokeKind::Inside,
        );

        // Content area with margin
        let content_rect = dialog_rect.shrink(SPACE_XL * 2.0);
        let mut cursor_y = content_rect.top();

        // Title row
        ui.painter().text(
            egui::pos2(content_rect.left(), cursor_y + 14.0),
            egui::Align2::LEFT_CENTER,
            "Settings",
            egui::FontId::new(FONT_SIZE_XL, egui::FontFamily::Proportional),
            TEXT_PRIMARY,
        );

        // Close button (X)
        let close_size = Vec2::new(ROW_L, ROW_L);
        let close_rect =
            egui::Rect::from_min_size(egui::pos2(content_rect.right() - close_size.x, cursor_y), close_size);
        let close_resp = ui.interact(close_rect, ui.id().with("settings_close"), egui::Sense::click());
        let close_color = if close_resp.hovered() {
            TEXT_PRIMARY
        } else {
            TEXT_MUTED
        };
        ui.painter().text(
            close_rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::X,
            egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
            close_color,
        );
        if close_resp.clicked() {
            self.settings_open = false;
        }

        cursor_y += ROW_L + SPACE_L;

        // Divider
        ui.painter().line_segment(
            [
                egui::pos2(content_rect.left(), cursor_y),
                egui::pos2(content_rect.right(), cursor_y),
            ],
            Stroke::new(1.0, BORDER),
        );
        cursor_y += SPACE_L;

        // ── Keyframe merge window setting ──
        let label = "Keyframe merge window";
        let label_font = egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional);
        let galley = ui.painter().layout(
            label.to_string(),
            label_font.clone(),
            TEXT_SECONDARY,
            f32::INFINITY,
        );
        ui.painter().galley(
            egui::pos2(content_rect.left(), cursor_y),
            galley,
            TEXT_SECONDARY,
        );
        cursor_y += ROW_S + SPACE_XS;

        let mut value_ms = (self.keyframe_merge_window_s * 1000.0) as f32;
        let value_rect = egui::Rect::from_min_size(
            egui::pos2(content_rect.left(), cursor_y),
            Vec2::new(120.0, ROW_M),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(value_rect), |ui| {
            components::field(ui, |ui| {
                ui.style_mut().spacing.item_spacing = Vec2::new(4.0, 0.0);
                ui.add(
                    egui::DragValue::new(&mut value_ms)
                        .speed(1.0)
                        .range(0.0..=500.0)
                        .suffix(" ms"),
                );
            });
        });
        self.keyframe_merge_window_s = (value_ms as f64 / 1000.0).max(0.0);
        cursor_y += ROW_M + SPACE_M;

        // Description
        let desc = "Edits within this window of the previous keyframe are merged\ninstead of creating a new timestamp.";
        let desc_font = egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional);
        let desc_galley = ui.painter().layout(
            desc.to_string(),
            desc_font,
            TEXT_MUTED,
            content_rect.width(),
        );
        ui.painter().galley(
            egui::pos2(content_rect.left(), cursor_y),
            desc_galley,
            TEXT_MUTED,
        );
        cursor_y += SPACE_XL * 2.0 + SPACE_S;

        // ── Debug bounds toggle ──
        let toggle_rect = egui::Rect::from_min_size(
            egui::pos2(content_rect.left(), cursor_y),
            Vec2::new(content_rect.width(), ROW_M),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(toggle_rect), |ui| {
            ui.horizontal(|ui| {
                let mut debug = self.debug_bounds;
                ui.checkbox(
                    &mut debug,
                    RichText::new("Draw debug bounding boxes")
                        .size(FONT_SIZE_S)
                        .color(TEXT_SECONDARY),
                );
                self.debug_bounds = debug;
            });
        });

        cursor_y += ROW_M + SPACE_M;

        let desc2 = "Shows per-actor content bounding boxes on the preview canvas\nand in exported output.";
        let desc2_font = egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional);
        let desc2_galley = ui.painter().layout(
            desc2.to_string(),
            desc2_font,
            TEXT_MUTED,
            content_rect.width(),
        );
        ui.painter().galley(
            egui::pos2(content_rect.left(), cursor_y),
            desc2_galley,
            TEXT_MUTED,
        );
    }
}
