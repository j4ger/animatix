//! Fluent `Ui` styling helpers (A5).
//!
//! These are thin sugar over egui layout primitives, following the same
//! theme-driven rules as the rest of `eparts`.

use egui::{IntoAtoms, Response, Stroke, Ui, Widget};

/// Layout helpers on `egui::Ui`.
pub trait UiExt {
    /// Add a selectable label whose interaction states keep the same layout size.
    ///
    /// Unlike `Ui::selectable_label`, unselected labels still reserve the same
    /// frame margin that hover/selection will paint, so interaction only changes
    /// paint state and never the row's layout cursor.
    fn stable_selectable_label<'a>(&mut self, selected: bool, text: impl IntoAtoms<'a>)
    -> Response;
}

impl UiExt for Ui {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let _ = crate::run_ui(&ctx, input(), |ui| {
            normal = Some(ui.stable_selectable_label(false, "Grid"));
            normal_sibling = Some(ui.label("next"));
        });

        let pointer = normal.as_ref().expect("normal response").rect.center();
        let mut hovered = None;
        let mut hovered_sibling = None;
        let mut hover_input = input();
        hover_input.events.push(egui::Event::PointerMoved(pointer));
        let _ = crate::run_ui(&ctx, hover_input, |ui| {
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
        let _ = crate::run_ui(&ctx, input(), |ui| {
            normal = Some(ui.add(crate::widget::button::Button::primary("Create")));
        });

        let pointer = normal.as_ref().expect("normal response").rect.center();
        let mut hovered = None;
        let mut hover_input = input();
        hover_input.events.push(egui::Event::PointerMoved(pointer));
        let _ = crate::run_ui(&ctx, hover_input, |ui| {
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
        let _ = crate::run_ui(&ctx, active_input, |ui| {
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
