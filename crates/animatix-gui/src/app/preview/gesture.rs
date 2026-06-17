use egui::{Pos2, Vec2};
use crate::app::commands::ActionQueue;

/// Mouse button for pointer events.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

/// High-level gesture events produced by the gesture router.
#[derive(Debug, Clone)]
pub enum Gesture {
    Tap { pos: Pos2, button: PointerButton, modifiers: egui::Modifiers },
    DoubleTap { pos: Pos2, button: PointerButton, modifiers: egui::Modifiers },
    SecondaryTap { pos: Pos2, modifiers: egui::Modifiers },
    DragStart { pos: Pos2, button: PointerButton, modifiers: egui::Modifiers },
    DragMove { pos: Pos2, delta: Vec2, button: PointerButton, modifiers: egui::Modifiers },
    DragEnd { pos: Pos2, button: PointerButton, modifiers: egui::Modifiers },
    Hover { pos: Pos2, modifiers: egui::Modifiers },
    ScrollZoom { delta: f32, pos: Pos2, modifiers: egui::Modifiers },
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
    fn handle(&mut self, gesture: &Gesture, ctx: &mut GestureContext) -> GestureResult;
}

/// Context passed to gesture handlers, providing access to preview state and command emission.
pub struct GestureContext<'a> {
    pub commands: &'a mut ActionQueue,
    pub preview_ctx: &'a mut crate::app::preview::context::PreviewContext<'a>,
}
