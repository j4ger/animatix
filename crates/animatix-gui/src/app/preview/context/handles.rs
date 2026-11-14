use super::PreviewContext;
use crate::app::design_tokens::spatial::STROKE_WIDTH;

impl PreviewContext<'_> {
    /// Bounds are inspection affordances, not resize handles.
    pub(crate) fn render_review_selection_overlay(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
    ) {
        let theme = eparts::theme(ui);
        let tx = self.preview_transform(preview_rect);
        for (label, bounds) in self.hit_regions {
            if self.selected_actors.contains(label) {
                let rect = egui::Rect::from_two_pos(
                    tx.scene_to_screen(kurbo::Point::new(bounds.x0, bounds.y0)),
                    tx.scene_to_screen(kurbo::Point::new(bounds.x1, bounds.y1)),
                );
                ui.painter().rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(1.0, theme.palette.accent.primary),
                    egui::StrokeKind::Outside,
                );
            }
        }
        self.render_marquee(ui, theme);
    }

    /// Rubber-band rectangle for an in-progress marquee selection.
    fn render_marquee(&self, ui: &egui::Ui, theme: eparts::Theme) {
        let (Some(start), Some(current)) =
            (self.selection.marquee_start, self.selection.marquee_current)
        else {
            return;
        };
        let marquee_rect = egui::Rect::from_two_pos(start, current);
        ui.painter().rect_filled(marquee_rect, 0.0, theme.palette.accent.faint);
        ui.painter().rect_stroke(
            marquee_rect,
            0.0,
            egui::Stroke::new(STROKE_WIDTH, theme.palette.accent.subtle),
            egui::StrokeKind::Outside,
        );
    }
}
