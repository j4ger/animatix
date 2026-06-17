use super::context::PreviewContext;
use egui::{Rect, Response, Ui};

/// Routes raw egui pointer events into typed Gestures and dispatches them to handlers.
pub struct GestureRouter;

impl GestureRouter {
    /// Process pointer events from the preview canvas and dispatch to the current gesture handlers.
    /// Currently delegates all input to the legacy `drag_handler::handle_preview_drag`.
    ///
    /// In later phases (Steps 3-7), this method will accept an explicit commands queue so that
    /// extracted gesture handlers can emit commands directly. For now, the legacy drag handler
    /// accesses commands through `ctx.commands`.
    pub fn handle_preview_gestures(
        ctx: &mut PreviewContext<'_>,
        ui: &mut Ui,
        preview_rect: Rect,
        response: &Response,
    ) {
        // Phase 1: Delegate entirely to the legacy drag handler.
        // Gesture handlers will be extracted incrementally in Steps 3-7.
        super::drag_handler::handle_preview_drag(ctx, ui, preview_rect, response);
    }
}
