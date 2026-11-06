//! Shared utilities for gesture handlers.

use super::super::context::PreviewContext;
use crate::app::commands::{DragEvent, ShellAction};
use crate::app::preview::ActorProps;

/// First selected actor, unless it is locked.
pub(crate) fn selected_unlocked_actor(ctx: &PreviewContext) -> Option<String> {
    let actor = ctx.selected_actors.iter().next().cloned()?;
    if ctx.timeline.is_some_and(|t| t.is_actor_locked(&actor)) {
        return None;
    }
    Some(actor)
}

/// First selected actor plus its evaluated props; `None` when nothing is
/// selected, the actor is locked, or its props cannot be evaluated.
pub(crate) fn selected_actor_start(ctx: &PreviewContext) -> Option<(String, ActorProps)> {
    let actor = selected_unlocked_actor(ctx)?;
    let props = ctx.get_actor_props(&actor)?;
    Some((actor, props))
}

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
pub(crate) fn finish_drag(ctx: &mut PreviewContext, old_drag_state: super::super::DragState) {
    // Finalize keyframes for move/scale/rotate
    super::super::drag_utils::finalize_drag_keyframes(&old_drag_state, ctx);

    // Push DragEnded exactly once — drives source flush and interaction reset in GuiShell
    ctx.commands.push_back(ShellAction::Drag(DragEvent::DragEnded));
}
