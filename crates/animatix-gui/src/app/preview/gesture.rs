use egui::Pos2;

/// High-level gesture events produced by the gesture router.
#[derive(Debug, Clone)]
pub enum Gesture {
    Start { pos: Pos2 },
    Move { pos: Pos2 },
    End { modifiers: egui::Modifiers },
}

/// Result from a gesture handler indicating whether it claimed the gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GestureResult {
    Claimed,
    Ignored,
}

/// Trait for gesture handlers. Handlers are checked in priority order by the router.
pub trait GestureHandler {
    /// Handle a gesture event. Return Claimed if the gesture was handled and should not be
    /// passed to lower-priority handlers.
    fn handle(
        &mut self,
        gesture: &Gesture,
        ctx: &mut crate::app::preview::context::PreviewContext,
        preview_rect: egui::Rect,
    ) -> GestureResult;
}
