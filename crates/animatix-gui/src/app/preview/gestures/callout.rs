//! Gesture handler for Callout actor handle interactions.
//!
//! Two handles per selected Callout:
//! - **Tip handle** (diamond, at scene `to`): dragging updates `to` (manual) or `to_offset` (targeted).
//! - **Label handle** (circle, at `to + label_at`): dragging updates `label_at`.

use crate::app::commands::{DocumentCommand, DragEvent, PropertyEdit, PropertyValue, ShellAction};
use crate::app::design_tokens::spatial::preview::HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS;
use crate::app::preview::DragState;
use crate::app::preview::gesture::{Gesture, GestureHandler, GestureResult};
use animatix::timeline::{ActorKindId, TrackAccessor};

pub(crate) struct CalloutGesture;

impl GestureHandler for CalloutGesture {
    fn handle(
        &mut self,
        gesture: &Gesture,
        ctx: &mut crate::app::preview::context::PreviewContext<'_>,
        preview_rect: egui::Rect,
    ) -> GestureResult {
        match gesture {
            Gesture::DragStart { pos, .. } => {
                let actor = match ctx.selected_actors.iter().next().cloned() {
                    Some(a) => a,
                    None => return GestureResult::Ignored,
                };

                // Only handle Callout actors
                let is_callout = ctx
                    .timeline
                    .and_then(|t| t.get_track(&actor))
                    .map(|tr| tr.kind == ActorKindId::Callout)
                    .unwrap_or(false);
                if !is_callout {
                    return GestureResult::Ignored;
                }

                let is_locked = ctx
                    .timeline
                    .and_then(|t| t.get_track(&actor))
                    .map(|tr| tr.locked)
                    .unwrap_or(false);
                if is_locked {
                    return GestureResult::Ignored;
                }

                let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                let timeline = match ctx.timeline {
                    Some(t) => t,
                    None => return GestureResult::Ignored,
                };
                let track = match timeline.get_track(&actor) {
                    Some(t) => t,
                    None => return GestureResult::Ignored,
                };

                let to = crate::app::preview::callout_effective_to(track, timeline, time_ms, ctx.scene_dimensions);
                let label_at = track.geometry.label_at.get(time_ms, [0.0, 50.0]);
                let is_targeted = !track.geometry.callout_target.get(time_ms, String::new()).is_empty();
                let to_offset = track.geometry.callout_to_offset.get(time_ms, [0.0, 0.0]);

                let zoom = ctx.preview.viewport.preview_zoom;
                let pan = ctx.preview.viewport.preview_pan;

                let tip_world = kurbo::Point::new(to[0] as f64, to[1] as f64);
                let label_world = kurbo::Point::new(
                    (to[0] + label_at[0]) as f64,
                    (to[1] + label_at[1]) as f64,
                );
                let tx = crate::app::preview::PreviewTransform::new(
                    ctx.scene_dimensions,
                    preview_rect,
                    zoom,
                    pan,
                );
                let tip_screen = tx.scene_to_screen(tip_world);
                let label_screen = tx.scene_to_screen(label_world);

                let hit_r = PREVIEW_HANDLE_HIT_RADIUS;
                let scene_start = ctx.preview_screen_to_scene(preview_rect, *pos);

                // Priority: tip first, then label
                if pos.distance(tip_screen) <= hit_r {
                    let start_value = if is_targeted { to_offset } else { to };
                    *ctx.drag_state = DragState::CalloutTip {
                        actor,
                        is_targeted,
                        start_value,
                        start_scene: scene_start,
                    };
                    return GestureResult::Claimed;
                }

                if pos.distance(label_screen) <= hit_r {
                    *ctx.drag_state = DragState::CalloutLabel {
                        actor,
                        start_label_at: label_at,
                        start_scene: scene_start,
                    };
                    return GestureResult::Claimed;
                }

                GestureResult::Ignored
            },

            Gesture::DragMove { pos, .. } => {
                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);

                match ctx.drag_state.clone() {
                    DragState::CalloutLabel { actor, start_label_at, start_scene } => {
                        let dx = (scene.x - start_scene.x) as f32;
                        let dy = (scene.y - start_scene.y) as f32;
                        let new_label_at = [start_label_at[0] + dx, start_label_at[1] + dy];
                        ctx.commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor,
                                property: "label_at".into(),
                                value: PropertyValue::Vec2(new_label_at),
                                create_keyframe: ctx.keyframe_mode,
                            })
                            .into(),
                        );
                        GestureResult::Claimed
                    },
                    DragState::CalloutTip { actor, is_targeted, start_value, start_scene } => {
                        let dx = (scene.x - start_scene.x) as f32;
                        let dy = (scene.y - start_scene.y) as f32;
                        let new_value = [start_value[0] + dx, start_value[1] + dy];
                        let property = if is_targeted { "to_offset" } else { "to" };
                        ctx.commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor,
                                property: property.into(),
                                value: PropertyValue::Vec2(new_value),
                                create_keyframe: ctx.keyframe_mode,
                            })
                            .into(),
                        );
                        GestureResult::Claimed
                    },
                    _ => GestureResult::Ignored,
                }
            },

            Gesture::DragEnd { .. } => {
                match &*ctx.drag_state {
                    DragState::CalloutLabel { .. } | DragState::CalloutTip { .. } => {},
                    _ => return GestureResult::Ignored,
                }
                ctx.commands.push_back(ShellAction::Drag(DragEvent::DragEnded));
                *ctx.drag_state = DragState::None;
                GestureResult::Claimed
            },

            _ => GestureResult::Ignored,
        }
    }
}
