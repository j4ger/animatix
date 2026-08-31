//! Gesture handler for scaling — ToolMode::Scale and Select scale handles.
//!
//! Extracted from the legacy `drag_handler.rs` Scale match arms.

use animatix::timeline::TrackAccessor;
use egui::Pos2;

use crate::app::commands::{DocumentCommand, PropertyEdit, PropertyValue};
use crate::app::design_tokens::spatial::preview::{
    HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS, MIN_ACTOR_SIZE as PREVIEW_MIN_ACTOR_SIZE,
    MIN_SCALE as PREVIEW_MIN_SCALE,
};
use crate::app::preview::gesture::{Gesture, GestureHandler, GestureResult};
use crate::app::preview::{self, DragState, ToolMode, drag_utils};

pub(crate) struct ScaleGesture;

impl GestureHandler for ScaleGesture {
    fn handle(
        &mut self,
        gesture: &Gesture,
        ctx: &mut crate::app::preview::context::PreviewContext<'_>,
        preview_rect: egui::Rect,
    ) -> GestureResult {
        match gesture {
            Gesture::DragStart { pos, modifiers, .. } => {
                // Only handle Scale tool mode or Select tool mode (handle hit)
                match *ctx.tool_mode {
                    ToolMode::Scale | ToolMode::Select => {},
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

                let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);

                // Hit test 8 handles
                let handle_world = preview::world_handle_positions(&props);
                let handle_screen: [Pos2; 8] = std::array::from_fn(|i| {
                    ctx.preview_scene_to_screen(preview_rect, handle_world[i])
                });
                let nearest = drag_utils::find_nearest_handle(*pos, &handle_screen, hit_radius);

                let idx = match nearest {
                    Some(i) => i,
                    None => return GestureResult::Ignored,
                };

                let anchor_local = if props.pivot_offset != [0.0, 0.0] {
                    props.pivot_offset
                } else {
                    preview::handle_anchor_local(idx, props.size)
                };

                let (resize_mode, start_scale) = ctx
                    .timeline
                    .and_then(|t| {
                        t.get_track(&actor).map(|tr| {
                            // Prefer the live timeline registry (covers
                            // extension primitives, whose
                            // `ActorKindId::Extension` has no static
                            // metadata); fall back to the static built-in
                            // lookup for hand-built tracks without an actor
                            // type. The snapshot Arc must stay alive while
                            // `find` borrows from it.
                            let registry = t.primitive_registry_snapshot();
                            let mode = if let Some(primitive) =
                                tr.actor_type.as_deref().and_then(|ty| registry.find(ty)).or_else(
                                    || {
                                        animatix::timeline::actor_kind_meta(tr.kind).and_then(|m| {
                                            animatix::primitives::find_primitive(m.type_name)
                                        })
                                    },
                                ) {
                                match primitive.resize_mode() {
                                    animatix::timeline::ResizeMode::Scale => {
                                        preview::ResizeMode::Scale
                                    },
                                    _ => preview::ResizeMode::Size,
                                }
                            } else {
                                preview::ResizeMode::Size
                            };
                            (mode, tr.geometry.scale.get(time_ms, 1.0))
                        })
                    })
                    .unwrap_or((preview::ResizeMode::Size, 1.0));

                *ctx.drag_state = DragState::Scale {
                    actor,
                    handle: idx,
                    start_scene: scene,
                    start_position: props.position,
                    start_size: props.size,
                    start_rotation: props.rotation,
                    anchor_local,
                    constrain_axis: preview::handle_constrains_axis(idx),
                    uniform_ratio: modifiers.shift,
                    resize_mode,
                    start_scale,
                };

                GestureResult::Claimed
            },
            Gesture::DragMove { pos, modifiers, .. } => {
                // Only handle if we are already in Scale state
                let (
                    actor,
                    handle,
                    start_scene,
                    start_position,
                    start_size,
                    start_rotation,
                    anchor_local,
                    constrain_axis,
                    uniform_ratio,
                    resize_mode,
                    start_scale,
                ) = match &*ctx.drag_state {
                    DragState::Scale {
                        actor,
                        handle,
                        start_scene,
                        start_position,
                        start_size,
                        start_rotation,
                        anchor_local,
                        constrain_axis,
                        uniform_ratio,
                        resize_mode,
                        start_scale,
                    } => (
                        actor.clone(),
                        *handle,
                        *start_scene,
                        *start_position,
                        *start_size,
                        *start_rotation,
                        *anchor_local,
                        *constrain_axis,
                        *uniform_ratio,
                        *resize_mode,
                        *start_scale,
                    ),
                    _ => return GestureResult::Ignored,
                };

                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);
                let dx_world = (scene.x - start_scene.x) as f32;
                let dy_world = (scene.y - start_scene.y) as f32;
                let cos = (-start_rotation).cos();
                let sin = (-start_rotation).sin();
                let dx_local = dx_world * cos - dy_world * sin;
                let dy_local = dx_world * sin + dy_world * cos;

                let sign = match handle {
                    0 => [-1.0, -1.0],
                    1 => [1.0, -1.0],
                    2 => [1.0, 1.0],
                    3 => [-1.0, 1.0],
                    4 => [0.0, -1.0],
                    5 => [1.0, 0.0],
                    6 => [0.0, 1.0],
                    7 => [-1.0, 0.0],
                    _ => [1.0, 1.0],
                };

                let mut new_w = start_size[0];
                let mut new_h = start_size[1];
                if sign[0] != 0.0 {
                    new_w = (start_size[0] + sign[0] * dx_local).max(PREVIEW_MIN_ACTOR_SIZE);
                }
                if sign[1] != 0.0 {
                    new_h = (start_size[1] + sign[1] * dy_local).max(PREVIEW_MIN_ACTOR_SIZE);
                }

                let force_uniform = resize_mode == preview::ResizeMode::Scale;
                let uniform = modifiers.shift || uniform_ratio || force_uniform;
                if uniform {
                    let scale_w = new_w / start_size[0].max(1.0);
                    let scale_h = new_h / start_size[1].max(1.0);
                    let s = if constrain_axis && !force_uniform {
                        if sign[0] == 0.0 { scale_h } else { scale_w }
                    } else {
                        scale_w.max(scale_h)
                    };
                    new_w = (start_size[0] * s).max(PREVIEW_MIN_ACTOR_SIZE);
                    new_h = (start_size[1] * s).max(PREVIEW_MIN_ACTOR_SIZE);
                }

                let cos_rot = start_rotation.cos();
                let sin_rot = start_rotation.sin();
                let old_anchor_local = [anchor_local[0], anchor_local[1]];
                let new_anchor_local = [
                    old_anchor_local[0] * new_w / start_size[0].max(1.0),
                    old_anchor_local[1] * new_h / start_size[1].max(1.0),
                ];
                let anchor_world_x = start_position[0] + old_anchor_local[0] * cos_rot
                    - old_anchor_local[1] * sin_rot;
                let anchor_world_y = start_position[1]
                    + old_anchor_local[0] * sin_rot
                    + old_anchor_local[1] * cos_rot;
                let new_pos_x =
                    anchor_world_x - new_anchor_local[0] * cos_rot + new_anchor_local[1] * sin_rot;
                let new_pos_y =
                    anchor_world_y - new_anchor_local[0] * sin_rot - new_anchor_local[1] * cos_rot;

                if resize_mode == preview::ResizeMode::Scale {
                    let ratio = new_w / start_size[0].max(1.0);
                    ctx.commands.push_back(
                        DocumentCommand::PropertyEdit(PropertyEdit {
                            time_s: None,
                            actor: actor.clone(),
                            property: "scale".into(),
                            value: PropertyValue::F32((start_scale * ratio).max(PREVIEW_MIN_SCALE)),
                            create_keyframe: ctx.keyframe_mode,
                        })
                        .into(),
                    );
                } else {
                    ctx.commands.push_back(
                        DocumentCommand::PropertyEdit(PropertyEdit {
                            time_s: None,
                            actor: actor.clone(),
                            property: "size".into(),
                            value: PropertyValue::Vec2([new_w, new_h]),
                            create_keyframe: ctx.keyframe_mode,
                        })
                        .into(),
                    );
                }

                drag_utils::emit_position_edit(actor.clone(), new_pos_x, new_pos_y, ctx);

                GestureResult::Claimed
            },
            Gesture::DragEnd { .. } => {
                let old_drag_state = ctx.drag_state.clone();

                // Only handle if we were in Scale state
                match &old_drag_state {
                    DragState::Scale { .. } => {},
                    _ => return GestureResult::Ignored,
                }

                crate::app::preview::gestures::common::finish_drag(ctx, old_drag_state);
                *ctx.drag_state = DragState::None;

                GestureResult::Claimed
            },
            _ => GestureResult::Ignored,
        }
    }
}
