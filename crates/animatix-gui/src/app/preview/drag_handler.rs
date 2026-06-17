//! Preview canvas drag interaction handler.

use egui::Pos2;

use crate::app::commands::{
    Command, DocumentCommand, DragEvent, PropertyEdit, PropertyValue, ShellAction,
};
use crate::app::design_tokens::spatial::preview::{
    HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS, MIN_ACTOR_SIZE as PREVIEW_MIN_ACTOR_SIZE,
    MIN_SCALE as PREVIEW_MIN_SCALE, ROTATION_OFFSET as PREVIEW_ROTATION_OFFSET,
};
use crate::app::preview::context::PreviewContext;
use crate::app::preview::{self, DragState, drag_utils};
use animatix::timeline::TrackAccessor;

pub(crate) fn handle_preview_drag(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    response: &egui::Response,
) -> bool {
    let is_dragging = !matches!(ctx.drag_state, DragState::None);
    let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());

    if ui.input(|i| i.pointer.middle_down()) {
        return false;
    }

    let drag_started =
        response.drag_started() || (!is_dragging && ui.input(|i| i.pointer.primary_pressed()));
    let hit_radius = PREVIEW_HANDLE_HIT_RADIUS;

    if drag_started {
        if let (Some(actor), Some(mouse)) =
            (ctx.selected_actors.iter().next().cloned(), raw_pointer_pos)
        {
            let is_locked = ctx
                .timeline
                .and_then(|t| t.get_track(&actor))
                .map(|tr| tr.locked)
                .unwrap_or(false);
            if is_locked {
                return false;
            }
            let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
            let props = ctx.get_actor_props(&actor);

            if let Some(ref p) = props {
                let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;

                match *ctx.tool_mode {
                    preview::ToolMode::Move => {},
                    preview::ToolMode::Vertex => {},
                    preview::ToolMode::Scale => {
                        let handle_world = preview::world_handle_positions(p);
                        let handle_screen: [Pos2; 8] = std::array::from_fn(|i| {
                            ctx.preview_scene_to_screen(preview_rect, handle_world[i])
                        });
                        // Find nearest handle within hit radius
                        let nearest =
                            drag_utils::find_nearest_handle(mouse, &handle_screen, hit_radius);
                        if let Some(idx) = nearest {
                            let anchor_local = if p.pivot_offset != [0.0, 0.0] {
                                p.pivot_offset
                            } else {
                                preview::handle_anchor_local(idx, p.size)
                            };
                            let (resize_mode, start_scale) = ctx
                                .timeline
                                .and_then(|t| t.get_track(&actor))
                                .map(|tr| {
                                    let mode = if let Some(primitive) =
                                        animatix::timeline::actor_kind_meta(tr.kind).and_then(|m| {
                                            animatix::primitives::find_primitive(m.type_name)
                                        }) {
                                        match primitive.resize_mode() {
                                            animatix::timeline::ResizeMode::Scale => {
                                                preview::ResizeMode::Scale
                                            },
                                            _ => preview::ResizeMode::Size,
                                        }
                                    } else {
                                        preview::ResizeMode::Size
                                    };
                                    (mode, tr.scale.get(time_ms, 1.0))
                                })
                                .unwrap_or((preview::ResizeMode::Size, 1.0));
                            *ctx.drag_state = DragState::Scale {
                                actor,
                                handle: idx,
                                start_scene: scene,
                                start_position: p.position,
                                start_size: p.size,
                                start_rotation: p.rotation,
                                anchor_local,
                                constrain_axis: preview::handle_constrains_axis(idx),
                                uniform_ratio: ui.input(|i| i.modifiers.shift),
                                resize_mode,
                                start_scale,
                            };
                            return true;
                        }
                    },
                    preview::ToolMode::Rotate => {
                        // Only start rotation if cursor is near the rotation handle or actor body
                        let pivot = preview::pivot_world(p);
                        let pivot_screen = ctx.preview_scene_to_screen(
                            preview_rect,
                            kurbo::Point::new(pivot[0] as f64, pivot[1] as f64),
                        );
                        let rotation_handle_offset = PREVIEW_ROTATION_OFFSET;
                        let rotation_handle_screen =
                            Pos2::new(pivot_screen.x, pivot_screen.y - rotation_handle_offset);
                        let near_rotation_handle = drag_utils::is_near_rotation_handle(
                            mouse,
                            rotation_handle_screen,
                            hit_radius,
                        );
                        // Check if cursor is within actor bounds
                        let near_actor_body =
                            drag_utils::is_over_actor_hit_region(scene, &actor, ctx.hit_regions);

                        if near_rotation_handle || near_actor_body {
                            let angle = ((scene.y - pivot[1] as f64) as f32)
                                .atan2((scene.x - pivot[0] as f64) as f32);
                            *ctx.drag_state = DragState::Rotate {
                                actor,
                                start_angle: angle,
                                start_rotation: p.rotation,
                                pivot,
                            };
                            return true;
                        }
                    },
                    preview::ToolMode::Pivot => {
                        let pivot_world_pt = preview::pivot_world(p);
                        let pivot_screen = ctx.preview_scene_to_screen(
                            preview_rect,
                            kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64),
                        );
                        if preview::hit_test_pivot(mouse, pivot_screen, hit_radius) {
                            *ctx.drag_state = DragState::MovePivot {
                                actor,
                                start_offset: p.pivot_offset,
                                start_scene: scene,
                            };
                            return true;
                        }
                    },
                    preview::ToolMode::Select => {
                        let handle_world = preview::world_handle_positions(p);
                        let handle_screen: [Pos2; 8] = std::array::from_fn(|i| {
                            ctx.preview_scene_to_screen(preview_rect, handle_world[i])
                        });
                        if let Some(idx) =
                            preview::hit_test_handle(mouse, &handle_screen, hit_radius)
                        {
                            let anchor_local = if p.pivot_offset != [0.0, 0.0] {
                                p.pivot_offset
                            } else {
                                preview::handle_anchor_local(idx, p.size)
                            };
                            let (resize_mode, start_scale) = ctx
                                .timeline
                                .and_then(|t| t.get_track(&actor))
                                .map(|tr| {
                                    let mode = if let Some(primitive) =
                                        animatix::timeline::actor_kind_meta(tr.kind).and_then(|m| {
                                            animatix::primitives::find_primitive(m.type_name)
                                        }) {
                                        match primitive.resize_mode() {
                                            animatix::timeline::ResizeMode::Scale => {
                                                preview::ResizeMode::Scale
                                            },
                                            _ => preview::ResizeMode::Size,
                                        }
                                    } else {
                                        preview::ResizeMode::Size
                                    };
                                    (mode, tr.scale.get(time_ms, 1.0))
                                })
                                .unwrap_or((preview::ResizeMode::Size, 1.0));
                            *ctx.drag_state = DragState::Scale {
                                actor: actor.clone(),
                                handle: idx,
                                start_scene: scene,
                                start_position: p.position,
                                start_size: p.size,
                                start_rotation: p.rotation,
                                anchor_local,
                                constrain_axis: preview::handle_constrains_axis(idx),
                                uniform_ratio: ui.input(|i| i.modifiers.shift),
                                resize_mode,
                                start_scale,
                            };
                            return true;
                        }

                        let rot_world = preview::rotation_handle_world(p);
                        let rot_screen = ctx.preview_scene_to_screen(preview_rect, rot_world);
                        if preview::hit_test_rotation_handle(mouse, rot_screen, hit_radius) {
                            let pivot = preview::pivot_world(p);
                            let angle = ((scene.y - pivot[1] as f64) as f32)
                                .atan2((scene.x - pivot[0] as f64) as f32);
                            *ctx.drag_state = DragState::Rotate {
                                actor: actor.clone(),
                                start_angle: angle,
                                start_rotation: p.rotation,
                                pivot,
                            };
                            return true;
                        }

                        let pivot_world_pt = preview::pivot_world(p);
                        let pivot_screen = ctx.preview_scene_to_screen(
                            preview_rect,
                            kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64),
                        );
                        if preview::hit_test_pivot(mouse, pivot_screen, hit_radius) {
                            *ctx.drag_state = DragState::MovePivot {
                                actor: actor.clone(),
                                start_offset: p.pivot_offset,
                                start_scene: scene,
                            };
                            return true;
                        }
                    },
                }
            }

            let hit_body = drag_utils::hit_test_actor_body(scene, props.as_ref());

            if hit_body
                || ctx
                    .hit_regions
                    .iter()
                    .rev()
                    .any(|(label, bounds)| label == &actor && bounds.contains(scene))
            {
                if ctx.is_layout_managed(&actor) {
                    let shift = ui.input(|i| i.modifiers.shift);
                    if shift {
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
                        return true;
                    }
                    if let Some((container, layout_type, source_index)) =
                        ctx.find_layout_container(&actor)
                    {
                        // Only activate reorder on actual drag movement (not just click)
                        // This allows double-click to reach the text editing handler
                        if response.drag_started() {
                            *ctx.drag_state = DragState::Reorder {
                                actor,
                                container,
                                source_index,
                                target_index: source_index,
                                layout_type,
                            };
                            return true;
                        }
                        // Click without drag: fall through to selection/double-click handlers
                        // Don't return true here - let selection handler process the click
                    }
                }

                let alt = ui.input(|i| i.modifiers.alt);
                if alt {
                    ctx.commands.push_back(ShellAction::Command(Command::DuplicateActor(actor)));
                    return true;
                }

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
            }
        } else if let Some(mouse) = raw_pointer_pos {
            ctx.selection.marquee_start = Some(mouse);
            ctx.selection.marquee_current = Some(mouse);
        }
    }

    if is_dragging {
        if let Some(mouse) = raw_pointer_pos {
            let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
            let shift = ui.input(|i| i.modifiers.shift);

            match ctx.drag_state.clone() {
                DragState::Move {
                    primary: _,
                    actors,
                    start_scene,
                } => {
                    let raw_dx = (scene.x - start_scene.x) as f32;
                    let raw_dy = (scene.y - start_scene.y) as f32;
                    let (dx, dy) = if shift {
                        if raw_dx.abs() > raw_dy.abs() {
                            (raw_dx, 0.0)
                        } else {
                            (0.0, raw_dy)
                        }
                    } else {
                        (raw_dx, raw_dy)
                    };

                    let snap_enabled =
                        ctx.preview.snap.snap_enabled && !ui.input(|i| i.modifiers.alt);
                    let threshold = ctx.preview.snap.snap_threshold;

                    let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                    for (actor, start_position) in actors {
                        let mut nx = start_position[0] + dx;
                        let mut ny = start_position[1] + dy;

                        if ctx.preview.overlay.show_grid {
                            let grid = ctx.preview.overlay.grid_size;
                            nx = (nx / grid).round() * grid;
                            ny = (ny / grid).round() * grid;
                        }

                        if snap_enabled {
                            let result =
                                drag_utils::resolve_snap(&actor, nx, ny, threshold, time_ms, ctx);
                            nx = result.nx;
                            ny = result.ny;
                        }

                        drag_utils::emit_position_edit(actor.clone(), nx, ny, ctx);
                    }
                },
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
                } => {
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
                    let uniform = shift || uniform_ratio || force_uniform;
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
                    let new_pos_x = anchor_world_x - new_anchor_local[0] * cos_rot
                        + new_anchor_local[1] * sin_rot;
                    let new_pos_y = anchor_world_y
                        - new_anchor_local[0] * sin_rot
                        - new_anchor_local[1] * cos_rot;

                    if resize_mode == preview::ResizeMode::Scale {
                        let ratio = new_w / start_size[0].max(1.0);
                        ctx.commands.push_back(
                            DocumentCommand::PropertyEdit(PropertyEdit {
                                time_s: None,
                                actor: actor.clone(),
                                property: "scale".into(),
                                value: PropertyValue::Float(
                                    (start_scale * ratio).max(PREVIEW_MIN_SCALE),
                                ),
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
                },
                DragState::Rotate {
                    actor,
                    start_angle,
                    start_rotation,
                    pivot,
                } => {
                    let angle = ((scene.y - pivot[1] as f64) as f32)
                        .atan2((scene.x - pivot[0] as f64) as f32);
                    let mut delta = angle - start_angle;
                    while delta > std::f32::consts::PI {
                        delta -= 2.0 * std::f32::consts::PI;
                    }
                    while delta < -std::f32::consts::PI {
                        delta += 2.0 * std::f32::consts::PI;
                    }
                    let mut new_rot = start_rotation + delta;
                    if shift {
                        new_rot = (new_rot / ctx.rotation_snap_degrees.to_radians()).round()
                            * ctx.rotation_snap_degrees.to_radians();
                    }
                    ctx.commands.push_back(
                        DocumentCommand::PropertyEdit(PropertyEdit {
                            time_s: None,
                            actor,
                            property: "rotation".into(),
                            value: PropertyValue::Float(new_rot),
                            create_keyframe: ctx.keyframe_mode,
                        })
                        .into(),
                    );
                },
                DragState::Reorder {
                    actor,
                    container,
                    source_index: _,
                    target_index: _,
                    layout_type,
                } => {
                    let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                    if let Some(timeline) = ctx.timeline {
                        let order = timeline.get_child_order(&container, time_ms);
                        let siblings: Vec<String> =
                            order.into_iter().filter(|l| l != &actor).collect();
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
                                    .unwrap_or(
                                        if layout_type == animatix::timeline::LayoutType::Row {
                                            scene.x as f32
                                        } else {
                                            scene.y as f32
                                        },
                                    )
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
                },

                DragState::MovePivot {
                    actor,
                    start_offset,
                    start_scene,
                } => {
                    let dx = (scene.x - start_scene.x) as f32;
                    let dy = (scene.y - start_scene.y) as f32;
                    if let Some(p) = ctx.get_actor_props(&actor) {
                        let cos = (-p.rotation).cos();
                        let sin = (-p.rotation).sin();
                        let local_dx = dx * cos - dy * sin;
                        let local_dy = dx * sin + dy * cos;
                        ctx.pivot_offsets.insert(
                            actor,
                            [start_offset[0] + local_dx, start_offset[1] + local_dy],
                        );
                    }
                },
                DragState::EditVertices { .. } => {},
                DragState::None => {},
            }
        }
    }

    let pointer_released = ui.input(|i| i.pointer.any_released());
    if is_dragging
        && (response.drag_stopped() || pointer_released || !ui.input(|i| i.pointer.any_down()))
    {
        let old_drag_state = ctx.drag_state.clone();
        drag_utils::finalize_drag_keyframes(&old_drag_state, ctx);

        if let DragState::Reorder {
            actor,
            container,
            source_index,
            target_index,
            ..
        } = ctx.drag_state.clone()
        {
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
        }
        ctx.commands.push_back(ShellAction::Drag(DragEvent::DragEnded));
    }
    false
}
