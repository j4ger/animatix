//! Gesture handler for layout-managed child reordering — drag within a Row/Col container.
//!
//! Extracted from the legacy `drag_handler.rs` Reorder match arms.
//! Only activates for layout-managed children when Shift is NOT held (Shift detaches).
//! Projects mouse position onto the Row/Col main axis to determine the target insertion index.

use crate::app::commands::{DocumentCommand, PropertyEdit, PropertyValue};
use crate::app::preview::DragState;
use crate::app::preview::drag_utils;
use crate::app::preview::gesture::{Gesture, GestureHandler, GestureResult};
use crate::app::preview::gestures::common::finish_drag;

pub(crate) struct ReorderGesture;

impl GestureHandler for ReorderGesture {
    fn handle(
        &mut self,
        gesture: &Gesture,
        ctx: &mut crate::app::preview::context::PreviewContext<'_>,
        preview_rect: egui::Rect,
    ) -> GestureResult {
        match gesture {
            Gesture::DragStart { pos, modifiers, .. } => {
                // Reorder only fires on the drag start of the first real movement,
                // not on a click. We use `drag_started` which is driven by egui's
                // drag_started() — indicating actual pixel movement.
                // However, the gesture router calls DragStart on every drag_started()
                // response, so we gate on `response.drag_started()` inside the caller.
                // Here we simply check preconditions.

                // Must have a selected actor
                let actor = match ctx.selected_actors.iter().next().cloned() {
                    Some(a) => a,
                    None => return GestureResult::Ignored,
                };

                // Only for layout-managed children
                if !ctx.is_layout_managed(&actor) {
                    return GestureResult::Ignored;
                }

                // Shift means detach, not reorder
                if modifiers.shift {
                    return GestureResult::Ignored;
                }

                // Check if locked
                let is_locked = ctx
                    .timeline
                    .and_then(|t| t.get_track(&actor))
                    .map(|tr| tr.locked)
                    .unwrap_or(false);
                if is_locked {
                    return GestureResult::Ignored;
                }

                // Find the layout container and source index
                let (container, layout_type, source_index) = match ctx.find_layout_container(&actor)
                {
                    Some(info) => info,
                    None => return GestureResult::Ignored,
                };

                *ctx.drag_state = DragState::Reorder {
                    actor,
                    container,
                    source_index,
                    target_index: source_index,
                    layout_type,
                };

                GestureResult::Claimed
            },
            Gesture::DragMove { pos, .. } => {
                // Only handle if we are already in Reorder state
                let (actor, container, layout_type) = match &*ctx.drag_state {
                    DragState::Reorder {
                        actor,
                        container,
                        layout_type,
                        ..
                    } => (actor.clone(), container.clone(), *layout_type),
                    _ => return GestureResult::Ignored,
                };

                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);
                let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;

                if let Some(timeline) = ctx.timeline {
                    let order = timeline.get_child_order(&container, time_ms);
                    let siblings: Vec<String> = order.into_iter().filter(|l| l != &actor).collect();
                    let positions: Vec<f32> = siblings
                        .iter()
                        .map(|label| {
                            ctx.hit_regions
                                .iter()
                                .find(|(l, _)| l == label)
                                .map(|(_, bounds)| {
                                    if layout_type == animatix::timeline::LayoutType::Row {
                                        (bounds.x0 + bounds.x1) as f32 / 2.0
                                    } else {
                                        (bounds.y0 + bounds.y1) as f32 / 2.0
                                    }
                                })
                                .or_else(|| {
                                    ctx.get_actor_props(label).map(|p| {
                                        if layout_type == animatix::timeline::LayoutType::Row {
                                            p.position[0]
                                        } else {
                                            p.position[1]
                                        }
                                    })
                                })
                                .unwrap_or(if layout_type == animatix::timeline::LayoutType::Row {
                                    scene.x as f32
                                } else {
                                    scene.y as f32
                                })
                        })
                        .collect();

                    let mouse_coord = if layout_type == animatix::timeline::LayoutType::Row {
                        scene.x as f32
                    } else {
                        scene.y as f32
                    };
                    let mut insert_at = positions.len();
                    for (idx, coord) in positions.iter().enumerate() {
                        if mouse_coord < *coord {
                            insert_at = idx;
                            break;
                        }
                    }
                    if let DragState::Reorder { target_index, .. } = &mut *ctx.drag_state {
                        *target_index = insert_at;
                    }
                }

                GestureResult::Claimed
            },
            Gesture::DragEnd { .. } => {
                let old_drag_state = ctx.drag_state.clone();

                // Only handle if we were in Reorder state
                let (actor, container, source_index, target_index) = match &old_drag_state {
                    DragState::Reorder {
                        actor,
                        container,
                        source_index,
                        target_index,
                        ..
                    } => (actor.clone(), container.clone(), *source_index, *target_index),
                    _ => return GestureResult::Ignored,
                };

                // Finalize keyframes (no-op for reorder, but preserves convention)
                drag_utils::finalize_drag_keyframes(&old_drag_state, ctx);

                // Emit child_order property edit if the index changed
                if source_index != target_index {
                    let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                    if let Some(timeline) = ctx.timeline {
                        let mut new_order = timeline.get_child_order(&container, time_ms);
                        if let Some(pos) = new_order.iter().position(|label| label == &actor) {
                            let item = new_order.remove(pos);
                            let insert_at = target_index.min(new_order.len());
                            new_order.insert(insert_at, item);
                            ctx.commands.push_back(
                                DocumentCommand::PropertyEdit(PropertyEdit {
                                    time_s: None,
                                    actor: container,
                                    property: "child_order".into(),
                                    value: PropertyValue::StringList(new_order),
                                    create_keyframe: ctx.keyframe_mode,
                                })
                                .into(),
                            );
                        }
                    }
                }

                // Use shared finish_drag to emit DragEnded and reset
                finish_drag(ctx, old_drag_state);
                *ctx.drag_state = DragState::None;

                GestureResult::Claimed
            },
            _ => GestureResult::Ignored,
        }
    }
}
