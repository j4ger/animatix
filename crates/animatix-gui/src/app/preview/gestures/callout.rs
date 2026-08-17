//! Gesture handler for Callout actor handle interactions.
//!
//! Handles per selected Callout:
//! - **Tip handle** (diamond, at scene `to`): dragging updates `to` (manual) or `to_offset`
//!   (targeted).
//! - **Label handle** (circle, at `to + label_at`): dragging updates `label_at`.
//! - **Place handles** (4 side circles around target bounds, targeted only): click sets `place`.
//! - **Standoff handle** (circle at `from`, targeted only): dragging updates `standoff` scalar.
//! - **Shift+drag** on targeted callout: converts to manual mode (detach).

use animatix::timeline::animation_track::CalloutPlace;
use animatix::timeline::callout_geometry::derive_callout_geometry;
use animatix::timeline::{ActorKindId, TrackAccessor};

use crate::app::commands::{
    Command, DocumentCommand, DragEvent, PropertyEdit, PropertyValue, ShellAction,
};
use crate::app::design_tokens::spatial::preview::HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS;
use crate::app::preview::DragState;
use crate::app::preview::drag_utils;
use crate::app::preview::gesture::{Gesture, GestureHandler, GestureResult};

pub(crate) struct CalloutGesture;

/// The four `CalloutPlace` variants in the same order as place-handle arrays.
const PLACE_ORDER: [CalloutPlace; 4] = [
    CalloutPlace::Top,
    CalloutPlace::Bottom,
    CalloutPlace::Left,
    CalloutPlace::Right,
];

impl GestureHandler for CalloutGesture {
    fn handle(
        &mut self,
        gesture: &Gesture,
        ctx: &mut crate::app::preview::context::PreviewContext<'_>,
        preview_rect: egui::Rect,
    ) -> GestureResult {
        match gesture {
            Gesture::DragStart { pos, modifiers, .. } => {
                let actor = match ctx.selected_actors.iter().next().cloned() {
                    Some(a) => a,
                    None => return GestureResult::Ignored,
                };

                let timeline = match ctx.timeline {
                    Some(t) => t,
                    None => return GestureResult::Ignored,
                };

                let track = match timeline.get_track(&actor) {
                    Some(t) => t,
                    None => return GestureResult::Ignored,
                };

                if track.kind != ActorKindId::Callout || track.locked {
                    return GestureResult::Ignored;
                }

                let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                let geo =
                    derive_callout_geometry(track, time_ms, Some(timeline), ctx.scene_dimensions);

                let zoom = ctx.preview.viewport.preview_zoom;
                let pan = ctx.preview.viewport.preview_pan;
                let desired = preview_rect.size();

                let tx = crate::app::preview::PreviewTransform::new(
                    ctx.scene_dimensions,
                    preview_rect,
                    zoom,
                    pan,
                );
                let tip_screen =
                    tx.scene_to_screen(kurbo::Point::new(geo.to[0] as f64, geo.to[1] as f64));
                let label_world =
                    kurbo::Point::new(geo.label_point[0] as f64, geo.label_point[1] as f64);
                let label_screen = tx.scene_to_screen(label_world);

                let hit_r = PREVIEW_HANDLE_HIT_RADIUS;
                let scene_start = ctx.preview_screen_to_scene(preview_rect, *pos);
                let to_offset = track.geometry.callout_to_offset.get(time_ms, [0.0, 0.0]);
                let label_at = track.geometry.label_at.get(time_ms, [0.0, 50.0]);

                // Priority 1: tip handle
                if pos.distance(tip_screen) <= hit_r {
                    ctx.selection.clear_tapped_place();
                    // Shift+drag on targeted callout: initiate detach
                    if geo.is_targeted && modifiers.shift {
                        *ctx.drag_state = DragState::CalloutDetach {
                            actor,
                            from: geo.from,
                            to: geo.to,
                            label_at,
                        };
                        return GestureResult::Claimed;
                    }
                    let start_value = if geo.is_targeted { to_offset } else { geo.to };
                    *ctx.drag_state = DragState::CalloutTip {
                        actor,
                        is_targeted: geo.is_targeted,
                        start_value,
                        start_scene: scene_start,
                    };
                    return GestureResult::Claimed;
                }

                // Priority 2: label handle
                if pos.distance(label_screen) <= hit_r {
                    ctx.selection.clear_tapped_place();
                    *ctx.drag_state = DragState::CalloutLabel {
                        actor,
                        start_label_at: label_at,
                        start_scene: scene_start,
                    };
                    return GestureResult::Claimed;
                }

                // Targeted-only handles
                if geo.is_targeted {
                    // Priority 3: standoff handle (at `from`)
                    let from_screen = crate::app::preview::scene_to_screen(
                        kurbo::Point::new(geo.from[0] as f64, geo.from[1] as f64),
                        preview_rect,
                        ctx.scene_dimensions,
                        desired,
                        zoom,
                        pan,
                    );
                    if pos.distance(from_screen) <= hit_r {
                        ctx.selection.clear_tapped_place();
                        *ctx.drag_state = DragState::CalloutStandoff {
                            actor,
                            tip_scene: geo.to,
                            start_standoff: geo.standoff,
                            start_scene: scene_start,
                        };
                        return GestureResult::Claimed;
                    }

                    // Priority 4: place handles (click only — claim on tap too)
                    let place_screens = crate::app::preview::callout_place_handle_screens(
                        &geo,
                        preview_rect,
                        ctx.scene_dimensions,
                        desired,
                        zoom,
                        pan,
                    );
                    for (i, screen) in place_screens.iter().enumerate() {
                        if pos.distance(*screen) <= hit_r {
                            // Clicking a place handle: emit PropertyEdit immediately and don't
                            // enter drag.
                            let place = PLACE_ORDER[i];
                            ctx.selection.tapped_place = Some(place);
                            ctx.selection.tapped_place_actor = Some(actor.clone());
                            let place_str = place.as_str();
                            ctx.commands.push_back(
                                DocumentCommand::PropertyEdit(PropertyEdit {
                                    time_s: None,
                                    actor,
                                    property: "place".into(),
                                    value: PropertyValue::String(place_str.to_string()),
                                    create_keyframe: ctx.keyframe_mode,
                                })
                                .into(),
                            );
                            return GestureResult::Claimed;
                        }
                    }
                }

                GestureResult::Ignored
            },

            Gesture::DragMove { pos, modifiers, .. } => {
                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);
                let snap_enabled = ctx.preview.snap.snap_enabled && !modifiers.alt;
                let snap_threshold = ctx.preview.snap.snap_threshold;
                let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;

                match ctx.drag_state.clone() {
                    DragState::CalloutLabel {
                        actor,
                        start_label_at,
                        start_scene,
                    } => {
                        let dx = (scene.x - start_scene.x) as f32;
                        let dy = (scene.y - start_scene.y) as f32;
                        let mut new_label_at = [start_label_at[0] + dx, start_label_at[1] + dy];

                        // Snap the absolute label point to guides/edges, then
                        // convert the snapped point back to a label offset.
                        if snap_enabled {
                            if let (Some(timeline), Some(track)) =
                                (ctx.timeline, ctx.timeline.and_then(|t| t.get_track(&actor)))
                            {
                                let geo = derive_callout_geometry(
                                    track,
                                    time_ms,
                                    Some(timeline),
                                    ctx.scene_dimensions,
                                );
                                let snapped = drag_utils::resolve_point_snap(
                                    geo.to[0] + new_label_at[0],
                                    geo.to[1] + new_label_at[1],
                                    snap_threshold,
                                    ctx,
                                );
                                new_label_at = [snapped.nx - geo.to[0], snapped.ny - geo.to[1]];
                            }
                        }

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
                    DragState::CalloutTip {
                        actor,
                        is_targeted,
                        start_value,
                        start_scene,
                    } => {
                        let dx = (scene.x - start_scene.x) as f32;
                        let dy = (scene.y - start_scene.y) as f32;
                        let mut new_value = [start_value[0] + dx, start_value[1] + dy];

                        // For targeted callouts the authored value is an offset,
                        // so snap the derived world-space tip and convert back.
                        if snap_enabled {
                            if let (Some(timeline), Some(track)) =
                                (ctx.timeline, ctx.timeline.and_then(|t| t.get_track(&actor)))
                            {
                                let geo = derive_callout_geometry(
                                    track,
                                    time_ms,
                                    Some(timeline),
                                    ctx.scene_dimensions,
                                );
                                if geo.is_targeted {
                                    let current_offset =
                                        track.geometry.callout_to_offset.get(time_ms, [0.0, 0.0]);
                                    let attach = [
                                        geo.to[0] - current_offset[0],
                                        geo.to[1] - current_offset[1],
                                    ];
                                    let snapped = drag_utils::resolve_point_snap(
                                        attach[0] + new_value[0],
                                        attach[1] + new_value[1],
                                        snap_threshold,
                                        ctx,
                                    );
                                    new_value = [snapped.nx - attach[0], snapped.ny - attach[1]];
                                } else {
                                    let snapped = drag_utils::resolve_point_snap(
                                        new_value[0],
                                        new_value[1],
                                        snap_threshold,
                                        ctx,
                                    );
                                    new_value = [snapped.nx, snapped.ny];
                                }
                            }
                        }

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
                    DragState::CalloutStandoff {
                        actor,
                        tip_scene,
                        start_standoff,
                        start_scene,
                    } => {
                        // Compute new standoff: distance from tip to current pointer in scene
                        // space. We project the drag delta onto the
                        // direction vector, then add to start_standoff.
                        let dx = (scene.x - start_scene.x) as f32;
                        let dy = (scene.y - start_scene.y) as f32;
                        // Direction of drag away from tip: normalise start-scene - tip_scene.
                        let dir_x = start_scene.x as f32 - tip_scene[0];
                        let dir_y = start_scene.y as f32 - tip_scene[1];
                        let dir_len = (dir_x * dir_x + dir_y * dir_y).sqrt().max(1.0);
                        let delta = (dx * dir_x + dy * dir_y) / dir_len;
                        let mut new_standoff = (start_standoff + delta).max(0.0);

                        // Snap the standoff handle's world-space position to
                        // guides/edges, then project the snapped point back
                        // onto the drag direction to preserve the scalar edit.
                        if snap_enabled {
                            let unit_x = dir_x / dir_len;
                            let unit_y = dir_y / dir_len;
                            let mut handle_point = [
                                tip_scene[0] + unit_x * new_standoff,
                                tip_scene[1] + unit_y * new_standoff,
                            ];
                            let snapped = drag_utils::resolve_point_snap(
                                handle_point[0],
                                handle_point[1],
                                snap_threshold,
                                ctx,
                            );
                            handle_point = [snapped.nx, snapped.ny];
                            new_standoff = ((handle_point[0] - tip_scene[0]) * unit_x
                                + (handle_point[1] - tip_scene[1]) * unit_y)
                                .max(0.0);
                        }

                        ctx.commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor,
                                property: "standoff".into(),
                                value: PropertyValue::F32(new_standoff),
                                create_keyframe: ctx.keyframe_mode,
                            })
                            .into(),
                        );
                        GestureResult::Claimed
                    },
                    DragState::CalloutDetach { .. } => {
                        // No live preview during detach drag; finalise on DragEnd.
                        GestureResult::Claimed
                    },
                    _ => GestureResult::Ignored,
                }
            },

            Gesture::DragEnd { .. } => {
                let state = ctx.drag_state.clone();
                match &state {
                    DragState::CalloutLabel { .. }
                    | DragState::CalloutTip { .. }
                    | DragState::CalloutStandoff { .. } => {},
                    DragState::CalloutDetach {
                        actor,
                        from,
                        to,
                        label_at,
                    } => {
                        // Emit atomic detach command: bakes from/to/label_at and removes target.
                        ctx.commands.push_back(ShellAction::Command(Command::DetachCallout {
                            actor: actor.clone(),
                            from: *from,
                            to: *to,
                            label_at: *label_at,
                        }));
                        ctx.commands.push_back(ShellAction::Drag(DragEvent::DragEnded));
                        *ctx.drag_state = DragState::None;
                        return GestureResult::Claimed;
                    },
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
