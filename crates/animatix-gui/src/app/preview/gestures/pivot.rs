//! Gesture handler for ToolMode::Pivot – dragging the pivot crosshair.
//!
//! Extracted from the legacy `drag_handler.rs` MovePivot match arms.

use crate::app::commands::{DragEvent, ShellAction};
use crate::app::design_tokens::spatial::preview::HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS;
use crate::app::preview::DragState;
use crate::app::preview::drag_utils;
use crate::app::preview::gesture::{Gesture, GestureHandler, GestureResult};

pub(crate) struct PivotGesture;

impl GestureHandler for PivotGesture {
    fn handle(&mut self, gesture: &Gesture, ctx: &mut crate::app::preview::context::PreviewContext, preview_rect: egui::Rect) -> GestureResult {
        match gesture {
            Gesture::DragStart { pos, .. } => {
                // Only handle Pivot or Select tool mode
                match *ctx.tool_mode {
                    crate::app::preview::ToolMode::Pivot | crate::app::preview::ToolMode::Select => {},
                    _ => return GestureResult::Ignored,
                }

                let hit_radius = PREVIEW_HANDLE_HIT_RADIUS;

                // Get first selected actor
                let actor = match ctx.selected_actors.iter().next().cloned() {
                    Some(a) => a,
                    None => return GestureResult::Ignored,
                };

                // Check if locked
                let is_locked = ctx
                    .timeline
                    .and_then(|t| t.get_track(&actor))
                    .map(|tr| tr.locked)
                    .unwrap_or(false);
                if is_locked {
                    return GestureResult::Ignored;
                }

                // Get actor properties
                let props = match ctx.get_actor_props(&actor) {
                    Some(p) => p,
                    None => return GestureResult::Ignored,
                };

                // Hit-test the pivot crosshair
                let pivot_world_pt = crate::app::preview::pivot_world(&props);
                let pivot_screen = ctx.preview_scene_to_screen(
                    preview_rect,
                    kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64),
                );

                if !crate::app::preview::hit_test_pivot(*pos, pivot_screen, hit_radius) {
                    return GestureResult::Ignored;
                }

                // Convert screen pos to scene space for drag start reference
                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);

                *ctx.drag_state = DragState::MovePivot {
                    actor,
                    start_offset: props.pivot_offset,
                    start_scene: scene,
                };

                GestureResult::Claimed
            },
            Gesture::DragMove { pos, .. } => {
                // Only handle if we are already in MovePivot state
                let (actor, start_offset, start_scene) = match &*ctx.drag_state {
                    DragState::MovePivot {
                        actor,
                        start_offset,
                        start_scene,
                    } => (actor.clone(), *start_offset, *start_scene),
                    _ => return GestureResult::Ignored,
                };

                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);
                let dx = (scene.x - start_scene.x) as f32;
                let dy = (scene.y - start_scene.y) as f32;

                if let Some(p) = ctx.get_actor_props(&actor) {
                    let cos = (-p.rotation).cos();
                    let sin = (-p.rotation).sin();
                    let local_dx = dx * cos - dy * sin;
                    let local_dy = dx * sin + dy * cos;
                    ctx
                        .pivot_offsets
                        .insert(actor, [start_offset[0] + local_dx, start_offset[1] + local_dy]);
                }

                GestureResult::Claimed
            },
            Gesture::DragEnd { .. } => {
                let old_drag_state = ctx.drag_state.clone();

                // Only handle if we were in MovePivot state
                match &old_drag_state {
                    DragState::MovePivot { .. } => {},
                    _ => return GestureResult::Ignored,
                }

                // Finalize (MovePivot is a no-op in finalize_drag_keyframes)
                drag_utils::finalize_drag_keyframes(&old_drag_state, ctx);
                ctx.commands.push_back(ShellAction::Drag(DragEvent::DragEnded));

                *ctx.drag_state = DragState::None;

                GestureResult::Claimed
            },
            _ => GestureResult::Ignored,
        }
    }
}
