use egui::{Pos2, Vec2};

/// Mouse button for pointer events.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)] // Reserved for incremental gesture handler extraction (Phase 4, Steps 3-8)
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

/// High-level gesture events produced by the gesture router.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Reserved for incremental gesture handler extraction (Phase 4, Steps 3-8)
pub enum Gesture {
    Tap {
        pos: Pos2,
        button: PointerButton,
        modifiers: egui::Modifiers,
    },
    DoubleTap {
        pos: Pos2,
        button: PointerButton,
        modifiers: egui::Modifiers,
    },
    SecondaryTap {
        pos: Pos2,
        modifiers: egui::Modifiers,
    },
    DragStart {
        pos: Pos2,
        button: PointerButton,
        modifiers: egui::Modifiers,
    },
    DragMove {
        pos: Pos2,
        delta: Vec2,
        button: PointerButton,
        modifiers: egui::Modifiers,
    },
    DragEnd {
        pos: Pos2,
        button: PointerButton,
        modifiers: egui::Modifiers,
    },
    Hover {
        pos: Pos2,
        modifiers: egui::Modifiers,
    },
    ScrollZoom {
        delta: f32,
        pos: Pos2,
        modifiers: egui::Modifiers,
    },
}

/// Result from a gesture handler indicating whether it claimed the gesture.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)] // Reserved for incremental gesture handler extraction (Phase 4, Steps 3-8)
pub enum GestureResult {
    Claimed,
    Ignored,
}

/// Trait for gesture handlers. Handlers are checked in priority order by the router.
#[allow(dead_code)] // Reserved for incremental gesture handler extraction (Phase 4, Steps 3-8)
pub trait GestureHandler {
    /// Handle a gesture event. Return Claimed if the gesture was handled and should not be
    /// passed to lower-priority handlers.
    fn handle(&mut self, gesture: &Gesture, ctx: &mut crate::app::preview::context::PreviewContext, preview_rect: egui::Rect) -> GestureResult;
} 
