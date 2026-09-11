//! Gesture handler for rotation — ToolMode::Rotate and Select rotation handle.
//!
//! Extracted from the legacy `drag_handler.rs` Rotate match arms.

use crate::app::commands::{DocumentCommand, PropertyEdit, PropertyValue};
use crate::app::design_tokens::spatial::preview::HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS;
use crate::app::preview::gesture::{Gesture, GestureHandler, GestureResult};
use crate::app::preview::{self, DragState, ToolMode, drag_utils};

pub(crate) struct RotateGesture;

impl GestureHandler for RotateGesture {
    fn handle(
        &mut self,
        gesture: &Gesture,
        ctx: &mut crate::app::preview::context::PreviewContext<'_>,
        preview_rect: egui::Rect,
    ) -> GestureResult {
        match gesture {
            Gesture::DragStart { pos, .. } => {
                // Only handle Rotate tool mode or Select tool mode (rotation handle only)
                match *ctx.tool_mode {
                    ToolMode::Rotate | ToolMode::Select => {},
                    _ => return GestureResult::Ignored,
                }

                let hit_radius = PREVIEW_HANDLE_HIT_RADIUS;

                // Multi-selection: rotate the whole group around its union centre.
                if ctx.selected_actors.len() > 1 && *ctx.tool_mode == ToolMode::Rotate {
                    let Some(union) = ctx.selected_union_rect() else {
                        return GestureResult::Ignored;
                    };
                    let scene = ctx.preview_screen_to_scene(preview_rect, *pos);
                    let center = union.center();
                    if !union.contains(kurbo::Point::new(scene.x, scene.y)) {
                        return GestureResult::Ignored;
                    }
                    let actors = ctx.capture_group_actors();
                    if actors.is_empty() {
                        return GestureResult::Ignored;
                    }
                    let angle = ((scene.y - center.y) as f32).atan2((scene.x - center.x) as f32);
                    *ctx.drag_state = DragState::GroupRotate {
                        actors,
                        center: [center.x as f32, center.y as f32],
                        start_angle: angle,
                    };
                    return GestureResult::Claimed;
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
                let props = match ctx.get_actor_props(&actor) {
                    Some(p) => p,
                    None => return GestureResult::Ignored,
                };

                // Convert screen pos to scene space
                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);

                // --- Hit test rotation handle first ---
                let rotation_handle_world = preview::rotation_handle_world(&props);
                let rotation_handle_screen =
                    ctx.preview_scene_to_screen(preview_rect, rotation_handle_world);
                let near_rotation_handle =
                    preview::hit_test_rotation_handle(*pos, rotation_handle_screen, hit_radius);

                // --- Check cursor within actor body (only for Rotate tool mode) ---
                let near_actor_body = match *ctx.tool_mode {
                    ToolMode::Rotate => {
                        drag_utils::is_over_actor_hit_region(scene, &actor, ctx.hit_regions)
                            || drag_utils::hit_test_actor_body(scene, Some(&props))
                    },
                    ToolMode::Select => false,
                    _ => false,
                };

                if !near_rotation_handle && !near_actor_body {
                    return GestureResult::Ignored;
                }

                // Compute pivot: opposite edge (bottom-center) for handle hit, actor center for
                // body hit
                let pivot = if near_rotation_handle {
                    // Opposite edge = bottom center of the actor's bounding box
                    let hh = props.size[1] / 2.0;
                    let cos = props.rotation.cos();
                    let sin = props.rotation.sin();
                    let local_bottom = [0.0_f32, hh];
                    let rotated_x = local_bottom[0] * cos - local_bottom[1] * sin;
                    let rotated_y = local_bottom[0] * sin + local_bottom[1] * cos;
                    [props.position[0] + rotated_x, props.position[1] + rotated_y]
                } else {
                    // Actor centre
                    props.position
                };

                let angle =
                    ((scene.y - pivot[1] as f64) as f32).atan2((scene.x - pivot[0] as f64) as f32);

                *ctx.drag_state = DragState::Rotate {
                    actor,
                    start_angle: angle,
                    start_rotation: props.rotation,
                    pivot,
                };

                GestureResult::Claimed
            },
            Gesture::DragMove { pos, modifiers, .. } => {
                // Group rotate: rotate every member around the union centre.
                let group = match &*ctx.drag_state {
                    DragState::GroupRotate {
                        actors,
                        center,
                        start_angle,
                    } => Some((actors.clone(), *center, *start_angle)),
                    _ => None,
                };
                if let Some((actors, center, start_angle)) = group {
                    let scene = ctx.preview_screen_to_scene(preview_rect, *pos);
                    let angle = ((scene.y - center[1] as f64) as f32)
                        .atan2((scene.x - center[0] as f64) as f32);
                    let mut delta = angle - start_angle;
                    while delta > std::f32::consts::PI {
                        delta -= 2.0 * std::f32::consts::PI;
                    }
                    while delta < -std::f32::consts::PI {
                        delta += 2.0 * std::f32::consts::PI;
                    }
                    let snap = ctx.rotation_snap_degrees.to_radians();
                    let (sin_d, cos_d) = (delta.sin(), delta.cos());
                    for a in &actors {
                        let mut new_rot = a.rotation + delta;
                        if modifiers.shift {
                            new_rot = (new_rot / snap).round() * snap;
                        }
                        let dx = a.position[0] - center[0];
                        let dy = a.position[1] - center[1];
                        let nx = center[0] + dx * cos_d - dy * sin_d;
                        let ny = center[1] + dx * sin_d + dy * cos_d;
                        ctx.commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor: a.label.clone(),
                                property: "rotation".into(),
                                value: PropertyValue::F32(new_rot),
                                create_keyframe: ctx.keyframe_mode,
                            })
                            .into(),
                        );
                        drag_utils::emit_position_edit(a.label.clone(), nx, ny, ctx);
                    }
                    return GestureResult::Claimed;
                }

                // Only handle if we are already in Rotate state
                let (actor, start_angle, start_rotation, pivot) = match &*ctx.drag_state {
                    DragState::Rotate {
                        actor,
                        start_angle,
                        start_rotation,
                        pivot,
                    } => (actor.clone(), *start_angle, *start_rotation, *pivot),
                    _ => return GestureResult::Ignored,
                };

                let scene = ctx.preview_screen_to_scene(preview_rect, *pos);
                let angle =
                    ((scene.y - pivot[1] as f64) as f32).atan2((scene.x - pivot[0] as f64) as f32);
                let mut delta = angle - start_angle;
                // Normalize delta to [-PI, PI]
                while delta > std::f32::consts::PI {
                    delta -= 2.0 * std::f32::consts::PI;
                }
                while delta < -std::f32::consts::PI {
                    delta += 2.0 * std::f32::consts::PI;
                }
                let mut new_rot = start_rotation + delta;
                if modifiers.shift {
                    new_rot = (new_rot / ctx.rotation_snap_degrees.to_radians()).round()
                        * ctx.rotation_snap_degrees.to_radians();
                }
                ctx.commands.push_back(
                    DocumentCommand::PropertyEdit(PropertyEdit {
                        time_s: None,
                        actor,
                        property: "rotation".into(),
                        value: PropertyValue::F32(new_rot),
                        create_keyframe: ctx.keyframe_mode,
                    })
                    .into(),
                );

                GestureResult::Claimed
            },
            Gesture::DragEnd { .. } => {
                let old_drag_state = ctx.drag_state.clone();

                // Only handle if we were in Rotate state
                match &old_drag_state {
                    DragState::Rotate { .. } | DragState::GroupRotate { .. } => {},
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
