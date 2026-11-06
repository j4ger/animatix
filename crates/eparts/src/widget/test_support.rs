//! Shared helpers for widget unit tests: a standard screen and raw input.

use egui::{Pos2, RawInput, Rect};

/// A standard test screen sized large enough for full-width widgets.
pub(crate) fn screen() -> Rect {
    Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0))
}

/// Raw input on the default test screen.
pub(crate) fn base_input() -> RawInput {
    base_input_with(screen())
}

/// Raw input on a caller-supplied screen rect.
pub(crate) fn base_input_with(rect: Rect) -> RawInput {
    RawInput {
        screen_rect: Some(rect),
        ..Default::default()
    }
}
