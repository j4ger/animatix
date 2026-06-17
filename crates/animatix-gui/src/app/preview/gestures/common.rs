//! Shared utilities for gesture handlers.
//! Extracted helpers that multiple handlers need.

use super::super::context::PreviewContext;
use crate::app::commands::{DragEvent, ShellAction};

/// Per-frame snapshot of pointer state for gesture dispatch.
#[derive(Clone, Debug)]
pub(crate) struct GestureFrame {
    pub screen_pos: Option<egui::Pos2>,
    pub modifiers: egui::Modifiers,
    pub drag_stopped: bool,
    pub any_down: bool,
    pub any_released: bool,
}

/// Shared drag-end lifecycle for all non-marquee handlers.
/// Must be called exactly once per drag end to preserve source-flush behavior.
pub(crate) fn finish_drag(
    ctx: &mut PreviewContext,
    old_drag_state: super::super::DragState,
) {
    // Finalize keyframes for move/scale/rotate
    super::super::drag_utils::finalize_drag_keyframes(&old_drag_state, ctx);

    // Push DragEnded exactly once — drives source flush and interaction reset in GuiShell
    ctx.commands.push_back(ShellAction::Drag(DragEvent::DragEnded));
}
