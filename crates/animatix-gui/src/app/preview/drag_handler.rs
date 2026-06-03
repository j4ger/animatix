//! Preview canvas drag interaction handler.

use egui::Pos2;

use crate::app::commands::{Command, DragEvent, ShellAction, PropertyEdit, PropertyValue};
use crate::app::design_tokens::*;
use crate::app::preview::{self, DragState};
use crate::app::preview::context::PreviewContext;
use animatix::timeline::{PositionBinding, TrackAccessor};

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

        let drag_started = response.drag_started()
            || (!is_dragging && ui.input(|i| i.pointer.primary_pressed()));
        let hit_radius = PREVIEW_HANDLE_HIT_RADIUS * ui.ctx().pixels_per_point();

        if drag_started {
            if let (Some(actor), Some(mouse)) = (ctx.selected_actors.iter().next().cloned(), raw_pointer_pos) {
                let is_locked = ctx.timeline
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
                    let vertex_points = ctx.timeline
                        .and_then(|t| t.get_track(&actor))
                        .and_then(|tr| tr.points.as_ref().map(|pt| pt.evaluate(time_ms)))
                        .filter(|pts| !pts.is_empty());

                    match *ctx.tool_mode {
                        preview::ToolMode::Move => {}
                        preview::ToolMode::Vertex => {
                            if let Some(ref points) = vertex_points {
                                if let Some(vidx) = preview::hit_test_vertex(mouse, p, points, preview_rect, ctx.scene_dimensions, preview_rect.size(), hit_radius * 2.0, ctx.preview.viewport.preview_zoom, ctx.preview.viewport.preview_pan) {
                                    *ctx.drag_state = DragState::EditVertices {
                                        actor,
                                        vertex: vidx,
                                        start_points: points.clone(),
                                        start_scene: scene,
                                    };
                                    return true;
                                }
                            }
                        }
                        preview::ToolMode::Scale => {
                            let handle_world = preview::world_handle_positions(p);
                            let handle_screen: [Pos2; 8] =
                                std::array::from_fn(|i| ctx.preview_scene_to_screen(preview_rect, handle_world[i]));
                            let nearest = (0..8).min_by_key(|i| {
                                let d = mouse.distance(handle_screen[*i]);
                                (d * 1000.0) as i32
                            });
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
                                        let mode = if let Some(primitive) = animatix::timeline::actor_kind_meta(tr.kind)
                                            .and_then(|m| animatix::primitives::find_primitive(m.type_name))
                                        {
                                            match primitive.resize_mode() {
                                                animatix::timeline::ResizeMode::Scale => preview::ResizeMode::Scale,
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
                        }
                        preview::ToolMode::Rotate => {
                            let pivot = preview::pivot_world(p);
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
                        }
                        preview::ToolMode::Select => {
                            if let Some(ref points) = vertex_points {
                                if let Some(vidx) = preview::hit_test_vertex(mouse, p, points, preview_rect, ctx.scene_dimensions, preview_rect.size(), hit_radius, ctx.preview.viewport.preview_zoom, ctx.preview.viewport.preview_pan) {
                                    *ctx.drag_state = DragState::EditVertices {
                                        actor: actor.clone(),
                                        vertex: vidx,
                                        start_points: points.clone(),
                                        start_scene: scene,
                                    };
                                    return true;
                                }
                            }

                            let handle_world = preview::world_handle_positions(p);
                            let handle_screen: [Pos2; 8] =
                                std::array::from_fn(|i| ctx.preview_scene_to_screen(preview_rect, handle_world[i]));
                            if let Some(idx) = preview::hit_test_handle(mouse, &handle_screen, hit_radius) {
                                let anchor_local = if p.pivot_offset != [0.0, 0.0] {
                                    p.pivot_offset
                                } else {
                                    preview::handle_anchor_local(idx, p.size)
                                };
                                let (resize_mode, start_scale) = ctx
                                    .timeline
                                    .and_then(|t| t.get_track(&actor))
                                    .map(|tr| {
                                        let mode = if let Some(primitive) = animatix::timeline::actor_kind_meta(tr.kind)
                                            .and_then(|m| animatix::primitives::find_primitive(m.type_name))
                                        {
                                            match primitive.resize_mode() {
                                                animatix::timeline::ResizeMode::Scale => preview::ResizeMode::Scale,
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
                        }
                    }
                }

                // ── Motion path keyframe hit test ──
                if let Some(timeline) = ctx.timeline {
                    if let Some(track) = timeline.get_track(&actor) {
                        if let Some(pos_track) = &track.position {
                            for (&time_ms, (pos, _)) in &pos_track.keyframes {
                                let screen = preview::scene_to_screen(
                                    kurbo::Point::new(pos[0] as f64, pos[1] as f64),
                                    preview_rect, ctx.scene_dimensions, preview_rect.size(),
                                    ctx.preview.viewport.preview_zoom, ctx.preview.viewport.preview_pan,
                                );
                                if mouse.distance(screen) <= hit_radius * 2.0 {
                                    *ctx.drag_state = DragState::MotionPath {
                                        actor: actor.clone(),
                                        time_ms,
                                        start_position: *pos,
                                        start_scene: scene,
                                    };
                                    return true;
                                }
                            }
                        }
                    }
                }

                let hit_body = props
                    .map(|p| {
                        let local_pt = [
                            (scene.x - p.position[0] as f64) as f32,
                            (scene.y - p.position[1] as f64) as f32,
                        ];
                        let cos = (-p.rotation).cos();
                        let sin = (-p.rotation).sin();
                        let lx = local_pt[0] * cos - local_pt[1] * sin;
                        let ly = local_pt[0] * sin + local_pt[1] * cos;
                        let hw = p.size[0] / 2.0;
                        let hh = p.size[1] / 2.0;
                        lx.abs() <= hw && ly.abs() <= hh
                    })
                    .unwrap_or(false);

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
                            let current_pos = props.as_ref().map(|p| p.position).unwrap_or([scene.x as f32, scene.y as f32]);
                            ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                actor: actor.clone(),
                                property: "placement_mode".into(),
                                value: PropertyValue::Text("manual".into()),
                                create_keyframe: ctx.keyframe_mode,
                            })));
                            ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                actor: actor.clone(),
                                property: "position".into(),
                                value: PropertyValue::Vec2(current_pos),
                                create_keyframe: ctx.keyframe_mode,
                            })));
                            let mut actors = Vec::new();
                            for sel in ctx.selected_actors.iter() {
                                let pos = if let Some(p) = ctx.get_actor_props(sel) {
                                    p.position
                                } else {
                                    ctx.hit_regions
                                        .iter()
                                        .find(|(l, _)| l == sel)
                                        .map(|(_, r)| [(r.x0 + r.x1) as f32 / 2.0, (r.y0 + r.y1) as f32 / 2.0])
                                        .unwrap_or([0.0, 0.0])
                                };
                                actors.push((sel.clone(), pos));
                            }
                            *ctx.drag_state = DragState::Move {
                                primary: actor,
                                actors,
                                start_scene: scene,
                            };
                            return true;
                        }
                        if let Some((container, layout_type, source_index)) = ctx.find_layout_container(&actor) {
                            *ctx.drag_state = DragState::Reorder {
                                actor,
                                container,
                                source_index,
                                target_index: source_index,
                                layout_type,
                            };
                            return true;
                        }
                        return true;
                    }

                    let alt = ui.input(|i| i.modifiers.alt);
                    if alt {
                        ctx.commands.push_back(ShellAction::Command(Command::DuplicateActor(actor)));
                        return true;
                    }

                    let mut actors = Vec::new();
                    for sel in ctx.selected_actors.iter() {
                        let pos = if let Some(p) = ctx.get_actor_props(sel) {
                            p.position
                        } else {
                            ctx.hit_regions
                                .iter()
                                .find(|(l, _)| l == sel)
                                .map(|(_, r)| [(r.x0 + r.x1) as f32 / 2.0, (r.y0 + r.y1) as f32 / 2.0])
                                .unwrap_or([0.0, 0.0])
                        };
                        actors.push((sel.clone(), pos));
                    }
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

        if !is_dragging {
            if let (Some(mouse), Some(_start)) = (raw_pointer_pos, ctx.selection.marquee_start) {
                ctx.selection.marquee_current = Some(mouse);
            }
        } else if let Some(mouse) = raw_pointer_pos {
            let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
            let shift = ui.input(|i| i.modifiers.shift);

            match ctx.drag_state.clone() {
                DragState::Move { primary: _, actors, start_scene } => {
                    let raw_dx = (scene.x - start_scene.x) as f32;
                    let raw_dy = (scene.y - start_scene.y) as f32;
                    let (dx, dy) = if shift {
                        if raw_dx.abs() > raw_dy.abs() { (raw_dx, 0.0) } else { (0.0, raw_dy) }
                    } else { (raw_dx, raw_dy) };

                    let snap_enabled = ctx.preview.snap.snap_enabled && !ui.input(|i| i.modifiers.alt);
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

                        let mut snapped_guide_h = false;
                        let mut snapped_guide_v = false;
                        let mut snapped_actor_h = false;
                        let mut snapped_actor_v = false;
                        let mut snapped_container = false;
                        let mut snapped_keyframe = false;
                        let mut snap_hud_text: Option<String> = None;

                        if snap_enabled {
                            for &guide_y in &ctx.preview.guides.horizontal_guides {
                                if (ny - guide_y).abs() < threshold { ny = guide_y; snapped_guide_h = true; snap_hud_text = Some(format!("Guide y={}", guide_y as i32)); }
                            }
                            for &guide_x in &ctx.preview.guides.vertical_guides {
                                if (nx - guide_x).abs() < threshold { nx = guide_x; snapped_guide_v = true; snap_hud_text = Some(format!("Guide x={}", guide_x as i32)); }
                            }

                            let dragged_props = ctx.get_actor_props(&actor);
                            let half_w = dragged_props.as_ref().map(|p| p.size[0] / 2.0).unwrap_or(0.0);
                            let half_h = dragged_props.as_ref().map(|p| p.size[1] / 2.0).unwrap_or(0.0);
                            let dragged_x_edges = [nx - half_w, nx, nx + half_w];
                            let dragged_y_edges = [ny - half_h, ny, ny + half_h];
                            let edge_labels = ["left", "center", "right"];
                            let edge_labels_y = ["top", "center", "bottom"];

                            for (other_label, other_bounds) in ctx.hit_regions.iter() {
                                if other_label == &actor { continue; }
                                let other_x_edges = [other_bounds.x0 as f32, (other_bounds.x0 + other_bounds.x1) as f32 / 2.0, other_bounds.x1 as f32];
                                let other_y_edges = [other_bounds.y0 as f32, (other_bounds.y0 + other_bounds.y1) as f32 / 2.0, other_bounds.y1 as f32];

                                for &de in dragged_x_edges.iter() {
                                    for (oi, &oe) in other_x_edges.iter().enumerate() {
                                        let candidate_nx = nx + (oe - de);
                                        if (candidate_nx - nx).abs() < threshold && (candidate_nx - nx).abs() > 0.001 {
                                            nx = candidate_nx; snapped_actor_v = true;
                                            snap_hud_text = Some(format!("{} {}", other_label, edge_labels[oi]));
                                        }
                                    }
                                }
                                for &de in dragged_y_edges.iter() {
                                    for (oi, &oe) in other_y_edges.iter().enumerate() {
                                        let candidate_ny = ny + (oe - de);
                                        if (candidate_ny - ny).abs() < threshold && (candidate_ny - ny).abs() > 0.001 {
                                            ny = candidate_ny; snapped_actor_h = true;
                                            snap_hud_text = Some(format!("{} {}", other_label, edge_labels_y[oi]));
                                        }
                                    }
                                }
                            }

                            if let Some((container, _, _)) = ctx.find_layout_container(&actor) {
                                if let Some(container_props) = ctx.get_actor_props(&container) {
                                    if (nx - container_props.position[0]).abs() < threshold { nx = container_props.position[0]; snapped_container = true; snap_hud_text = Some(format!("{} center X", container)); }
                                    if (ny - container_props.position[1]).abs() < threshold { ny = container_props.position[1]; snapped_container = true; snap_hud_text = Some(format!("{} center Y", container)); }
                                    let c_hw = container_props.size[0] / 2.0;
                                    let c_left = container_props.position[0] - c_hw;
                                    let c_right = container_props.position[0] + c_hw;
                                    if (nx - c_left).abs() < threshold { nx = c_left; snapped_container = true; snap_hud_text = Some(format!("{} left", container)); }
                                    if (nx - c_right).abs() < threshold { nx = c_right; snapped_container = true; snap_hud_text = Some(format!("{} right", container)); }
                                    let c_hh = container_props.size[1] / 2.0;
                                    let c_top = container_props.position[1] - c_hh;
                                    let c_bottom = container_props.position[1] + c_hh;
                                    if (ny - c_top).abs() < threshold { ny = c_top; snapped_container = true; snap_hud_text = Some(format!("{} top", container)); }
                                    if (ny - c_bottom).abs() < threshold { ny = c_bottom; snapped_container = true; snap_hud_text = Some(format!("{} bottom", container)); }
                                }
                            }

                            if let Some(track) = ctx.timeline.and_then(|t| t.get_track(&actor)) {
                                if let Some(ref pos_track) = track.position {
                                    let prev_kf_time = pos_track.keyframes.range(..time_ms).next_back().map(|(&t, _)| t);
                                    if let Some(kf_ms) = prev_kf_time {
                                        if let Some(kf_props) = ctx.get_actor_props_at_time(&actor, kf_ms) {
                                            if (nx - kf_props.position[0]).abs() < threshold { nx = kf_props.position[0]; snapped_keyframe = true; snap_hud_text = Some(format!("prev keyframe ({:.2}s)", kf_ms as f64 / 1000.0)); }
                                            if (ny - kf_props.position[1]).abs() < threshold { ny = kf_props.position[1]; snapped_keyframe = true; snap_hud_text = Some(format!("prev keyframe ({:.2}s)", kf_ms as f64 / 1000.0)); }
                                        }
                                    }
                                }
                            }
                        }

                        if snapped_guide_h || snapped_actor_h || snapped_container || snapped_keyframe { ctx.preview.snap.snap_lines_h.push(ny); }
                        if snapped_guide_v || snapped_actor_v || snapped_container || snapped_keyframe { ctx.preview.snap.snap_lines_v.push(nx); }
                        if snapped_guide_h || snapped_guide_v || snapped_actor_h || snapped_actor_v || snapped_container || snapped_keyframe {
                            ctx.preview.snap.snap_line_color = Some(
                                if snapped_guide_h || snapped_guide_v { AMBER }
                                else if snapped_keyframe { ACCENT_CYAN }
                                else if snapped_container { ACCENT_BLUE }
                                else { GREEN }
                            );
                            ctx.preview.snap.snap_hud_label = snap_hud_text;
                        }

                        let binding = ctx.timeline.and_then(|t| t.get_track(&actor))
                            .map(|tr| tr.position_binding.get(time_ms, PositionBinding::Absolute))
                            .unwrap_or(PositionBinding::Absolute);

                        match binding {
                            PositionBinding::SceneAnchor { anchor, .. } => {
                                let anchor_pt = animatix::timeline::scene_anchor_point(anchor, ctx.scene_dimensions);
                                ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                    actor, property: "offset".into(), value: PropertyValue::Vec2([nx - anchor_pt.x as f32, ny - anchor_pt.y as f32]), create_keyframe: ctx.keyframe_mode,
                                })));
                            }
                            PositionBinding::ScenePercent { .. } => {
                                let w = ctx.scene_dimensions.width.max(1) as f32;
                                let h = ctx.scene_dimensions.height.max(1) as f32;
                                ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                    actor, property: "at".into(), value: PropertyValue::Vec2([nx / w, ny / h]), create_keyframe: ctx.keyframe_mode,
                                })));
                            }
                            _ => {
                                ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                    actor, property: "position".into(), value: PropertyValue::Vec2([nx, ny]), create_keyframe: ctx.keyframe_mode,
                                })));
                            }
                        }
                    }
                }
                DragState::Scale { actor, handle, start_scene, start_position, start_size, start_rotation, anchor_local, constrain_axis, uniform_ratio, resize_mode, start_scale } => {
                    let dx_world = (scene.x - start_scene.x) as f32;
                    let dy_world = (scene.y - start_scene.y) as f32;
                    let cos = (-start_rotation).cos();
                    let sin = (-start_rotation).sin();
                    let dx_local = dx_world * cos - dy_world * sin;
                    let dy_local = dx_world * sin + dy_world * cos;

                    let sign = match handle {
                        0 => [-1.0, -1.0], 1 => [1.0, -1.0], 2 => [1.0, 1.0], 3 => [-1.0, 1.0],
                        4 => [0.0, -1.0], 5 => [1.0, 0.0], 6 => [0.0, 1.0], 7 => [-1.0, 0.0],
                        _ => [1.0, 1.0],
                    };

                    let mut new_w = start_size[0];
                    let mut new_h = start_size[1];
                    if sign[0] != 0.0 { new_w = (start_size[0] + sign[0] * dx_local).max(PREVIEW_MIN_ACTOR_SIZE); }
                    if sign[1] != 0.0 { new_h = (start_size[1] + sign[1] * dy_local).max(PREVIEW_MIN_ACTOR_SIZE); }

                    let force_uniform = resize_mode == preview::ResizeMode::Scale;
                    let uniform = shift || uniform_ratio || force_uniform;
                    if uniform {
                        let scale_w = new_w / start_size[0].max(1.0);
                        let scale_h = new_h / start_size[1].max(1.0);
                        let s = if constrain_axis && !force_uniform {
                            if sign[0] == 0.0 { scale_h } else { scale_w }
                        } else { scale_w.max(scale_h) };
                        new_w = (start_size[0] * s).max(PREVIEW_MIN_ACTOR_SIZE);
                        new_h = (start_size[1] * s).max(PREVIEW_MIN_ACTOR_SIZE);
                    }

                    let cos_rot = start_rotation.cos();
                    let sin_rot = start_rotation.sin();
                    let old_anchor_local = [anchor_local[0], anchor_local[1]];
                    let new_anchor_local = [old_anchor_local[0] * new_w / start_size[0].max(1.0), old_anchor_local[1] * new_h / start_size[1].max(1.0)];
                    let anchor_world_x = start_position[0] + old_anchor_local[0] * cos_rot - old_anchor_local[1] * sin_rot;
                    let anchor_world_y = start_position[1] + old_anchor_local[0] * sin_rot + old_anchor_local[1] * cos_rot;
                    let new_pos_x = anchor_world_x - new_anchor_local[0] * cos_rot + new_anchor_local[1] * sin_rot;
                    let new_pos_y = anchor_world_y - new_anchor_local[0] * sin_rot - new_anchor_local[1] * cos_rot;

                    if resize_mode == preview::ResizeMode::Scale {
                        let ratio = new_w / start_size[0].max(1.0);
                        ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                            actor: actor.clone(), property: "scale".into(), value: PropertyValue::Float((start_scale * ratio).max(PREVIEW_MIN_SCALE)), create_keyframe: ctx.keyframe_mode,
                        })));
                    } else {
                        ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                            actor: actor.clone(), property: "size".into(), value: PropertyValue::Vec2([new_w, new_h]), create_keyframe: ctx.keyframe_mode,
                        })));
                    }

                    let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                    let binding = ctx.timeline.and_then(|t| t.get_track(&actor))
                        .map(|tr| tr.position_binding.get(time_ms, PositionBinding::Absolute))
                        .unwrap_or(PositionBinding::Absolute);

                    match binding {
                        PositionBinding::SceneAnchor { anchor, .. } => {
                            let anchor_pt = animatix::timeline::scene_anchor_point(anchor, ctx.scene_dimensions);
                            ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                actor, property: "offset".into(), value: PropertyValue::Vec2([new_pos_x - anchor_pt.x as f32, new_pos_y - anchor_pt.y as f32]), create_keyframe: ctx.keyframe_mode,
                            })));
                        }
                        PositionBinding::ScenePercent { .. } => {
                            let w = ctx.scene_dimensions.width.max(1) as f32;
                            let h = ctx.scene_dimensions.height.max(1) as f32;
                            ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                actor, property: "at".into(), value: PropertyValue::Vec2([new_pos_x / w, new_pos_y / h]), create_keyframe: ctx.keyframe_mode,
                            })));
                        }
                        _ => {
                            ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                actor, property: "position".into(), value: PropertyValue::Vec2([new_pos_x, new_pos_y]), create_keyframe: ctx.keyframe_mode,
                            })));
                        }
                    }
                }
                DragState::Rotate { actor, start_angle, start_rotation, pivot } => {
                    let angle = ((scene.y - pivot[1] as f64) as f32).atan2((scene.x - pivot[0] as f64) as f32);
                    let mut delta = angle - start_angle;
                    while delta > std::f32::consts::PI { delta -= 2.0 * std::f32::consts::PI; }
                    while delta < -std::f32::consts::PI { delta += 2.0 * std::f32::consts::PI; }
                    let mut new_rot = start_rotation + delta;
                    if shift { new_rot = (new_rot / ctx.rotation_snap_degrees.to_radians()).round() * ctx.rotation_snap_degrees.to_radians(); }
                    ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                        actor, property: "rotation".into(), value: PropertyValue::Float(new_rot), create_keyframe: ctx.keyframe_mode,
                    })));
                }
                DragState::Reorder { actor, container, source_index: _, target_index: _, layout_type } => {
                    let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                    if let Some(timeline) = ctx.timeline {
                        let order = timeline.get_child_order(&container, time_ms);
                        let siblings: Vec<String> = order.into_iter().filter(|l| l != &actor).collect();
                        let positions: Vec<f32> = siblings.iter().map(|label| {
                            ctx.hit_regions.iter().find(|(l, _)| l == label)
                                .map(|(_, bounds)| if layout_type == animatix::timeline::LayoutType::Row { (bounds.x0 + bounds.x1) as f32 / 2.0 } else { (bounds.y0 + bounds.y1) as f32 / 2.0 })
                                .or_else(|| ctx.get_actor_props(label).map(|p| if layout_type == animatix::timeline::LayoutType::Row { p.position[0] } else { p.position[1] }))
                                .unwrap_or(if layout_type == animatix::timeline::LayoutType::Row { scene.x as f32 } else { scene.y as f32 })
                        }).collect();

                        let mouse_coord = if layout_type == animatix::timeline::LayoutType::Row { scene.x as f32 } else { scene.y as f32 };
                        let mut insert_at = positions.len();
                        for (idx, coord) in positions.iter().enumerate() { if mouse_coord < *coord { insert_at = idx; break; } }
                        if let DragState::Reorder { target_index, .. } = &mut *ctx.drag_state { *target_index = insert_at; }
                    }
                }
                DragState::EditVertices { actor, vertex, start_points, start_scene } => {
                    let dx = (scene.x - start_scene.x) as f32;
                    let dy = (scene.y - start_scene.y) as f32;
                    let mut new_points = start_points.clone();
                    if let Some(p) = ctx.get_actor_props(&actor) {
                        let cos = (-p.rotation).cos();
                        let sin = (-p.rotation).sin();
                        let local_dx = dx * cos - dy * sin;
                        let local_dy = dx * sin + dy * cos;
                        if let Some(pt) = new_points.get_mut(vertex) { pt[0] += local_dx; pt[1] += local_dy; }
                    }
                    ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                        actor, property: "points".into(), value: PropertyValue::PointList(new_points), create_keyframe: ctx.keyframe_mode,
                    })));
                }
                DragState::MovePivot { actor, start_offset, start_scene } => {
                    let dx = (scene.x - start_scene.x) as f32;
                    let dy = (scene.y - start_scene.y) as f32;
                    if let Some(p) = ctx.get_actor_props(&actor) {
                        let cos = (-p.rotation).cos();
                        let sin = (-p.rotation).sin();
                        let local_dx = dx * cos - dy * sin;
                        let local_dy = dx * sin + dy * cos;
                        ctx.pivot_offsets.insert(actor, [start_offset[0] + local_dx, start_offset[1] + local_dy]);
                    }
                }
                DragState::MotionPath { actor, time_ms, start_position, start_scene } => {
                    let dx = (scene.x - start_scene.x) as f32;
                    let dy = (scene.y - start_scene.y) as f32;
                    let new_pos = [start_position[0] + dx, start_position[1] + dy];
                    ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                        actor,
                        property: "position".into(),
                        value: PropertyValue::Vec2(new_pos),
                        time_s: Some(time_ms as f64 / 1000.0),
                        create_keyframe: true,
                    })));
                }
                DragState::None => {}
            }
        }

        let pointer_released = ui.input(|i| i.pointer.any_released());
        if is_dragging && (response.drag_stopped() || pointer_released || !ui.input(|i| i.pointer.any_down())) {
            let old_drag_state = ctx.drag_state.clone();
            if let Some(tl) = ctx.timeline {
                let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                match &old_drag_state {
                    DragState::Move { primary, actors, .. } => {
                        if let Some(current_props) = ctx.get_actor_props(primary) {
                            if !tl.has_keyframe_at(primary, "position", time_ms) {
                                if let Some(start_pos) = actors.iter().find(|(l, _)| l == primary).map(|(_, p)| *p) {
                                    if current_props.position != start_pos {
                                        ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                            actor: primary.clone(), property: "position".into(), value: PropertyValue::Vec2(current_props.position), create_keyframe: true,
                                        })));
                                    }
                                }
                            }
                        }
                    }
                    DragState::Scale { actor, start_size, start_position, .. } => {
                        if let Some(current_props) = ctx.get_actor_props(actor) {
                            if !tl.has_keyframe_at(actor, "size", time_ms) && current_props.size != *start_size {
                                ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                    actor: actor.clone(), property: "size".into(), value: PropertyValue::Vec2(current_props.size), create_keyframe: true,
                                })));
                            }
                            if !tl.has_keyframe_at(actor, "position", time_ms) && current_props.position != *start_position {
                                ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                    actor: actor.clone(), property: "position".into(), value: PropertyValue::Vec2(current_props.position), create_keyframe: true,
                                })));
                            }
                        }
                    }
                    DragState::Rotate { actor, start_rotation, .. } => {
                        if let Some(current_props) = ctx.get_actor_props(actor) {
                            if !tl.has_keyframe_at(actor, "rotation", time_ms) && current_props.rotation != *start_rotation {
                                ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                    actor: actor.clone(), property: "rotation".into(), value: PropertyValue::Float(current_props.rotation), create_keyframe: true,
                                })));
                            }
                        }
                    }
                    _ => {}
                }
            }

            if let DragState::Reorder { actor, container, source_index, target_index, .. } = ctx.drag_state.clone() {
                if source_index != target_index {
                    let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
                    if let Some(timeline) = ctx.timeline {
                        let mut new_order = timeline.get_child_order(&container, time_ms);
                        if let Some(pos) = new_order.iter().position(|label| label == &actor) {
                            let item = new_order.remove(pos);
                            let insert_at = target_index.min(new_order.len());
                            new_order.insert(insert_at, item);
                            ctx.commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit { time_s: None,
                                actor: container, property: "child_order".into(), value: PropertyValue::StringList(new_order), create_keyframe: ctx.keyframe_mode,
                            })));
                        }
                    }
                }
            }
            ctx.commands.push_back(ShellAction::Drag(DragEvent::DragEnded));
        }

        if pointer_released && ctx.selection.marquee_start.is_some() {
            if let (Some(start), Some(current)) = (ctx.selection.marquee_start, ctx.selection.marquee_current) {
                let marquee_rect = egui::Rect::from_two_pos(start, current);
                let multi = ui.input(|i| i.modifiers.shift || i.modifiers.ctrl || i.modifiers.command);
                if !multi { ctx.selected_actors.clear(); }
                for (label, bounds) in ctx.hit_regions {
                    let is_locked = ctx.timeline
                        .and_then(|t| t.get_track(label))
                        .map(|tr| tr.locked)
                        .unwrap_or(false);
                    if is_locked { continue; }
                    let center = egui::pos2(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32);
                    if marquee_rect.contains(center) {
                        if multi && ctx.selected_actors.contains(label) { ctx.selected_actors.remove(label); }
                        else { ctx.selected_actors.insert(label.clone()); }
                    }
                }
            }
            ctx.selection.marquee_start = None;
            ctx.selection.marquee_current = None;
        }
        false
    }

