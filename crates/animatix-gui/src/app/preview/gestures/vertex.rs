//! Gesture handler for vertex editing — dragging individual polygon vertices.
//!
//! Extracted from the legacy `drag_handler.rs` `EditVertices` match arms.
//! Active in both `ToolMode::Vertex` (generous hit zone) and `ToolMode::Select`
//! (standard hit zone, after other handle checks).

use crate::app::commands::{DocumentCommand, DragEvent, PropertyEdit, PropertyValue, ShellAction};
use crate::app::design_tokens::spatial::preview::HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS;
use crate::app::preview::DragState;
use crate::app::preview::drag_utils;
use crate::app::preview::gesture::{Gesture, GestureHandler, GestureResult};

pub(crate) struct VertexGesture;

impl GestureHandler for VertexGesture {
    fn handle(
        &mut self,
        gesture: &Gesture,
        ctx: &mut crate::app::preview::context::PreviewContext<'_>,
        preview_rect: egui::Rect,
    ) -> GestureResult {
        match gesture {
            Gesture::DragStart { pos, .. } => {
                // Only handle Vertex or Select tool modes
                match *ctx.tool_mode {
                    crate::app::preview::ToolMode::Vertex
                    | crate::app::preview::ToolMode::Select => {},
                    _ => return GestureResult::Ignored,
                }

                // Hit radius: larger in Vertex mode for easier grabbing
                let hit_radius = if *ctx.tool_mode == crate::app::preview::ToolMode::Vertex {
                    PREVIEW_HANDLE_HIT_RADIUS * 2.0
                } else {
                    PREVIEW_HANDLE_HIT_RADIUS
                };

                // Get first selected (unlocked) actor
                let actor = match ctx.selected_actors.iter().next().cloned() {
                    Some(a) => a,
                    None => return GestureResult::Ignored,
                };

                // Check locked
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

                // Evaluate points at current time
                let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                let points = match ctx
                    .timeline
                    .and_then(|t| t.get_track(&actor))
                    .and_then(|tr| tr.points.as_ref().map(|pt| pt.evaluate(time_ms)))
                {
                    Some(pts) if !pts.is_empty() => pts,
                    _ => return GestureResult::Ignored,
                };

                // Hit-test each vertex
                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);
                if let Some(vidx) = crate::app::preview::hit_test_vertex(
                    *pos,
                    &props,
                    &points,
                    preview_rect,
                    ctx.scene_dimensions,
                    preview_rect.size(),
                    hit_radius,
                    ctx.preview.viewport.preview_zoom,
                    ctx.preview.viewport.preview_pan,
                ) {
                    *ctx.drag_state = DragState::EditVertices {
                        actor,
                        vertex: vidx,
                        start_points: points.clone(),
                        start_scene: scene,
                    };
                    GestureResult::Claimed
                } else {
                    GestureResult::Ignored
                }
            },
            Gesture::DragMove { pos, .. } => {
                // Only handle if we are already in EditVertices state
                let (actor, vertex, start_points, start_scene) = match &*ctx.drag_state {
                    DragState::EditVertices {
                        actor,
                        vertex,
                        start_points,
                        start_scene,
                    } => (actor.clone(), *vertex, start_points.clone(), *start_scene),
                    _ => return GestureResult::Ignored,
                };

                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);
                let dx = (scene.x - start_scene.x) as f32;
                let dy = (scene.y - start_scene.y) as f32;

                let mut new_points = start_points.clone();
                if let Some(p) = ctx.get_actor_props(&actor) {
                    let cos = (-p.rotation).cos();
                    let sin = (-p.rotation).sin();
                    let local_dx = dx * cos - dy * sin;
                    let local_dy = dx * sin + dy * cos;
                    if let Some(pt) = new_points.get_mut(vertex) {
                        pt[0] += local_dx;
                        pt[1] += local_dy;
                    }
                }

                ctx.commands.push_back(
                    DocumentCommand::PropertyEdit(PropertyEdit {
                        time_s: None,
                        actor,
                        property: "points".into(),
                        value: PropertyValue::PointList(new_points),
                        create_keyframe: ctx.keyframe_mode,
                    })
                    .into(),
                );

                GestureResult::Claimed
            },
            Gesture::DragEnd { .. } => {
                let old_drag_state = ctx.drag_state.clone();

                // Only handle if we were in EditVertices state
                match &old_drag_state {
                    DragState::EditVertices { .. } => {},
                    _ => return GestureResult::Ignored,
                }

                // Finalize (EditVertices is a no-op in finalize_drag_keyframes)
                drag_utils::finalize_drag_keyframes(&old_drag_state, ctx);
                ctx.commands
                    .push_back(ShellAction::Drag(DragEvent::DragEnded));

                *ctx.drag_state = DragState::None;

                GestureResult::Claimed
            },
            _ => GestureResult::Ignored,
        }
    }
}
