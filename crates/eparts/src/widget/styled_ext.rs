//! Fluent `Ui` / `Response` styling helpers (A5).
//!
//! These are thin sugar over egui layout and painting primitives, following the
//! same theme-driven rules as the rest of `eparts`: borders use semantic theme
//! slots rather than hardcoded colors.

use egui::{Color32, IntoAtoms, Margin, Response, Stroke, Ui, Widget};

/// Layout helpers on `egui::Ui`.
pub trait UiExt {
    /// Run `add_contents` in a horizontal row.
    fn h_flex(&mut self, add_contents: impl FnOnce(&mut Ui));

    /// Run `add_contents` in a vertical column.
    fn v_flex(&mut self, add_contents: impl FnOnce(&mut Ui));

    /// Wrap `add_contents` in a frame with the given inner margin.
    fn with_padding(&mut self, padding: impl Into<Margin>, add_contents: impl FnOnce(&mut Ui));

    /// Add a selectable label whose interaction states keep the same layout size.
    ///
    /// Unlike `Ui::selectable_label`, unselected labels still reserve the same
    /// frame margin that hover/selection will paint, so interaction only changes
    /// paint state and never the row's layout cursor.
    fn stable_selectable_label<'a>(&mut self, selected: bool, text: impl IntoAtoms<'a>)
    -> Response;
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

    fn stable_selectable_label<'a>(
        &mut self,
        selected: bool,
        text: impl IntoAtoms<'a>,
    ) -> Response {
        let original_style: egui::Style = self.style().as_ref().clone();
        if !selected {
            // Reserve the hover/selected frame's width without painting an
            // inactive border, so the label stays visually frame-free while its
            // allocated width remains identical across interaction states.
            self.style_mut().visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        }
        let response =
            egui::Button::selectable(selected, text).frame_when_inactive(selected).ui(self);
        *self.style_mut() = original_style;
        response
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

    #[test]
    fn stable_selectable_label_keeps_sibling_position_on_hover() {
        let ctx = egui::Context::default();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 80.0));
        let input = || egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };

        let mut normal = None;
        let mut normal_sibling = None;
        let _ = ctx.run_ui(input(), |ui| {
            normal = Some(ui.stable_selectable_label(false, "Grid"));
            normal_sibling = Some(ui.label("next"));
        });

        let pointer = normal.as_ref().expect("normal response").rect.center();
        let mut hovered = None;
        let mut hovered_sibling = None;
        let mut hover_input = input();
        hover_input.events.push(egui::Event::PointerMoved(pointer));
        let _ = ctx.run_ui(hover_input, |ui| {
            hovered = Some(ui.stable_selectable_label(false, "Grid"));
            hovered_sibling = Some(ui.label("next"));
        });

        let normal = normal.expect("normal response");
        let normal_sibling = normal_sibling.expect("normal sibling response");
        let hovered = hovered.expect("hovered response");
        let hovered_sibling = hovered_sibling.expect("hovered sibling response");

        assert!(hovered.hovered());
        assert_eq!(normal.rect.size(), hovered.rect.size());
        assert_eq!(normal_sibling.rect.min.x, hovered_sibling.rect.min.x);
    }

    #[test]
    fn eparts_button_keeps_rect_across_interaction_states() {
        let ctx = egui::Context::default();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 80.0));
        let input = || egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };

        let mut normal = None;
        let _ = ctx.run_ui(input(), |ui| {
            normal = Some(ui.add(crate::widget::button::Button::primary("Create")));
        });

        let pointer = normal.as_ref().expect("normal response").rect.center();
        let mut hovered = None;
        let mut hover_input = input();
        hover_input.events.push(egui::Event::PointerMoved(pointer));
        let _ = ctx.run_ui(hover_input, |ui| {
            hovered = Some(ui.add(crate::widget::button::Button::primary("Create")));
        });

        let mut active = None;
        let mut active_input = input();
        active_input.events.push(egui::Event::PointerMoved(pointer));
        active_input.events.push(egui::Event::PointerButton {
            pos: pointer,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        let _ = ctx.run_ui(active_input, |ui| {
            active = Some(ui.add(crate::widget::button::Button::primary("Create")));
        });

        let normal = normal.expect("normal response");
        let hovered = hovered.expect("hovered response");
        let active = active.expect("active response");

        assert!(hovered.hovered());
        assert!(active.is_pointer_button_down_on());
        assert_eq!(normal.rect.size(), hovered.rect.size());
        assert_eq!(normal.rect.size(), active.rect.size());
    }
}
