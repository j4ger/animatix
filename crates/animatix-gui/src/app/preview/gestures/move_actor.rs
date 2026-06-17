//! Gesture handler for body-move interactions — ToolMode::Move and Select body drag.
//!
//! Extracted from the legacy `drag_handler.rs` Move match arms.
//! Handles: hit-test actor body, Alt duplicate, Shift detach from layout,
//! multi-actor move with snap, grid, shift-lock, and keyframe finalization.

use crate::app::commands::{Command, DocumentCommand, DragEvent, PropertyEdit, PropertyValue, ShellAction};
use crate::app::preview::drag_utils;
use crate::app::preview::gesture::{Gesture, GestureHandler, GestureResult};
use crate::app::preview::{DragState, ToolMode};
pub(crate) struct MoveActorGesture;

impl GestureHandler for MoveActorGesture {
    fn handle(
        &mut self,
        gesture: &Gesture,
        ctx: &mut crate::app::preview::context::PreviewContext<'_>,
        preview_rect: egui::Rect,
    ) -> GestureResult {
        match gesture {
            Gesture::DragStart { pos, modifiers, .. } => {
                // Only handle Move or Select tool mode (NOT Rotate/Scale/Pivot/Vertex)
                match *ctx.tool_mode {
                    ToolMode::Move | ToolMode::Select => {},
                    _ => return GestureResult::Ignored,
                }

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
                let props = ctx.get_actor_props(&actor);
                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);

                // Hit test actor body or hit region
                let hit_body = drag_utils::hit_test_actor_body(scene, props.as_ref());
                let hit_region = ctx
                    .hit_regions
                    .iter()
                    .rev()
                    .any(|(label, bounds)| label == &actor && bounds.contains(scene));

                if !hit_body && !hit_region {
                    return GestureResult::Ignored;
                }

                // Layout-managed handling
                if ctx.is_layout_managed(&actor) {
                    if modifiers.shift {
                        // Shift detach: detach from layout before moving
                        let current_pos = props
                            .as_ref()
                            .map(|p| p.position)
                            .unwrap_or([scene.x as f32, scene.y as f32]);
                        ctx.commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor: actor.clone(),
                                property: "placement_mode".into(),
                                value: PropertyValue::Text("manual".into()),
                                create_keyframe: ctx.keyframe_mode,
                            })
                            .into(),
                        );
                        ctx.commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor: actor.clone(),
                                property: "position".into(),
                                value: PropertyValue::Vec2(current_pos),
                                create_keyframe: ctx.keyframe_mode,
                            })
                            .into(),
                        );
                    } else {
                        // Layout-managed without Shift: let legacy handler handle Reorder
                        return GestureResult::Ignored;
                    }
                }

                // Alt duplicate before drag
                if modifiers.alt {
                    ctx.commands.push_back(
                        ShellAction::Command(Command::DuplicateActor(actor.clone())),
                    );
                    return GestureResult::Claimed;
                }

                // Capture start positions for multi-select
                let actors = drag_utils::capture_start_positions(
                    ctx.selected_actors,
                    |a| ctx.get_actor_props(a),
                    ctx.hit_regions,
                );

                *ctx.drag_state = DragState::Move {
                    primary: actor,
                    actors,
                    start_scene: scene,
                };

                GestureResult::Claimed
            },
            Gesture::DragMove { pos, modifiers, .. } => {
                // Only handle if we are already in Move state
                let (_primary, actors, start_scene) = match &*ctx.drag_state {
                    DragState::Move {
                        primary,
                        actors,
                        start_scene,
                    } => (primary.clone(), actors.clone(), *start_scene),
                    _ => return GestureResult::Ignored,
                };

                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);

                // Compute scene delta
                let raw_dx = (scene.x - start_scene.x) as f32;
                let raw_dy = (scene.y - start_scene.y) as f32;

                // Shift locks to dominant axis
                let (dx, dy) = if modifiers.shift {
                    if raw_dx.abs() > raw_dy.abs() {
                        (raw_dx, 0.0)
                    } else {
                        (0.0, raw_dy)
                    }
                } else {
                    (raw_dx, raw_dy)
                };

                let snap_enabled =
                    ctx.preview.snap.snap_enabled && !modifiers.alt;
                let threshold = ctx.preview.snap.snap_threshold;
                let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;

                for (actor, start_position) in &actors {
                    let mut nx = start_position[0] + dx;
                    let mut ny = start_position[1] + dy;

                    // Grid snap
                    if ctx.preview.overlay.show_grid {
                        let grid = ctx.preview.overlay.grid_size;
                        nx = (nx / grid).round() * grid;
                        ny = (ny / grid).round() * grid;
                    }

                    // Guide/actor/container/keyframe snap
                    if snap_enabled {
                        let result =
                            drag_utils::resolve_snap(actor, nx, ny, threshold, time_ms, ctx);
                        nx = result.nx;
                        ny = result.ny;
                    }

                    // Emit position edit for each actor
                    drag_utils::emit_position_edit(actor.clone(), nx, ny, ctx);
                }

                GestureResult::Claimed
            },
            Gesture::DragEnd { .. } => {
                let old_drag_state = ctx.drag_state.clone();

                // Only handle if we were in Move state
                match &old_drag_state {
                    DragState::Move { .. } => {},
                    _ => return GestureResult::Ignored,
                }

                // Finalize keyframes (creates position keyframes for primary actor)
                drag_utils::finalize_drag_keyframes(&old_drag_state, ctx);
                ctx.commands.push_back(ShellAction::Drag(DragEvent::DragEnded));
                *ctx.drag_state = DragState::None;

                GestureResult::Claimed
            },
            _ => GestureResult::Ignored,
        }
    }
}
