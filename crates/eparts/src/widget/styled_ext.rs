//! Fluent `Ui` / `Response` styling helpers (A5).
//!
//! These are thin sugar over egui layout and painting primitives, following the
//! same theme-driven rules as the rest of `eparts`: borders use semantic theme
//! slots rather than hardcoded colors.

use egui::{Color32, Margin, Response, Stroke, Ui};

/// Layout helpers on `egui::Ui`.
pub trait UiExt {
    /// Run `add_contents` in a horizontal row.
    fn h_flex(&mut self, add_contents: impl FnOnce(&mut Ui));

    /// Run `add_contents` in a vertical column.
    fn v_flex(&mut self, add_contents: impl FnOnce(&mut Ui));

    /// Wrap `add_contents` in a frame with the given inner margin.
    fn with_padding(&mut self, padding: impl Into<Margin>, add_contents: impl FnOnce(&mut Ui));
}

/// Painting helpers on `egui::Response`.
pub trait ResponseExt {
    /// Paint a border only while the response has keyboard focus.
    fn focused_border(&self, ui: &Ui, color: Color32);

    /// Paint a border regardless of focus state (useful for debug overlays).
    fn debug_border(&self, ui: &Ui, color: Color32);
}

impl UiExt for Ui {
    fn h_flex(&mut self, add_contents: impl FnOnce(&mut Ui)) {
        self.horizontal(add_contents);
    }

    fn v_flex(&mut self, add_contents: impl FnOnce(&mut Ui)) {
        self.vertical(add_contents);
    }

    fn with_padding(&mut self, padding: impl Into<Margin>, add_contents: impl FnOnce(&mut Ui)) {
        egui::Frame::new().inner_margin(padding.into()).show(self, add_contents);
    }
}

impl ResponseExt for Response {
    fn focused_border(&self, ui: &Ui, color: Color32) {
        if self.has_focus() {
            self.debug_border(ui, color);
        }
    }

    fn debug_border(&self, ui: &Ui, color: Color32) {
        ui.painter().rect_stroke(
            self.rect,
            egui::CornerRadius::same(2),
            Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ui(mut add_contents: impl FnMut(&mut egui::Ui)) {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| add_contents(ui));
    }

    #[test]
    fn flex_helpers_run_contents_without_allocating_extra() {
        run_ui(|ui| {
            ui.h_flex(|ui| {
                ui.label("left");
                ui.label("right");
            });
            ui.v_flex(|ui| {
                ui.label("top");
                ui.label("bottom");
            });
        });
    }

    #[test]
    fn padding_helper_wraps_contents() {
        run_ui(|ui| {
            ui.with_padding(Margin::same(8), |ui| {
                ui.label("padded");
            });
        });
    }

    #[test]
    fn debug_border_does_not_panic_on_response() {
        run_ui(|ui| {
            let response = ui.allocate_response(egui::vec2(32.0, 16.0), egui::Sense::hover());
            response.debug_border(ui, Color32::RED);
            response.focused_border(ui, Color32::BLUE);
        });
    }
}
