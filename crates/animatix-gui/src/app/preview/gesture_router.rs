use super::DragState;
use super::context::PreviewContext;
use super::gesture::{Gesture, GestureHandler};
use egui::{Rect, Response, Ui};

pub struct GestureRouter;
impl GestureRouter {
    /// Review drags select a marquee, never transform runtime geometry.
    pub fn handle_review_gestures(
        ctx: &mut PreviewContext<'_>,
        ui: &mut Ui,
        preview_rect: Rect,
        response: &Response,
    ) {
        *ctx.drag_state = DragState::None;
        *ctx.tool_mode = super::ToolMode::Select;
        let modifiers = ui.input(|i| i.modifiers);
        let pos = ui.input(|i| i.pointer.latest_pos());
        let gesture = if response.drag_started_by(egui::PointerButton::Primary) {
            pos.map(|pos| Gesture::Start { pos, modifiers })
        } else if ctx.selection.marquee_start.is_some() {
            if response.drag_stopped_by(egui::PointerButton::Primary)
                || !ui.input(|i| i.pointer.primary_down())
            {
                Some(Gesture::End { modifiers })
            } else {
                pos.map(|pos| Gesture::Move { pos, modifiers })
            }
        } else {
            None
        };
        if let Some(gesture) = gesture {
            super::gestures::marquee::MarqueeGesture.handle(&gesture, ctx, preview_rect);
        }
    }
}
