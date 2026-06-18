//! Gesture handler for dragging motion path keyframe control points.
//!
//! Allows the user to reposition keyframes on the motion path by clicking and
//! dragging the keyframe dots shown when motion paths are visible.
//!
//! Extracted from the legacy `drag_handler.rs` MotionPath match arms.

use crate::app::commands::{DocumentCommand, DragEvent, PropertyEdit, PropertyValue, ShellAction};
use crate::app::design_tokens::spatial::preview::HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS;
use crate::app::preview::DragState;
use crate::app::preview::drag_utils;
use crate::app::preview::gesture::{Gesture, GestureHandler, GestureResult};

pub(crate) struct MotionPathGesture;

impl GestureHandler for MotionPathGesture {
    fn handle(
        &mut self,
        gesture: &Gesture,
        ctx: &mut crate::app::preview::context::PreviewContext<'_>,
        preview_rect: egui::Rect,
    ) -> GestureResult {
        match gesture {
            Gesture::DragStart { pos, .. } => {
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

                // Get scene position for drag start reference
                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);

                // Scan position keyframes for a hit
                let hit_radius = PREVIEW_HANDLE_HIT_RADIUS;
                if let Some(timeline) = ctx.timeline {
                    if let Some(track) = timeline.get_track(&actor) {
                        if let Some(pos_track) = &track.geometry.position {
                            for (&time_ms, (kf_pos, _)) in pos_track.keyframes() {
                                let screen = ctx.preview_scene_to_screen(
                                    preview_rect,
                                    kurbo::Point::new(kf_pos[0] as f64, kf_pos[1] as f64),
                                );
                                if pos.distance(screen) <= hit_radius * 2.0 {
                                    *ctx.drag_state = DragState::MotionPath {
                                        actor: actor.clone(),
                                        time_ms,
                                        start_position: *kf_pos,
                                        start_scene: scene,
                                    };
                                    return GestureResult::Claimed;
                                }
                            }
                        }
                    }
                }

                GestureResult::Ignored
            },
            Gesture::DragMove { pos, .. } => {
                // Only handle if we are already in MotionPath state
                let (actor, time_ms, start_position, start_scene) = match &*ctx.drag_state {
                    DragState::MotionPath {
                        actor,
                        time_ms,
                        start_position,
                        start_scene,
                    } => (actor.clone(), *time_ms, *start_position, *start_scene),
                    _ => return GestureResult::Ignored,
                };

                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);
                let dx = (scene.x - start_scene.x) as f32;
                let dy = (scene.y - start_scene.y) as f32;
                let new_pos = [start_position[0] + dx, start_position[1] + dy];

                ctx.commands.push_back(
                    DocumentCommand::PropertyEdit(PropertyEdit {
                        actor,
                        property: "position".into(),
                        value: PropertyValue::Vec2(new_pos),
                        time_s: Some(time_ms as f64 / 1000.0),
                        create_keyframe: true,
                    })
                    .into(),
                );

                GestureResult::Claimed
            },
            Gesture::DragEnd { .. } => {
                let old_drag_state = ctx.drag_state.clone();

                // Only handle if we were in MotionPath state
                match &old_drag_state {
                    DragState::MotionPath { .. } => {},
                    _ => return GestureResult::Ignored,
                }

                // Finalize (MotionPath is a no-op in finalize_drag_keyframes)
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
