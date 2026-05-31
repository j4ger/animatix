//! Preview panel: canvas with rulers, zoom/pan, drag interaction, and overlays.

use std::collections::{HashMap, HashSet};

use egui::{Pos2, Vec2};

use crate::app::commands::{Command, CommandQueue, PropertyEdit, PropertyValue};
use crate::app::design_tokens::*;
use crate::app::panels::{nice_tick_interval, RULER_SIZE};
use crate::app::preview::{self, selection, ActorProps, DragState, fit_preview};
use crate::app::PreviewPaneState;
use animatix::timeline::{PositionBinding, SceneDimensions, Timeline, TrackAccessor};

pub(crate) struct PreviewContext<'a> {
    pub scene_dimensions: SceneDimensions,
    pub preview: &'a mut PreviewPaneState,
    pub preview_texture_id: Option<egui::TextureId>,
    pub commands: &'a mut CommandQueue,
    pub drag_state: &'a mut DragState,
    pub selection: &'a mut selection::SelectionState,
    pub selected_actors: &'a mut HashSet<String>,
    pub hit_regions: &'a [(String, kurbo::Rect)],
    pub timeline: Option<&'a Timeline>,
    pub pivot_offsets: &'a mut HashMap<String, [f32; 2]>,
    pub tool_mode: &'a mut preview::ToolMode,
    pub rotation_snap_degrees: f32,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub active_scene: Option<&'a str>,
    pub keyframe_mode: bool,
}

// ─── Helper methods ─────────────────────────────────────────────────────────

impl PreviewContext<'_> {
    fn get_actor_props(&self, actor: &str) -> Option<ActorProps> {
        let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
        self.get_actor_props_at_time(actor, time_ms)
    }

    fn get_actor_props_at_time(&self, actor: &str, time_ms: u64) -> Option<ActorProps> {
        let timeline = self.timeline.or_else(|| {
            let comp = self.composition?;
            let scene_name = self.active_scene?;
            comp.scenes.get(scene_name).map(|s| &s.timeline)
        })?;
        let track = timeline.get_track(actor)?;
        let half = track.size.as_ref().map(|pt| pt.evaluate(time_ms))?;
        let local_size = [half[0] * 2.0, half[1] * 2.0];
        let world_affine = timeline.actor_world_affine(actor, time_ms, self.scene_dimensions)?;
        let coeffs = world_affine.as_coeffs();
        let position = [coeffs[4] as f32, coeffs[5] as f32];
        let rotation = (coeffs[1] as f32).atan2(coeffs[0] as f32);
        let scale = ((coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt()) as f32;
        let size = [local_size[0] * scale, local_size[1] * scale];
        let pivot_offset = self.pivot_offsets.get(actor).copied().unwrap_or([0.0, 0.0]);
        Some(ActorProps { position, size, rotation, pivot_offset })
    }

    fn is_layout_managed(&self, actor: &str) -> bool {
        let timeline = self.timeline.or_else(|| {
            let comp = self.composition?;
            let scene_name = self.active_scene?;
            comp.scenes.get(scene_name).map(|s| &s.timeline)
        });
        let Some(timeline) = timeline else { return false; };
        let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
        preview::is_layout_managed(actor, timeline, time_ms)
    }

    fn find_layout_container(&self, actor: &str) -> Option<(String, animatix::timeline::LayoutType, usize)> {
        let timeline = self.timeline.or_else(|| {
            let comp = self.composition?;
            let scene_name = self.active_scene?;
            comp.scenes.get(scene_name).map(|s| &s.timeline)
        })?;
        let container = timeline
            .tracks()
            .iter()
            .find(|(_, track)| track.children.iter().any(|child| child == actor))?
            .0
            .clone();
        let metadata = timeline.container_metadata().get(&container)?;
        let source_index = timeline
            .get_track(&container)?
            .children
            .iter()
            .position(|child| child == actor)?;
        Some((container, metadata.layout_type, source_index))
    }

    fn preview_transform(&self, preview_rect: egui::Rect) -> preview::PreviewTransform {
        preview::PreviewTransform::new(
            self.scene_dimensions,
            preview_rect,
            self.preview.viewport.preview_zoom,
            self.preview.viewport.preview_pan,
        )
    }

    fn clamp_pan(&self, pan: Vec2, preview_rect: egui::Rect) -> Vec2 {
        Self::clamp_pan_value(pan, preview_rect, self.scene_dimensions, self.preview.viewport.preview_zoom)
    }

    /// Pure clamping math — extracted so it can be unit-tested.
    pub(super) fn clamp_pan_value(
        pan: Vec2,
        preview_rect: egui::Rect,
        scene_dimensions: SceneDimensions,
        zoom: f32,
    ) -> Vec2 {
        let tx = preview::PreviewTransform::new(
            scene_dimensions, preview_rect, zoom, Vec2::ZERO,
        );
        let (scale, _) = tx.scale();
        let scene_w = scene_dimensions.width as f64;
        let scene_h = scene_dimensions.height as f64;
        let preview_w = preview_rect.width() as f64;
        let preview_h = preview_rect.height() as f64;

        let visible_w = (preview_w * scale).min(scene_w);
        let visible_h = (preview_h * scale).min(scene_h);
        let half_w = visible_w / 2.0;
        let half_h = visible_h / 2.0;

        Vec2::new(
            pan.x.clamp(half_w as f32, (scene_w - half_w) as f32),
            pan.y.clamp(half_h as f32, (scene_h - half_h) as f32),
        )
    }

    fn preview_screen_to_scene(&self, preview_rect: egui::Rect, screen: egui::Pos2) -> kurbo::Point {
        self.preview_transform(preview_rect).screen_to_scene(screen)
    }

    fn preview_scene_to_screen(&self, preview_rect: egui::Rect, scene: kurbo::Point) -> egui::Pos2 {
        self.preview_transform(preview_rect).scene_to_screen(scene)
    }

    /// Handle drag start/update/end for the preview.
    fn handle_preview_drag(
        &mut self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        response: &egui::Response,
    ) -> bool {
        let is_dragging = !matches!(self.drag_state, DragState::None);
        let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());

        if ui.input(|i| i.pointer.middle_down()) {
            return false;
        }

        let drag_started = response.drag_started()
            || (!is_dragging && ui.input(|i| i.pointer.primary_pressed()));
        let hit_radius = preview::HANDLE_HIT_RADIUS * ui.ctx().pixels_per_point();

        if drag_started {
            if let (Some(actor), Some(mouse)) = (self.selected_actors.iter().next().cloned(), raw_pointer_pos) {
                let scene = self.preview_screen_to_scene(preview_rect, mouse);
                let props = self.get_actor_props(&actor);

                if let Some(ref p) = props {
                    let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
                    let vertex_points = self.timeline
                        .and_then(|t| t.get_track(&actor))
                        .and_then(|tr| tr.points.as_ref().map(|pt| pt.evaluate(time_ms)))
                        .filter(|pts| !pts.is_empty());

                    match *self.tool_mode {
                        preview::ToolMode::Move => {}
                        preview::ToolMode::Vertex => {
                            if let Some(ref points) = vertex_points {
                                if let Some(vidx) = preview::hit_test_vertex(mouse, p, points, preview_rect, self.scene_dimensions, preview_rect.size(), hit_radius * 2.0, self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan) {
                                    *self.drag_state = DragState::EditVertices {
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
                                std::array::from_fn(|i| self.preview_scene_to_screen(preview_rect, handle_world[i]));
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
                                let (resize_mode, start_scale) = self
                                    .timeline
                                    .and_then(|t| t.get_track(&actor))
                                    .map(|tr| {
                                        let mode = if let Some(primitive) = animatix::primitives::find_primitive(
                                            animatix::timeline::actor_kind_meta(tr.kind).type_name,
                                        ) {
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
                                *self.drag_state = DragState::Scale {
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
                            *self.drag_state = DragState::Rotate {
                                actor,
                                start_angle: angle,
                                start_rotation: p.rotation,
                                pivot,
                            };
                            return true;
                        }
                        preview::ToolMode::Pivot => {
                            let pivot_world_pt = preview::pivot_world(p);
                            let pivot_screen = self.preview_scene_to_screen(
                                preview_rect,
                                kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64),
                            );
                            if preview::hit_test_pivot(mouse, pivot_screen, hit_radius) {
                                *self.drag_state = DragState::MovePivot {
                                    actor,
                                    start_offset: p.pivot_offset,
                                    start_scene: scene,
                                };
                                return true;
                            }
                        }
                        preview::ToolMode::Select => {
                            if let Some(ref points) = vertex_points {
                                if let Some(vidx) = preview::hit_test_vertex(mouse, p, points, preview_rect, self.scene_dimensions, preview_rect.size(), hit_radius, self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan) {
                                    *self.drag_state = DragState::EditVertices {
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
                                std::array::from_fn(|i| self.preview_scene_to_screen(preview_rect, handle_world[i]));
                            if let Some(idx) = preview::hit_test_handle(mouse, &handle_screen, hit_radius) {
                                let anchor_local = if p.pivot_offset != [0.0, 0.0] {
                                    p.pivot_offset
                                } else {
                                    preview::handle_anchor_local(idx, p.size)
                                };
                                let (resize_mode, start_scale) = self
                                    .timeline
                                    .and_then(|t| t.get_track(&actor))
                                    .map(|tr| {
                                        let mode = if let Some(primitive) = animatix::primitives::find_primitive(
                                            animatix::timeline::actor_kind_meta(tr.kind).type_name,
                                        ) {
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
                                *self.drag_state = DragState::Scale {
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
                            let rot_screen = self.preview_scene_to_screen(preview_rect, rot_world);
                            if preview::hit_test_rotation_handle(mouse, rot_screen, hit_radius) {
                                let pivot = preview::pivot_world(p);
                                let angle = ((scene.y - pivot[1] as f64) as f32)
                                    .atan2((scene.x - pivot[0] as f64) as f32);
                                *self.drag_state = DragState::Rotate {
                                    actor: actor.clone(),
                                    start_angle: angle,
                                    start_rotation: p.rotation,
                                    pivot,
                                };
                                return true;
                            }

                            let pivot_world_pt = preview::pivot_world(p);
                            let pivot_screen = self.preview_scene_to_screen(
                                preview_rect,
                                kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64),
                            );
                            if preview::hit_test_pivot(mouse, pivot_screen, hit_radius) {
                                *self.drag_state = DragState::MovePivot {
                                    actor: actor.clone(),
                                    start_offset: p.pivot_offset,
                                    start_scene: scene,
                                };
                                return true;
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
                    || self
                        .hit_regions
                        .iter()
                        .rev()
                        .any(|(label, bounds)| label == &actor && bounds.contains(scene))
                {
                    if self.is_layout_managed(&actor) {
                        let shift = ui.input(|i| i.modifiers.shift);
                        if shift {
                            let current_pos = props.as_ref().map(|p| p.position).unwrap_or([scene.x as f32, scene.y as f32]);
                            self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                actor: actor.clone(),
                                property: "placement_mode".into(),
                                value: PropertyValue::Text("manual".into()),
                                create_keyframe: self.keyframe_mode,
                            }));
                            self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                actor: actor.clone(),
                                property: "position".into(),
                                value: PropertyValue::Vec2(current_pos),
                                create_keyframe: self.keyframe_mode,
                            }));
                            let mut actors = Vec::new();
                            for sel in self.selected_actors.iter() {
                                let pos = if let Some(p) = self.get_actor_props(sel) {
                                    p.position
                                } else {
                                    self.hit_regions
                                        .iter()
                                        .find(|(l, _)| l == sel)
                                        .map(|(_, r)| [(r.x0 + r.x1) as f32 / 2.0, (r.y0 + r.y1) as f32 / 2.0])
                                        .unwrap_or([0.0, 0.0])
                                };
                                actors.push((sel.clone(), pos));
                            }
                            *self.drag_state = DragState::Move {
                                primary: actor,
                                actors,
                                start_scene: scene,
                            };
                            return true;
                        }
                        if let Some((container, layout_type, source_index)) = self.find_layout_container(&actor) {
                            *self.drag_state = DragState::Reorder {
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
                        self.commands.push_back(Command::DuplicateActor(actor));
                        return true;
                    }

                    let mut actors = Vec::new();
                    for sel in self.selected_actors.iter() {
                        let pos = if let Some(p) = self.get_actor_props(sel) {
                            p.position
                        } else {
                            self.hit_regions
                                .iter()
                                .find(|(l, _)| l == sel)
                                .map(|(_, r)| [(r.x0 + r.x1) as f32 / 2.0, (r.y0 + r.y1) as f32 / 2.0])
                                .unwrap_or([0.0, 0.0])
                        };
                        actors.push((sel.clone(), pos));
                    }
                    *self.drag_state = DragState::Move {
                        primary: actor,
                        actors,
                        start_scene: scene,
                    };
                }
            } else if let Some(mouse) = raw_pointer_pos {
                self.selection.marquee_start = Some(mouse);
                self.selection.marquee_current = Some(mouse);
            }
        }

        if !is_dragging {
            if let (Some(mouse), Some(_start)) = (raw_pointer_pos, self.selection.marquee_start) {
                self.selection.marquee_current = Some(mouse);
            }
        } else if let Some(mouse) = raw_pointer_pos {
            let scene = self.preview_screen_to_scene(preview_rect, mouse);
            let shift = ui.input(|i| i.modifiers.shift);

            match self.drag_state.clone() {
                DragState::Move { primary: _, actors, start_scene } => {
                    let raw_dx = (scene.x - start_scene.x) as f32;
                    let raw_dy = (scene.y - start_scene.y) as f32;
                    let (dx, dy) = if shift {
                        if raw_dx.abs() > raw_dy.abs() { (raw_dx, 0.0) } else { (0.0, raw_dy) }
                    } else { (raw_dx, raw_dy) };

                    let snap_enabled = self.preview.snap.snap_enabled && !ui.input(|i| i.modifiers.alt);
                    let threshold = self.preview.snap.snap_threshold;

                    let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
                    for (actor, start_position) in actors {
                        let mut nx = start_position[0] + dx;
                        let mut ny = start_position[1] + dy;

                        if self.preview.overlay.show_grid {
                            let grid = self.preview.overlay.grid_size;
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
                            for &guide_y in &self.preview.guides.horizontal_guides {
                                if (ny - guide_y).abs() < threshold { ny = guide_y; snapped_guide_h = true; snap_hud_text = Some(format!("Guide y={}", guide_y as i32)); }
                            }
                            for &guide_x in &self.preview.guides.vertical_guides {
                                if (nx - guide_x).abs() < threshold { nx = guide_x; snapped_guide_v = true; snap_hud_text = Some(format!("Guide x={}", guide_x as i32)); }
                            }

                            let dragged_props = self.get_actor_props(&actor);
                            let half_w = dragged_props.as_ref().map(|p| p.size[0] / 2.0).unwrap_or(0.0);
                            let half_h = dragged_props.as_ref().map(|p| p.size[1] / 2.0).unwrap_or(0.0);
                            let dragged_x_edges = [nx - half_w, nx, nx + half_w];
                            let dragged_y_edges = [ny - half_h, ny, ny + half_h];
                            let edge_labels = ["left", "center", "right"];
                            let edge_labels_y = ["top", "center", "bottom"];

                            for (other_label, other_bounds) in self.hit_regions.iter() {
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

                            if let Some((container, _, _)) = self.find_layout_container(&actor) {
                                if let Some(container_props) = self.get_actor_props(&container) {
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

                            if let Some(track) = self.timeline.and_then(|t| t.get_track(&actor)) {
                                if let Some(ref pos_track) = track.position {
                                    let prev_kf_time = pos_track.keyframes.range(..time_ms).next_back().map(|(&t, _)| t);
                                    if let Some(kf_ms) = prev_kf_time {
                                        if let Some(kf_props) = self.get_actor_props_at_time(&actor, kf_ms) {
                                            if (nx - kf_props.position[0]).abs() < threshold { nx = kf_props.position[0]; snapped_keyframe = true; snap_hud_text = Some(format!("prev keyframe ({:.2}s)", kf_ms as f64 / 1000.0)); }
                                            if (ny - kf_props.position[1]).abs() < threshold { ny = kf_props.position[1]; snapped_keyframe = true; snap_hud_text = Some(format!("prev keyframe ({:.2}s)", kf_ms as f64 / 1000.0)); }
                                        }
                                    }
                                }
                            }
                        }

                        if snapped_guide_h || snapped_actor_h || snapped_container || snapped_keyframe { self.preview.snap.snap_lines_h.push(ny); }
                        if snapped_guide_v || snapped_actor_v || snapped_container || snapped_keyframe { self.preview.snap.snap_lines_v.push(nx); }
                        if snapped_guide_h || snapped_guide_v || snapped_actor_h || snapped_actor_v || snapped_container || snapped_keyframe {
                            self.preview.snap.snap_line_color = Some(
                                if snapped_guide_h || snapped_guide_v { AMBER }
                                else if snapped_keyframe { ACCENT_CYAN }
                                else if snapped_container { ACCENT_BLUE }
                                else { GREEN }
                            );
                            self.preview.snap.snap_hud_label = snap_hud_text;
                        }

                        let binding = self.timeline.and_then(|t| t.get_track(&actor))
                            .map(|tr| tr.position_binding.get(time_ms, PositionBinding::Absolute))
                            .unwrap_or(PositionBinding::Absolute);

                        match binding {
                            PositionBinding::SceneAnchor { anchor, .. } => {
                                let anchor_pt = animatix::timeline::scene_anchor_point(anchor, self.scene_dimensions);
                                self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                    actor, property: "offset".into(), value: PropertyValue::Vec2([nx - anchor_pt.x as f32, ny - anchor_pt.y as f32]), create_keyframe: self.keyframe_mode,
                                }));
                            }
                            PositionBinding::ScenePercent { .. } => {
                                let w = self.scene_dimensions.width.max(1) as f32;
                                let h = self.scene_dimensions.height.max(1) as f32;
                                self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                    actor, property: "at".into(), value: PropertyValue::Vec2([nx / w, ny / h]), create_keyframe: self.keyframe_mode,
                                }));
                            }
                            _ => {
                                self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                    actor, property: "position".into(), value: PropertyValue::Vec2([nx, ny]), create_keyframe: self.keyframe_mode,
                                }));
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

                    let min_size = 10.0;
                    let mut new_w = start_size[0];
                    let mut new_h = start_size[1];
                    if sign[0] != 0.0 { new_w = (start_size[0] + sign[0] * dx_local).max(min_size); }
                    if sign[1] != 0.0 { new_h = (start_size[1] + sign[1] * dy_local).max(min_size); }

                    let force_uniform = resize_mode == preview::ResizeMode::Scale;
                    let uniform = shift || uniform_ratio || force_uniform;
                    if uniform {
                        let scale_w = new_w / start_size[0].max(1.0);
                        let scale_h = new_h / start_size[1].max(1.0);
                        let s = if constrain_axis && !force_uniform {
                            if sign[0] == 0.0 { scale_h } else { scale_w }
                        } else { scale_w.max(scale_h) };
                        new_w = (start_size[0] * s).max(min_size);
                        new_h = (start_size[1] * s).max(min_size);
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
                        self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                            actor: actor.clone(), property: "scale".into(), value: PropertyValue::Float((start_scale * ratio).max(0.01)), create_keyframe: self.keyframe_mode,
                        }));
                    } else {
                        self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                            actor: actor.clone(), property: "size".into(), value: PropertyValue::Vec2([new_w, new_h]), create_keyframe: self.keyframe_mode,
                        }));
                    }

                    let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
                    let binding = self.timeline.and_then(|t| t.get_track(&actor))
                        .map(|tr| tr.position_binding.get(time_ms, PositionBinding::Absolute))
                        .unwrap_or(PositionBinding::Absolute);

                    match binding {
                        PositionBinding::SceneAnchor { anchor, .. } => {
                            let anchor_pt = animatix::timeline::scene_anchor_point(anchor, self.scene_dimensions);
                            self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                actor, property: "offset".into(), value: PropertyValue::Vec2([new_pos_x - anchor_pt.x as f32, new_pos_y - anchor_pt.y as f32]), create_keyframe: self.keyframe_mode,
                            }));
                        }
                        PositionBinding::ScenePercent { .. } => {
                            let w = self.scene_dimensions.width.max(1) as f32;
                            let h = self.scene_dimensions.height.max(1) as f32;
                            self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                actor, property: "at".into(), value: PropertyValue::Vec2([new_pos_x / w, new_pos_y / h]), create_keyframe: self.keyframe_mode,
                            }));
                        }
                        _ => {
                            self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                actor, property: "position".into(), value: PropertyValue::Vec2([new_pos_x, new_pos_y]), create_keyframe: self.keyframe_mode,
                            }));
                        }
                    }
                }
                DragState::Rotate { actor, start_angle, start_rotation, pivot } => {
                    let angle = ((scene.y - pivot[1] as f64) as f32).atan2((scene.x - pivot[0] as f64) as f32);
                    let mut delta = angle - start_angle;
                    while delta > std::f32::consts::PI { delta -= 2.0 * std::f32::consts::PI; }
                    while delta < -std::f32::consts::PI { delta += 2.0 * std::f32::consts::PI; }
                    let mut new_rot = start_rotation + delta;
                    if shift { new_rot = (new_rot / self.rotation_snap_degrees.to_radians()).round() * self.rotation_snap_degrees.to_radians(); }
                    self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                        actor, property: "rotation".into(), value: PropertyValue::Float(new_rot), create_keyframe: self.keyframe_mode,
                    }));
                }
                DragState::Reorder { actor, container, source_index: _, target_index: _, layout_type } => {
                    let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
                    if let Some(timeline) = self.timeline {
                        let order = timeline.get_child_order(&container, time_ms);
                        let siblings: Vec<String> = order.into_iter().filter(|l| l != &actor).collect();
                        let positions: Vec<f32> = siblings.iter().map(|label| {
                            self.hit_regions.iter().find(|(l, _)| l == label)
                                .map(|(_, bounds)| if layout_type == animatix::timeline::LayoutType::Row { (bounds.x0 + bounds.x1) as f32 / 2.0 } else { (bounds.y0 + bounds.y1) as f32 / 2.0 })
                                .or_else(|| self.get_actor_props(label).map(|p| if layout_type == animatix::timeline::LayoutType::Row { p.position[0] } else { p.position[1] }))
                                .unwrap_or(if layout_type == animatix::timeline::LayoutType::Row { scene.x as f32 } else { scene.y as f32 })
                        }).collect();

                        let mouse_coord = if layout_type == animatix::timeline::LayoutType::Row { scene.x as f32 } else { scene.y as f32 };
                        let mut insert_at = positions.len();
                        for (idx, coord) in positions.iter().enumerate() { if mouse_coord < *coord { insert_at = idx; break; } }
                        if let DragState::Reorder { target_index, .. } = &mut *self.drag_state { *target_index = insert_at; }
                    }
                }
                DragState::EditVertices { actor, vertex, start_points, start_scene } => {
                    let dx = (scene.x - start_scene.x) as f32;
                    let dy = (scene.y - start_scene.y) as f32;
                    let mut new_points = start_points.clone();
                    if let Some(p) = self.get_actor_props(&actor) {
                        let cos = (-p.rotation).cos();
                        let sin = (-p.rotation).sin();
                        let local_dx = dx * cos - dy * sin;
                        let local_dy = dx * sin + dy * cos;
                        if let Some(pt) = new_points.get_mut(vertex) { pt[0] += local_dx; pt[1] += local_dy; }
                    }
                    self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                        actor, property: "points".into(), value: PropertyValue::PointList(new_points), create_keyframe: self.keyframe_mode,
                    }));
                }
                DragState::MovePivot { actor, start_offset, start_scene } => {
                    let dx = (scene.x - start_scene.x) as f32;
                    let dy = (scene.y - start_scene.y) as f32;
                    if let Some(p) = self.get_actor_props(&actor) {
                        let cos = (-p.rotation).cos();
                        let sin = (-p.rotation).sin();
                        let local_dx = dx * cos - dy * sin;
                        let local_dy = dx * sin + dy * cos;
                        self.pivot_offsets.insert(actor, [start_offset[0] + local_dx, start_offset[1] + local_dy]);
                    }
                }
                DragState::None => {}
            }
        }

        let pointer_released = ui.input(|i| i.pointer.any_released());
        if is_dragging && (response.drag_stopped() || pointer_released || !ui.input(|i| i.pointer.any_down())) {
            let old_drag_state = self.drag_state.clone();
            if let Some(tl) = self.timeline {
                let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
                match &old_drag_state {
                    DragState::Move { primary, actors, .. } => {
                        if let Some(current_props) = self.get_actor_props(primary) {
                            if !tl.has_keyframe_at(primary, "position", time_ms) {
                                if let Some(start_pos) = actors.iter().find(|(l, _)| l == primary).map(|(_, p)| *p) {
                                    if current_props.position != start_pos {
                                        self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                            actor: primary.clone(), property: "position".into(), value: PropertyValue::Vec2(current_props.position), create_keyframe: true,
                                        }));
                                    }
                                }
                            }
                        }
                    }
                    DragState::Scale { actor, start_size, start_position, .. } => {
                        if let Some(current_props) = self.get_actor_props(actor) {
                            if !tl.has_keyframe_at(actor, "size", time_ms) && current_props.size != *start_size {
                                self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                    actor: actor.clone(), property: "size".into(), value: PropertyValue::Vec2(current_props.size), create_keyframe: true,
                                }));
                            }
                            if !tl.has_keyframe_at(actor, "position", time_ms) && current_props.position != *start_position {
                                self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                    actor: actor.clone(), property: "position".into(), value: PropertyValue::Vec2(current_props.position), create_keyframe: true,
                                }));
                            }
                        }
                    }
                    DragState::Rotate { actor, start_rotation, .. } => {
                        if let Some(current_props) = self.get_actor_props(actor) {
                            if !tl.has_keyframe_at(actor, "rotation", time_ms) && current_props.rotation != *start_rotation {
                                self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                    actor: actor.clone(), property: "rotation".into(), value: PropertyValue::Float(current_props.rotation), create_keyframe: true,
                                }));
                            }
                        }
                    }
                    _ => {}
                }
            }

            if let DragState::Reorder { actor, container, source_index, target_index, .. } = self.drag_state.clone() {
                if source_index != target_index {
                    let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
                    if let Some(timeline) = self.timeline {
                        let mut new_order = timeline.get_child_order(&container, time_ms);
                        if let Some(pos) = new_order.iter().position(|label| label == &actor) {
                            let item = new_order.remove(pos);
                            let insert_at = target_index.min(new_order.len());
                            new_order.insert(insert_at, item);
                            self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                actor: container, property: "child_order".into(), value: PropertyValue::StringList(new_order), create_keyframe: self.keyframe_mode,
                            }));
                        }
                    }
                }
            }
            self.commands.push_back(Command::DragEnded);
        }

        if pointer_released && self.selection.marquee_start.is_some() {
            if let (Some(start), Some(current)) = (self.selection.marquee_start, self.selection.marquee_current) {
                let marquee_rect = egui::Rect::from_two_pos(start, current);
                let multi = ui.input(|i| i.modifiers.shift || i.modifiers.ctrl || i.modifiers.command);
                if !multi { self.selected_actors.clear(); }
                for (label, bounds) in self.hit_regions {
                    let center = egui::pos2(((bounds.x0 + bounds.x1) / 2.0) as f32, ((bounds.y0 + bounds.y1) / 2.0) as f32);
                    if marquee_rect.contains(center) {
                        if multi && self.selected_actors.contains(label) { self.selected_actors.remove(label); }
                        else { self.selected_actors.insert(label.clone()); }
                    }
                }
            }
            self.selection.marquee_start = None;
            self.selection.marquee_current = None;
        }
        false
    }

    fn handle_preview_selection(&mut self, ui: &mut egui::Ui, _preview_rect: egui::Rect, response: &egui::Response) {
        if ui.input(|i| i.pointer.middle_down()) { return; }

        let is_dragging = !matches!(self.drag_state, DragState::None);

        if response.secondary_clicked() && !is_dragging {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let scene_dimensions = self.scene_dimensions;
                let zoom = self.preview.viewport.preview_zoom;
                let pan = self.preview.viewport.preview_pan;
                selection::handle_right_click(
                    self.selection, self.hit_regions, click_pos,
                    move |screen| {
                        let tx = preview::PreviewTransform::new(scene_dimensions, _preview_rect, zoom, pan);
                        tx.screen_to_scene(screen)
                    },
                );
            }
        }

        let mut menu_item_clicked = false;
        if self.selection.context_menu_open {
            let (selected, close, _rect) = selection::draw_context_menu(ui, self.selection, self.selected_actors);
            menu_item_clicked = close;
            if let Some(actor) = selected { self.selected_actors.clear(); self.selected_actors.insert(actor); }
            if close { self.selection.context_menu_open = false; }
        }

        let mut suppress_click = false;
        if self.selection.context_menu_open && !menu_item_clicked && ui.input(|i| i.pointer.primary_clicked()) {
            self.selection.context_menu_open = false;
            suppress_click = true;
            self.selected_actors.clear();
        }

        // ── End of context-menu / click handling ──
        if response.clicked() && !is_dragging && !self.selection.context_menu_open && !suppress_click {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let scene_dimensions = self.scene_dimensions;
                let zoom = self.preview.viewport.preview_zoom;
                let pan = self.preview.viewport.preview_pan;
                let modifiers = ui.ctx().input(|i| i.modifiers);
                selection::handle_click(
                    self.selection, self.selected_actors, self.hit_regions, click_pos,
                    move |screen| {
                        let tx = preview::PreviewTransform::new(scene_dimensions, _preview_rect, zoom, pan);
                        tx.screen_to_scene(screen)
                    },
                    &modifiers,
                );

                if let Some(comp) = self.composition {
                    if let Some(actor) = self.selected_actors.iter().next().cloned() {
                        let active_has_actor = self.active_scene.is_some_and(|scene| {
                            comp.scenes.get(scene).is_some_and(|s| s.timeline.has_actor(&actor))
                        });
                        if !active_has_actor {
                            for (scene_name, scene) in &comp.scenes {
                                if scene.timeline.has_actor(&actor) {
                                    self.commands.push_back(Command::SelectScene(scene_name.clone()));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn render_preview_cursor_feedback(&self, ui: &egui::Ui, preview_rect: egui::Rect) {
        let is_dragging = !matches!(self.drag_state, DragState::None);
        let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());
        let hit_radius = preview::HANDLE_HIT_RADIUS * ui.ctx().pixels_per_point();

        if !is_dragging && !self.selection.context_menu_open {
            if let Some(mouse) = raw_pointer_pos {
                let scene = self.preview_screen_to_scene(preview_rect, mouse);

                let over_handle = self.selected_actors.iter().next().and_then(|a| {
                    let props = self.get_actor_props(a)?;
                    let pivot_world_pt = preview::pivot_world(&props);
                    let pivot_screen = self.preview_scene_to_screen(preview_rect, kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64));
                    if preview::hit_test_pivot(mouse, pivot_screen, hit_radius) { return Some(9usize); }
                    let handle_world = preview::world_handle_positions(&props);
                    let handle_screen: [Pos2; 8] = std::array::from_fn(|i| self.preview_scene_to_screen(preview_rect, handle_world[i]));
                    if let Some(idx) = preview::hit_test_handle(mouse, &handle_screen, hit_radius) { Some(idx) }
                    else {
                        let rot_world = preview::rotation_handle_world(&props);
                        let rot_screen = self.preview_scene_to_screen(preview_rect, rot_world);
                        if preview::hit_test_rotation_handle(mouse, rot_screen, hit_radius) { Some(8usize) } else { None }
                    }
                });

                if let Some(handle_idx) = over_handle {
                    let (icon, tooltip) = match handle_idx {
                        0 => (egui::CursorIcon::ResizeNwSe, "Scale from top-left"),
                        1 => (egui::CursorIcon::ResizeNeSw, "Scale from top-right"),
                        2 => (egui::CursorIcon::ResizeNwSe, "Scale from bottom-right"),
                        3 => (egui::CursorIcon::ResizeNeSw, "Scale from bottom-left"),
                        4 => (egui::CursorIcon::ResizeVertical, "Scale height"),
                        5 => (egui::CursorIcon::ResizeHorizontal, "Scale width"),
                        6 => (egui::CursorIcon::ResizeVertical, "Scale height"),
                        7 => (egui::CursorIcon::ResizeHorizontal, "Scale width"),
                        8 => (egui::CursorIcon::Crosshair, "Rotate"),
                        9 => (egui::CursorIcon::Move, "Move pivot"),
                        _ => (egui::CursorIcon::Default, ""),
                    };
                    ui.ctx().set_cursor_icon(icon);
                    egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), egui::Id::new("handle_tooltip"), egui::PopupAnchor::Pointer)
                        .show(|ui| { ui.label(egui::RichText::new(tooltip).size(crate::app::design_tokens::FONT_SIZE_S)); });
                } else {
                    let is_over_selected = self.selected_actors.iter().next()
                        .and_then(|a| self.hit_regions.iter().find(|(l, _)| l == a).map(|(_, b)| b.contains(scene)))
                        .unwrap_or(false);
                    if is_over_selected {
                        let cursor = match *self.tool_mode {
                            preview::ToolMode::Move => egui::CursorIcon::Grab,
                            preview::ToolMode::Scale => egui::CursorIcon::ResizeNwSe,
                            preview::ToolMode::Rotate => egui::CursorIcon::Crosshair,
                            preview::ToolMode::Vertex => egui::CursorIcon::Crosshair,
                            preview::ToolMode::Pivot => egui::CursorIcon::Move,
                            preview::ToolMode::Select => egui::CursorIcon::Grab,
                        };
                        ui.ctx().set_cursor_icon(cursor);
                    } else if self.selection.hovered_actor.is_some() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
            }
        } else if !self.selection.context_menu_open {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }

    fn render_preview_content(&self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        match self.preview_texture_id {
            Some(texture_id) => {
                let zoom = self.preview.viewport.preview_zoom;
                let pan = self.preview.viewport.preview_pan;
                let scene_w = self.scene_dimensions.width.max(1) as f32;
                let scene_h = self.scene_dimensions.height.max(1) as f32;
                let tx = preview::PreviewTransform::new(self.scene_dimensions, preview_rect, zoom, pan);
                let display_rect = tx.display_rect();

                if (zoom - 1.0).abs() > 0.001 || pan != Vec2::new(scene_w / 2.0, scene_h / 2.0) {
                    let half_inv_zx = 0.5 / zoom.max(0.01);
                    let half_inv_zy = 0.5 / zoom.max(0.01);
                    let uv_cx = (pan.x / scene_w).clamp(0.0, 1.0);
                    let uv_cy = (pan.y / scene_h).clamp(0.0, 1.0);
                    let uv_rect = egui::Rect::from_min_max(
                        egui::pos2((uv_cx - half_inv_zx).clamp(0.0, 1.0), (uv_cy - half_inv_zy).clamp(0.0, 1.0)),
                        egui::pos2((uv_cx + half_inv_zx).clamp(0.0, 1.0), (uv_cy + half_inv_zy).clamp(0.0, 1.0)),
                    );
                    ui.put(display_rect, egui::Image::new((texture_id, display_rect.size())).uv(uv_rect));
                } else {
                    ui.put(display_rect, egui::Image::new((texture_id, display_rect.size())));
                }
            }
            None => {
                ui.painter().text(
                    preview_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Preview initializing…",
                    egui::TextStyle::Body.resolve(ui.style()),
                    TEXT_MUTED,
                );
            }
        }
    }

    fn render_preview_overlays(&mut self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        if self.selection.context_menu_open { return; }

        if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()).filter(|p| preview_rect.contains(*p)) {
            if !self.selected_actors.contains(self.selection.hovered_actor.as_deref().unwrap_or("")) {
                selection::draw_cycle_indicator(ui.painter(), mouse, self.selection.cycle_index, self.selection.click_candidates.len());
            }
        }

        if self.preview.overlay.show_hover_highlight {
            if let Some(hovered) = self.selection.hovered_actor.as_ref() {
                if !self.selected_actors.contains(hovered) {
                    if let Some(hover_rect) = preview::selection_screen_rect(
                        &HashSet::from([hovered.clone()]), self.hit_regions, preview_rect,
                        self.scene_dimensions, preview_rect.size(),
                        self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan,
                    ) {
                        selection::draw_hover_highlight(ui.painter(), hovered, hover_rect);
                    }
                }
            }
        }

        if self.preview.overlay.show_snap_guides {
            if let DragState::Move { primary, .. } | DragState::Scale { actor: primary, .. } = &self.drag_state {
                self.draw_snap_guides(ui, preview_rect, primary);
            }
        }

        if self.preview.overlay.show_snap_guides {
            if let Some(ref label) = self.preview.snap.snap_hud_label {
                if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                    let hud_pos = mouse + Vec2::new(12.0, -24.0);
                    let galley = ui.painter().layout_no_wrap(label.clone(), egui::FontId::proportional(FONT_SIZE_S), GREEN);
                    let padding = Vec2::new(8.0, 4.0);
                    let bg_rect = egui::Rect::from_min_size(hud_pos, galley.size() + padding * 2.0);
                    ui.painter().rect_filled(bg_rect, 3.0, crate::app::design_tokens::snap_guide_label_bg());
                    ui.painter().rect_stroke(bg_rect, 3.0, egui::Stroke::new(STROKE_WIDTH, crate::app::design_tokens::snap_guide_line()), egui::StrokeKind::Outside);
                    ui.painter().galley(hud_pos + padding, galley, GREEN);
                }
            }
        }
    }

    fn draw_snap_guides(&self, ui: &mut egui::Ui, preview_rect: egui::Rect, primary: &str) {
        let primary_props = self.get_actor_props(primary);
        let primary_rect = if let Some(p) = primary_props {
            let hw = p.size[0] / 2.0;
            let hh = p.size[1] / 2.0;
            let corners = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
            let mut min_x = f32::INFINITY; let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY; let mut max_y = f32::NEG_INFINITY;
            for corner in &corners {
                let world = preview::local_to_world(*corner, p.position, p.rotation);
                let screen = preview::scene_to_screen(world, preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                min_x = min_x.min(screen.x); min_y = min_y.min(screen.y);
                max_x = max_x.max(screen.x); max_y = max_y.max(screen.y);
            }
            egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y))
        } else {
            self.hit_regions.iter().find(|(l, _)| l == primary).map(|(_, bounds)| {
                let tl = preview::scene_to_screen(kurbo::Point::new(bounds.x0, bounds.y0), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                let br = preview::scene_to_screen(kurbo::Point::new(bounds.x1, bounds.y1), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                egui::Rect::from_min_max(tl, br)
            }).unwrap_or(preview_rect)
        };

        let threshold = 8.0;
        let guide_color = crate::app::design_tokens::accent_subtle();
        let guide_stroke = egui::Stroke::new(STROKE_WIDTH, guide_color);

        for (label, bounds) in self.hit_regions {
            if label == primary || self.selected_actors.contains(label) { continue; }
            let tl = preview::scene_to_screen(kurbo::Point::new(bounds.x0, bounds.y0), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
            let br = preview::scene_to_screen(kurbo::Point::new(bounds.x1, bounds.y1), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
            let other_rect = egui::Rect::from_min_max(tl, br);

            let px = [primary_rect.min.x, primary_rect.max.x, primary_rect.center().x];
            let py = [primary_rect.min.y, primary_rect.max.y, primary_rect.center().y];
            let ox = [other_rect.min.x, other_rect.max.x, other_rect.center().x];
            let oy = [other_rect.min.y, other_rect.max.y, other_rect.center().y];

            for &px in &px { for &ox in &ox { if (px - ox).abs() < threshold { ui.painter().line_segment([egui::pos2(px, preview_rect.min.y), egui::pos2(px, preview_rect.max.y)], guide_stroke); } } }
            for &py in &py { for &oy in &oy { if (py - oy).abs() < threshold { ui.painter().line_segment([egui::pos2(preview_rect.min.x, py), egui::pos2(preview_rect.max.x, py)], guide_stroke); } } }
        }
    }

    fn render_preview_selection_overlay(&self, ui: &mut egui::Ui, preview_rect: egui::Rect, is_dragging: bool) {
        if self.selected_actors.len() > 1 {
            let mut screen_rects = Vec::new();
            for actor in self.selected_actors.iter() {
                if let Some(props) = self.get_actor_props(actor) {
                    let hw = props.size[0] / 2.0; let hh = props.size[1] / 2.0;
                    let local_corners = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
                    let mut min_x = f32::INFINITY; let mut min_y = f32::INFINITY;
                    let mut max_x = f32::NEG_INFINITY; let mut max_y = f32::NEG_INFINITY;
                    for corner in &local_corners {
                        let world = preview::local_to_world(*corner, props.position, props.rotation);
                        let screen = preview::scene_to_screen(world, preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                        min_x = min_x.min(screen.x); min_y = min_y.min(screen.y);
                        max_x = max_x.max(screen.x); max_y = max_y.max(screen.y);
                    }
                    screen_rects.push(egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y)));
                } else if let Some((_, bounds)) = self.hit_regions.iter().find(|(l, _)| l == actor) {
                    let top_left = preview::scene_to_screen(kurbo::Point::new(bounds.x0, bounds.y0), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                    let br = preview::scene_to_screen(kurbo::Point::new(bounds.x1, bounds.y1), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                    screen_rects.push(egui::Rect::from_min_max(top_left, br));
                }
            }
            preview::draw_multi_selection_overlay(ui.painter(), &screen_rects, is_dragging, ui.ctx().pixels_per_point());
            return;
        }

        for actor in self.selected_actors.iter() {
            let props = self.get_actor_props(actor);
            let fallback = self.hit_regions.iter().find(|(l, _)| l == actor).map(|(_, bounds)| {
                let tl = preview::scene_to_screen(kurbo::Point::new(bounds.x0, bounds.y0), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                let br = preview::scene_to_screen(kurbo::Point::new(bounds.x1, bounds.y1), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                egui::Rect::from_min_max(tl, br)
            });
            preview::draw_selection_overlay(ui.painter(), props.as_ref(), fallback, is_dragging, preview_rect, self.scene_dimensions, preview_rect.size(), ui.ctx().pixels_per_point(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);

            let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
            let points = self.timeline.and_then(|t| t.get_track(actor)).and_then(|tr| tr.points.as_ref().map(|pt| pt.evaluate(time_ms))).filter(|pts| !pts.is_empty());
            if let (Some(ref p), Some(pts)) = (props, points) {
                let active_vertex = match &self.drag_state {
                    DragState::EditVertices { actor: drag_actor, vertex, .. } => if drag_actor == actor { Some(*vertex) } else { None },
                    _ => None,
                };
                preview::draw_vertex_handles(ui.painter(), p, &pts, preview_rect, self.scene_dimensions, preview_rect.size(), active_vertex, ui.ctx().pixels_per_point(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
            }

            if is_dragging {
                let measurement_color = ACCENT_BLUE; let text_color = TEXT_PRIMARY;
                let font = egui::FontId::monospace(FONT_SIZE_XS);
                match &self.drag_state {
                    DragState::Move { primary, actors: _, start_scene } => {
                        if let Some(props) = self.get_actor_props(primary) {
                            let start_screen = preview::scene_to_screen(kurbo::Point::new(start_scene.x, start_scene.y), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                            let current_screen = preview::scene_to_screen(kurbo::Point::new(props.position[0] as f64, props.position[1] as f64), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                            let y = (start_screen.y + current_screen.y) / 2.0;
                            ui.painter().line_segment([Pos2::new(start_screen.x.min(current_screen.x), y), Pos2::new(start_screen.x.max(current_screen.x), y)], egui::Stroke::new(STROKE_WIDTH, measurement_color));
                            ui.painter().text(Pos2::new((start_screen.x + current_screen.x) / 2.0, y - 8.0), egui::Align2::CENTER_BOTTOM, format!("Δx: {:+.0}", props.position[0] - start_scene.x as f32), font.clone(), text_color);
                            let x = (start_screen.x + current_screen.x) / 2.0;
                            ui.painter().line_segment([Pos2::new(x, start_screen.y.min(current_screen.y)), Pos2::new(x, start_screen.y.max(current_screen.y))], egui::Stroke::new(STROKE_WIDTH, measurement_color));
                            ui.painter().text(Pos2::new(x + 4.0, (start_screen.y + current_screen.y) / 2.0), egui::Align2::LEFT_CENTER, format!("Δy: {:+.0}", props.position[1] - start_scene.y as f32), font.clone(), text_color);
                        }
                    }
                    DragState::Scale { actor, start_size, .. } => {
                        if let Some(props) = self.get_actor_props(actor) {
                            let screen_pos = preview::scene_to_screen(kurbo::Point::new(props.position[0] as f64, props.position[1] as f64), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                            let br = preview::scene_to_screen(kurbo::Point::new(props.position[0] as f64 + props.size[0] as f64 / 2.0, props.position[1] as f64 + props.size[1] as f64 / 2.0), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                            ui.painter().text(Pos2::new(screen_pos.x, br.y + 12.0), egui::Align2::CENTER_TOP, format!("w: {:.0} → {:.0}", start_size[0], props.size[0]), font.clone(), text_color);
                            ui.painter().text(Pos2::new(br.x + 4.0, screen_pos.y), egui::Align2::LEFT_CENTER, format!("h: {:.0} → {:.0}", start_size[1], props.size[1]), font.clone(), text_color);
                        }
                    }
                    DragState::Rotate { actor, start_rotation, .. } => {
                        if let Some(props) = self.get_actor_props(actor) {
                            let screen_pos = preview::scene_to_screen(kurbo::Point::new(props.position[0] as f64, props.position[1] as f64), preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                            ui.painter().text(Pos2::new(screen_pos.x, screen_pos.y - props.size[1] / 2.0 - 16.0), egui::Align2::CENTER_BOTTOM, format!("{:.0}° → {:.0}°", start_rotation.to_degrees(), props.rotation.to_degrees()), font.clone(), text_color);
                        }
                    }
                    _ => {}
                }
            }

            if !is_dragging {
                if let Some(timeline) = self.timeline {
                    let current_time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
                    let keyframe_times = timeline.keyframe_times_s();
                    let mut prev_time_ms: Option<u64> = None; let mut next_time_ms: Option<u64> = None;
                    for &time_s in &keyframe_times {
                        let time_ms = (time_s * 1000.0) as u64;
                        if time_ms < current_time_ms { prev_time_ms = Some(time_ms); }
                        else if time_ms > current_time_ms && next_time_ms.is_none() { next_time_ms = Some(time_ms); }
                    }
                    if let Some(prev_ms) = prev_time_ms {
                        if let Some(prev_props) = self.get_actor_props_at_time(actor, prev_ms) {
                            preview::draw_ghost_overlay(ui.painter(), &prev_props, preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan, crate::app::design_tokens::ghost_prev());
                        }
                    }
                    if let Some(next_ms) = next_time_ms {
                        if let Some(next_props) = self.get_actor_props_at_time(actor, next_ms) {
                            preview::draw_ghost_overlay(ui.painter(), &next_props, preview_rect, self.scene_dimensions, preview_rect.size(), self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan, crate::app::design_tokens::ghost_next());
                        }
                    }
                }
            }

            if let DragState::Reorder { actor: drag_actor, container, target_index, layout_type, .. } = self.drag_state.clone() {
                if &drag_actor == actor {
                    let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
                    if let Some(timeline) = self.timeline {
                        let order = timeline.get_child_order(&container, time_ms);
                        let siblings: Vec<(String, [f32; 2])> = order.into_iter().filter(|label| label != actor).filter_map(|label| self.get_actor_props(&label).map(|p| (label, p.position))).collect();
                        if let Some(props) = props.as_ref() {
                            preview::draw_reorder_overlay(ui.painter(), props, target_index, &siblings, preview_rect, self.scene_dimensions, preview_rect.size(), layout_type == animatix::timeline::LayoutType::Row, self.preview.viewport.preview_zoom, self.preview.viewport.preview_pan);
                        }
                    }
                }
            }
        }

        if let (Some(start), Some(current)) = (self.selection.marquee_start, self.selection.marquee_current) {
            let marquee_rect = egui::Rect::from_two_pos(start, current);
            ui.painter().rect_filled(marquee_rect, 0.0, crate::app::design_tokens::accent_faint());
            ui.painter().rect_stroke(marquee_rect, 0.0, egui::Stroke::new(STROKE_WIDTH, crate::app::design_tokens::accent_subtle()), egui::StrokeKind::Outside);
        }
    }
}

// ─── Free functions for the preview canvas ─────────────────────────────────

fn preview_screen_to_scene(
    scene_dimensions: SceneDimensions,
    preview_rect: egui::Rect,
    screen: egui::Pos2,
    zoom: f32,
    pan: Vec2,
) -> kurbo::Point {
    let tx = preview::PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan);
    tx.screen_to_scene(screen)
}

fn preview_scene_to_screen(
    scene_dimensions: SceneDimensions,
    preview_rect: egui::Rect,
    scene: kurbo::Point,
    zoom: f32,
    pan: Vec2,
) -> egui::Pos2 {
    let tx = preview::PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan);
    tx.scene_to_screen(scene)
}

// ─── Main preview_panel_ui function ─────────────────────────────────────────

pub(crate) fn preview_panel_ui(ctx: &mut PreviewContext<'_>, ui: &mut egui::Ui) {
    // Preview uses zero-margin frame to maximize canvas area.
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::ZERO)
        .show(ui, |ui| {
        ui.vertical(|ui| {
            // Handle fit-zoom request from the global toolbar.
            if ctx.preview.fit_zoom_requested {
                ctx.preview.fit_zoom_requested = false;
                let avail = ui.available_size_before_wrap();
                let preview_avail = Vec2::new(
                    (avail.x - RULER_SIZE).max(200.0),
                    (avail.y - RULER_SIZE).max(180.0),
                );
                let desired = fit_preview(ctx.scene_dimensions, preview_avail);
                ctx.preview.viewport.preview_zoom = desired.x / ctx.scene_dimensions.width as f32;
                ctx.preview.viewport.preview_pan = Vec2::new(
                    ctx.scene_dimensions.width as f32 / 2.0,
                    ctx.scene_dimensions.height as f32 / 2.0,
                );
            }

            let available = ui.available_size_before_wrap();
            let preview_available = Vec2::new(
                (available.x - RULER_SIZE).max(200.0),
                (available.y - RULER_SIZE).max(180.0),
            );
            let desired = fit_preview(ctx.scene_dimensions, preview_available);
            let total_size = desired + Vec2::new(RULER_SIZE, RULER_SIZE);
            let (total_rect, _) = ui.allocate_exact_size(total_size, egui::Sense::hover());
            let preview_rect = egui::Rect::from_min_size(
                egui::pos2(total_rect.min.x + RULER_SIZE, total_rect.min.y + RULER_SIZE),
                desired,
            );
            let response = ui.allocate_rect(preview_rect, egui::Sense::click_and_drag());
            ui.painter().rect_stroke(preview_rect, RADIUS_L, egui::Stroke::new(STROKE_WIDTH, BORDER), egui::StrokeKind::Outside);
            ui.painter().rect_filled(preview_rect, RADIUS_L, BG_BASE);

            // ── Rulers ──
            let ruler_bg = BG_PANEL;
            let ruler_tick_color = TEXT_MUTED;
            let ruler_text_color = TEXT_MUTED;
            let ruler_label_color = TEXT_SECONDARY;

            let h_ruler_rect = egui::Rect::from_min_size(
                egui::pos2(preview_rect.min.x, preview_rect.min.y - RULER_SIZE),
                Vec2::new(preview_rect.width(), RULER_SIZE),
            );
            let v_ruler_rect = egui::Rect::from_min_size(
                egui::pos2(preview_rect.min.x - RULER_SIZE, preview_rect.min.y),
                Vec2::new(RULER_SIZE, preview_rect.height()),
            );
            let corner_rect = egui::Rect::from_min_size(
                egui::pos2(preview_rect.min.x - RULER_SIZE, preview_rect.min.y - RULER_SIZE),
                Vec2::new(RULER_SIZE, RULER_SIZE),
            );
            let ruler_stroke = egui::Stroke::new(STROKE_WIDTH, BORDER);

            ui.painter().rect_filled(corner_rect, 0.0, ruler_bg);
            ui.painter().rect_stroke(corner_rect, 0.0, ruler_stroke, egui::StrokeKind::Outside);

            let scene_tl = preview_screen_to_scene(
                ctx.scene_dimensions, preview_rect, preview_rect.left_top(),
                ctx.preview.viewport.preview_zoom, ctx.preview.viewport.preview_pan,
            );
            let scene_br = preview_screen_to_scene(
                ctx.scene_dimensions, preview_rect, preview_rect.right_bottom(),
                ctx.preview.viewport.preview_zoom, ctx.preview.viewport.preview_pan,
            );
            let visible_w = (scene_br.x - scene_tl.x) as f32;
            let visible_h = (scene_br.y - scene_tl.y) as f32;

            // Horizontal ruler
            ui.painter().rect_filled(h_ruler_rect, 0.0, ruler_bg);
            ui.painter().rect_stroke(h_ruler_rect, 0.0, ruler_stroke, egui::StrokeKind::Outside);
            let h_interval = nice_tick_interval(visible_w, h_ruler_rect.width() / 60.0).max(1.0);
            let h_start = ((scene_tl.x as f32) / h_interval).floor() as i32 * h_interval as i32;
            let h_end = ((scene_br.x as f32) / h_interval).ceil() as i32 * h_interval as i32;
            let mut tick_x = h_start as f32;
            while tick_x <= h_end as f32 {
                let screen_pt = preview_scene_to_screen(
                    ctx.scene_dimensions, preview_rect,
                    kurbo::Point::new(tick_x as f64, scene_tl.y),
                    ctx.preview.viewport.preview_zoom, ctx.preview.viewport.preview_pan,
                );
                if screen_pt.x >= h_ruler_rect.min.x && screen_pt.x <= h_ruler_rect.max.x {
                    let rel_x = screen_pt.x - h_ruler_rect.min.x;
                    let is_major = (tick_x as i32) % (h_interval as i32 * 5) == 0;
                    let tick_h = if is_major { RULER_SIZE * 0.6 } else { RULER_SIZE * 0.3 };
                    ui.painter().line_segment(
                        [egui::pos2(h_ruler_rect.min.x + rel_x, h_ruler_rect.max.y),
                         egui::pos2(h_ruler_rect.min.x + rel_x, h_ruler_rect.max.y - tick_h)],
                        egui::Stroke::new(STROKE_WIDTH, if is_major { ruler_label_color } else { ruler_tick_color }),
                    );
                    if is_major {
                        ui.painter().text(
                            egui::pos2(h_ruler_rect.min.x + rel_x, h_ruler_rect.min.y + RULER_SIZE * 0.3),
                            egui::Align2::CENTER_CENTER, format!("{}", tick_x as i32),
                            egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional), ruler_text_color,
                        );
                    }
                }
                tick_x += h_interval;
            }

            // Vertical ruler
            ui.painter().rect_filled(v_ruler_rect, 0.0, ruler_bg);
            ui.painter().rect_stroke(v_ruler_rect, 0.0, ruler_stroke, egui::StrokeKind::Outside);
            let v_interval = nice_tick_interval(visible_h, v_ruler_rect.height() / 60.0).max(1.0);
            let v_start = ((scene_tl.y as f32) / v_interval).floor() as i32 * v_interval as i32;
            let v_end = ((scene_br.y as f32) / v_interval).ceil() as i32 * v_interval as i32;
            let mut tick_y = v_start as f32;
            while tick_y <= v_end as f32 {
                let screen_pt = preview_scene_to_screen(
                    ctx.scene_dimensions, preview_rect,
                    kurbo::Point::new(scene_tl.x, tick_y as f64),
                    ctx.preview.viewport.preview_zoom, ctx.preview.viewport.preview_pan,
                );
                if screen_pt.y >= v_ruler_rect.min.y && screen_pt.y <= v_ruler_rect.max.y {
                    let rel_y = screen_pt.y - v_ruler_rect.min.y;
                    let is_major = (tick_y as i32) % (v_interval as i32 * 5) == 0;
                    let tick_w = if is_major { RULER_SIZE * 0.6 } else { RULER_SIZE * 0.3 };
                    ui.painter().line_segment(
                        [egui::pos2(v_ruler_rect.max.x, v_ruler_rect.min.y + rel_y),
                         egui::pos2(v_ruler_rect.max.x - tick_w, v_ruler_rect.min.y + rel_y)],
                        egui::Stroke::new(STROKE_WIDTH, if is_major { ruler_label_color } else { ruler_tick_color }),
                    );
                    if is_major {
                        ui.painter().text(
                            egui::pos2(v_ruler_rect.min.x + RULER_SIZE * 0.3, v_ruler_rect.min.y + rel_y),
                            egui::Align2::CENTER_CENTER, format!("{}", tick_y as i32),
                            egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional), ruler_text_color,
                        );
                    }
                }
                tick_y += v_interval;
            }

            // ── Ruler drag interaction ──
            let ruler_drag_id = ui.id().with("guide_ruler_drag");
            let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());
            let h_ruler_resp = ui.allocate_rect(h_ruler_rect, egui::Sense::drag());
            let v_ruler_resp = ui.allocate_rect(v_ruler_rect, egui::Sense::drag());

            if h_ruler_resp.drag_started() {
                if let Some(mouse) = raw_pointer_pos {
                    let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                    ui.data_mut(|d| d.insert_temp(ruler_drag_id, Some((false, scene.y as f32))));
                }
            }
            if v_ruler_resp.drag_started() {
                if let Some(mouse) = raw_pointer_pos {
                    let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                    ui.data_mut(|d| d.insert_temp(ruler_drag_id, Some((true, scene.x as f32))));
                }
            }

            let ruler_drag_active: Option<(bool, f32)> = ui.data(|d| d.get_temp(ruler_drag_id));
            if let Some((is_vertical, _start_val)) = ruler_drag_active {
                if let Some(mouse) = raw_pointer_pos {
                    let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                    let guide_color = AMBER;
                    if is_vertical {
                        let ghost_screen = ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(scene.x, 0.0));
                        if ghost_screen.x >= preview_rect.min.x && ghost_screen.x <= preview_rect.max.x {
                            ui.painter().line_segment([egui::pos2(ghost_screen.x, preview_rect.min.y), egui::pos2(ghost_screen.x, preview_rect.max.y)], egui::Stroke::new(STROKE_WIDTH, guide_color));
                        }
                    } else {
                        let ghost_screen = ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(0.0, scene.y));
                        if ghost_screen.y >= preview_rect.min.y && ghost_screen.y <= preview_rect.max.y {
                            ui.painter().line_segment([egui::pos2(preview_rect.min.x, ghost_screen.y), egui::pos2(preview_rect.max.x, ghost_screen.y)], egui::Stroke::new(STROKE_WIDTH, guide_color));
                        }
                    }
                }
            }

            let pointer_released = ui.input(|i| i.pointer.any_released());
            if let Some((is_vertical, _start_val)) = ruler_drag_active {
                if pointer_released || h_ruler_resp.drag_stopped() || v_ruler_resp.drag_stopped() {
                    if let Some(mouse) = raw_pointer_pos {
                        if preview_rect.contains(mouse) {
                            let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                            if is_vertical { ctx.preview.guides.vertical_guides.push(scene.x as f32); }
                            else { ctx.preview.guides.horizontal_guides.push(scene.y as f32); }
                        }
                    }
                    ui.data_mut(|d| d.remove::<Option<(bool, f32)>>(ruler_drag_id));
                }
            }

            // ── Draw existing guides ──
            if ctx.preview.overlay.show_guides {
                let guide_color = AMBER;
                for &guide_y in &ctx.preview.guides.horizontal_guides {
                    let screen_pt = ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(0.0, guide_y as f64));
                    if screen_pt.y >= preview_rect.min.y && screen_pt.y <= preview_rect.max.y {
                        ui.painter().line_segment([egui::pos2(preview_rect.min.x, screen_pt.y), egui::pos2(preview_rect.max.x, screen_pt.y)], egui::Stroke::new(STROKE_WIDTH, guide_color));
                    }
                }
                for &guide_x in &ctx.preview.guides.vertical_guides {
                    let screen_pt = ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(guide_x as f64, 0.0));
                    if screen_pt.x >= preview_rect.min.x && screen_pt.x <= preview_rect.max.x {
                        ui.painter().line_segment([egui::pos2(screen_pt.x, preview_rect.min.y), egui::pos2(screen_pt.x, preview_rect.max.y)], egui::Stroke::new(STROKE_WIDTH, guide_color));
                    }
                }
            }

            // ── Scroll zoom ──
            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta);
                if scroll.y != 0.0 {
                    let zoom_factor = 1.0 + scroll.y * 0.001;
                    let new_zoom = (ctx.preview.viewport.preview_zoom * zoom_factor).clamp(1.0, 10.0);
                    let prev_zoom = ctx.preview.viewport.preview_zoom;
                    if let Some(cursor) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                        let cursor_in_rect = preview_rect.contains(cursor);
                        if cursor_in_rect && prev_zoom > 0.01 {
                            let scene_at_cursor = ctx.preview_screen_to_scene(preview_rect, cursor);
                            let rel = cursor - preview_rect.center();
                            ctx.preview.viewport.preview_zoom = new_zoom;
                            let tx = preview::PreviewTransform::new(ctx.scene_dimensions, preview_rect, new_zoom, Vec2::ZERO);
                            let (new_scale, _) = tx.scale();
                            let new_pan = Vec2::new(
                                (scene_at_cursor.x - rel.x as f64 * new_scale) as f32,
                                (scene_at_cursor.y - rel.y as f64 * new_scale) as f32,
                            );
                            ctx.preview.viewport.preview_pan = ctx.clamp_pan(new_pan, preview_rect);
                            ctx.preview.status = format!("Zoom: {:.0}%", ctx.preview.viewport.preview_zoom * 100.0);
                        }
                    } else {
                        ctx.preview.viewport.preview_zoom = new_zoom;
                        ctx.preview.status = format!("Zoom: {:.0}%", ctx.preview.viewport.preview_zoom * 100.0);
                    }
                }
            }

            // ── Middle-click pan ──
            if ui.input(|i| i.pointer.middle_down()) {
                if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                    if preview_rect.contains(mouse) {
                        let delta = ui.input(|i| i.pointer.delta());
                        if delta != Vec2::ZERO {
                            let tx = preview::PreviewTransform::new(ctx.scene_dimensions, preview_rect, ctx.preview.viewport.preview_zoom, Vec2::ZERO);
                            let (scale, _) = tx.scale();
                            let new_pan = Vec2::new(
                                ctx.preview.viewport.preview_pan.x - delta.x * scale as f32,
                                ctx.preview.viewport.preview_pan.y - delta.y * scale as f32,
                            );
                            ctx.preview.viewport.preview_pan = ctx.clamp_pan(new_pan, preview_rect);
                        }
                    }
                }
            }

            // Clear snap lines from previous frame
            ctx.preview.snap.snap_lines_h.clear();
            ctx.preview.snap.snap_lines_v.clear();
            ctx.preview.snap.snap_line_color = None;
            ctx.preview.snap.snap_hud_label = None;

            // ── Time Lens ──
            let mut all_kf: Vec<f64> = if let Some(tl) = ctx.timeline {
                tl.root_actor_labels().iter().flat_map(|label| {
                    tl.get_track(label).map(animatix::timeline::collect_all_keyframe_times).unwrap_or_default()
                }).collect()
            } else { Vec::new() };
            all_kf.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            all_kf.dedup_by(|a, b| (*a - *b).abs() < 0.001);
            if let Some(new_time) = ctx.preview.time_lens.update_and_show(
                ui, ctx.preview.playback.current_time_s, ctx.preview.playback.duration_s, &all_kf,
            ) {
                ctx.commands.push_back(Command::ScrubTo(new_time));
            }

            let is_dragging = !matches!(ctx.drag_state, DragState::None);
            if ctx.handle_preview_drag(ui, preview_rect, &response) { return; }

            let pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos()).filter(|p| preview_rect.contains(*p));
            let scene_dimensions = ctx.scene_dimensions;
            let zoom = ctx.preview.viewport.preview_zoom;
            let pan = ctx.preview.viewport.preview_pan;
            let screen_to_scene = move |screen: egui::Pos2| preview_screen_to_scene(scene_dimensions, preview_rect, screen, zoom, pan);

            if !ctx.selection.context_menu_open {
                selection::update_hover(ctx.selection, ctx.hit_regions, pointer_pos, screen_to_scene, is_dragging);
            } else {
                ctx.selection.hovered_actor = None;
            }

            ctx.handle_preview_selection(ui, preview_rect, &response);
            ctx.render_preview_cursor_feedback(ui, preview_rect);
            ctx.render_preview_overlays(ui, preview_rect);
            ctx.render_preview_content(ui, preview_rect);

            // ── Scene bounds overlay ──
            if ctx.preview.overlay.show_scene_bounds {
                let bounds_rect = preview::scene_to_screen(
                    kurbo::Point::new(0.0, 0.0), preview_rect, ctx.scene_dimensions, preview_rect.size(),
                    ctx.preview.viewport.preview_zoom, ctx.preview.viewport.preview_pan,
                );
                let bounds_br = preview::scene_to_screen(
                    kurbo::Point::new(ctx.scene_dimensions.width as f64, ctx.scene_dimensions.height as f64),
                    preview_rect, ctx.scene_dimensions, preview_rect.size(),
                    ctx.preview.viewport.preview_zoom, ctx.preview.viewport.preview_pan,
                );
                let bounds_screen = egui::Rect::from_min_max(bounds_rect, bounds_br).intersect(preview_rect);
                if bounds_screen.is_positive() {
                    ui.painter().rect_stroke(bounds_screen, 0.0, egui::Stroke::new(STROKE_WIDTH, BORDER_HOVER), egui::StrokeKind::Inside);
                }
            }

            // ── Actor labels overlay ──
            if ctx.preview.overlay.show_actor_labels {
                for (label, bounds) in ctx.hit_regions {
                    let center = preview::scene_to_screen(
                        kurbo::Point::new((bounds.x0 + bounds.x1) / 2.0, bounds.y0 - 4.0),
                        preview_rect, ctx.scene_dimensions, preview_rect.size(),
                        ctx.preview.viewport.preview_zoom, ctx.preview.viewport.preview_pan,
                    );
                    ui.painter().text(center, egui::Align2::CENTER_BOTTOM, label,
                        egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional), TEXT_MUTED);
                }
            }

            // Draw grid overlay
            if ctx.preview.overlay.show_grid {
                preview::grid::draw_grid(ui.painter(), ctx.scene_dimensions, preview_rect,
                    ctx.preview.viewport.preview_zoom, ctx.preview.viewport.preview_pan, ctx.preview.overlay.grid_size);
            }

            // ── Draw snap indicator lines ──
            if let Some(color) = ctx.preview.snap.snap_line_color {
                for &sy in &ctx.preview.snap.snap_lines_h {
                    let screen_pt = ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(0.0, sy as f64));
                    if screen_pt.y >= preview_rect.min.y && screen_pt.y <= preview_rect.max.y {
                        ui.painter().line_segment([egui::pos2(preview_rect.min.x, screen_pt.y), egui::pos2(preview_rect.max.x, screen_pt.y)], egui::Stroke::new(STROKE_WIDTH, color));
                    }
                }
                for &sx in &ctx.preview.snap.snap_lines_v {
                    let screen_pt = ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(sx as f64, 0.0));
                    if screen_pt.x >= preview_rect.min.x && screen_pt.x <= preview_rect.max.x {
                        ui.painter().line_segment([egui::pos2(screen_pt.x, preview_rect.min.y), egui::pos2(screen_pt.x, preview_rect.max.y)], egui::Stroke::new(STROKE_WIDTH, color));
                    }
                }
            }

            ctx.render_preview_selection_overlay(ui, preview_rect, is_dragging);

            // Floating property cards for selected actors
            if !is_dragging && ctx.selected_actors.len() == 1 {
                if let Some(actor) = ctx.selected_actors.iter().next() {
                    if let Some(props) = ctx.get_actor_props(actor) {
                        let screen_pos = preview::scene_to_screen(
                            kurbo::Point::new(props.position[0] as f64, props.position[1] as f64),
                            preview_rect, ctx.scene_dimensions, preview_rect.size(),
                            ctx.preview.viewport.preview_zoom, ctx.preview.viewport.preview_pan,
                        );
                        preview::property_popup::show_property_popup(
                            ui, actor, &props, screen_pos, ctx.commands, is_dragging,
                            ctx.timeline, ctx.preview.playback.current_time_s,
                        );
                    }
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Rect;

    #[test]
    fn test_clamp_pan_center() {
        // Scene 1920×1080, preview 960×540, zoom=1 → full scene visible, must pan to center (960, 540)
        let scene = SceneDimensions { width: 1920, height: 1080 };
        let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(960.0, 540.0));
        let result = PreviewContext::clamp_pan_value(Vec2::new(500.0, 300.0), preview, scene, 1.0);
        // Both axes are clamped to the exact center since all scene is visible
        assert_eq!(result.x, 960.0);
        assert_eq!(result.y, 540.0);
    }

    #[test]
    fn test_clamp_pan_beyond_bounds() {
        // Scene 1920×1080, preview 960×540, zoom=2 → half scene visible
        // visible_w = min(960*1.0, 1920) = 960, half_w = 480
        // range: [480, 1920-480] = [480, 1440]
        let scene = SceneDimensions { width: 1920, height: 1080 };
        let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(960.0, 540.0));
        // Pan way beyond right edge
        let result = PreviewContext::clamp_pan_value(Vec2::new(2000.0, 1000.0), preview, scene, 2.0);
        assert_eq!(result.x, 1440.0);
        assert_eq!(result.y, 810.0);
        // Pan way beyond left/top edge
        let result = PreviewContext::clamp_pan_value(Vec2::new(-100.0, -100.0), preview, scene, 2.0);
        assert_eq!(result.x, 480.0);
        assert_eq!(result.y, 270.0);
    }

    #[test]
    fn test_clamp_pan_zero_size_preview() {
        // Minimal preview rect (1×1 minimum via scale logic)
        let scene = SceneDimensions { width: 1920, height: 1080 };
        let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1.0, 1.0));
        let result = PreviewContext::clamp_pan_value(Vec2::new(500.0, 300.0), preview, scene, 1.0);
        // visible_w = min(1 * huge_scale, 1920) = 1920 → half_w = 960 → range [960, 960]
        assert_eq!(result.x, 960.0);
        assert_eq!(result.y, 540.0);
    }

    #[test]
    fn test_clamp_pan_extreme_zoom() {
        // Scene 1920×1080, preview 960×540, zoom=10 → tiny viewport in scene space
        // scale = (base_scale=2.0) / 10 = 0.2
        // visible_w = min(960 * 0.2, 1920) = min(192, 1920) = 192, half_w = 96
        // range: [96, 1920-96] = [96, 1824]
        let scene = SceneDimensions { width: 1920, height: 1080 };
        let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(960.0, 540.0));
        let result = PreviewContext::clamp_pan_value(Vec2::new(100.0, 100.0), preview, scene, 10.0);
        assert_eq!(result.x, 100.0);  // 100 is in [96, 1824]
        assert_eq!(result.y, 100.0);  // 100 is in [54, 1026]
    }

    #[test]
    fn test_clamp_pan_scene_smaller_than_preview() {
        // Scene 100×50, preview 500×500, zoom=1 → scene is tiny compared to preview
        // px_per_scene_x = 500/100 = 5, px_per_scene_y = 500/50 = 10
        // px_per_scene = min(5, 10) = 5
        // base_scale = 1/5 = 0.2
        // scale = 0.2 / 1 = 0.2
        // visible_w = min(500*0.2, 100) = min(100, 100) = 100, half_w = 50
        // range: [50, 100-50] = [50, 50]
        // visible_h = min(500*0.2, 50) = min(100, 50) = 50, half_h = 25
        // range: [25, 50-25] = [25, 25]
        let scene = SceneDimensions { width: 100, height: 50 };
        let preview = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(500.0, 500.0));
        let result = PreviewContext::clamp_pan_value(Vec2::new(0.0, 0.0), preview, scene, 1.0);
        assert_eq!(result.x, 50.0);
        assert_eq!(result.y, 25.0);
    }
}