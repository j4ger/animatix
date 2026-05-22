use super::*;

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

impl WorkspaceViewer<'_> {
    fn preview_transform(&self, preview_rect: egui::Rect) -> preview::PreviewTransform {
        preview::PreviewTransform::new(
            self.scene_dimensions,
            preview_rect,
            self.preview.preview_zoom,
            self.preview.preview_pan,
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
        let drag_started = response.drag_started()
            || (!is_dragging && ui.input(|i| i.pointer.primary_pressed()));
        let hit_radius = preview::HANDLE_HIT_RADIUS * ui.ctx().pixels_per_point();

        if drag_started {
            if let (Some(actor), Some(mouse)) = (self.selected_actors.iter().next().cloned(), raw_pointer_pos) {
                let scene = self.preview_screen_to_scene(preview_rect, mouse);
                let props = self.get_actor_props(&actor);

                if let Some(ref p) = props {
                    let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                    let vertex_points = self.timeline
                        .and_then(|t| t.get_track(&actor))
                        .and_then(|tr| tr.points.as_ref().map(|pt| pt.evaluate(time_ms)))
                        .filter(|pts| !pts.is_empty());

                    // ── Tool-mode overrides ──
                    match *self.tool_mode {
                        preview::ToolMode::Move => {
                            // Skip all handles; fall through to body drag
                        }
                        preview::ToolMode::Vertex => {
                            if let Some(ref points) = vertex_points {
                                // Find nearest vertex even if not directly hitting it
                                if let Some(vidx) = preview::hit_test_vertex(mouse, p, points, preview_rect, self.scene_dimensions, preview_rect.size(), hit_radius * 2.0, self.preview.preview_zoom, self.preview.preview_pan) {
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
                            // Find nearest handle even if not directly hitting it
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
                            // Standard auto-detect priority
                            if let Some(ref points) = vertex_points {
                                if let Some(vidx) = preview::hit_test_vertex(mouse, p, points, preview_rect, self.scene_dimensions, preview_rect.size(), hit_radius, self.preview.preview_zoom, self.preview.preview_pan) {
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

                            // Pivot hit-test (lowest priority in Select mode)
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
                            // Shift-drag: break out of layout and start Move drag
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
                                start_mouse: scene,
                                layout_type,
                            };
                            return true;
                        }
                        return true;
                    }

                    let alt = ui.input(|i| i.modifiers.alt);
                    if alt {
                        // Alt-drag: duplicate the primary actor
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
                // Drag started on empty canvas — begin marquee selection
                self.selection.marquee_start = Some(mouse);
                self.selection.marquee_current = Some(mouse);
            }
        }

        if !is_dragging {
            // Update marquee selection rectangle during empty-canvas drag
            if let (Some(mouse), Some(_start)) = (raw_pointer_pos, self.selection.marquee_start) {
                self.selection.marquee_current = Some(mouse);
            }
        } else if let Some(mouse) = raw_pointer_pos {
            let scene = self.preview_screen_to_scene(preview_rect, mouse);
            let shift = ui.input(|i| i.modifiers.shift);

            match self.drag_state.clone() {
                DragState::Move {
                    primary: _,
                    actors,
                    start_scene,
                } => {
                    let raw_dx = (scene.x - start_scene.x) as f32;
                    let raw_dy = (scene.y - start_scene.y) as f32;
                    // Shift constrains movement to horizontal or vertical based on primary drag direction
                    let (dx, dy) = if shift {
                        if raw_dx.abs() > raw_dy.abs() {
                            (raw_dx, 0.0)
                        } else {
                            (0.0, raw_dy)
                        }
                    } else {
                        (raw_dx, raw_dy)
                    };

                    // Snap is disabled when Alt is held during drag
                    let snap_enabled = self.preview.snap_enabled && !ui.input(|i| i.modifiers.alt);
                    let threshold = self.preview.snap_threshold;

                    let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                    for (actor, start_position) in actors {
                        let mut nx = start_position[0] + dx;
                        let mut ny = start_position[1] + dy;

                        if *self.grid_enabled {
                            let grid = *self.grid_size;
                            nx = (nx / grid).round() * grid;
                            ny = (ny / grid).round() * grid;
                        }

                        // ── Smart snap (skipped when disabled or Alt held) ──
                        let mut snapped_guide_h = false;
                        let mut snapped_guide_v = false;
                        let mut snapped_actor_h = false;
                        let mut snapped_actor_v = false;
                        let mut snapped_container = false;
                        let mut snapped_keyframe = false;
                        let mut snap_hud_text: Option<String> = None;

                        if snap_enabled {
                            // ── Guide snap ──
                            for &guide_y in &self.preview.horizontal_guides {
                                if (ny - guide_y).abs() < threshold {
                                    ny = guide_y;
                                    snapped_guide_h = true;
                                    snap_hud_text = Some(format!("Guide y={}", guide_y as i32));
                                }
                            }
                            for &guide_x in &self.preview.vertical_guides {
                                if (nx - guide_x).abs() < threshold {
                                    nx = guide_x;
                                    snapped_guide_v = true;
                                    snap_hud_text = Some(format!("Guide x={}", guide_x as i32));
                                }
                            }

                            // ── Actor-to-actor snap ──
                            let dragged_props = self.get_actor_props(&actor);
                            let half_w = dragged_props.as_ref().map(|p| p.size[0] / 2.0).unwrap_or(0.0);
                            let half_h = dragged_props.as_ref().map(|p| p.size[1] / 2.0).unwrap_or(0.0);
                            let dragged_x_edges = [nx - half_w, nx, nx + half_w];
                            let dragged_y_edges = [ny - half_h, ny, ny + half_h];
                            let edge_labels = ["left", "center", "right"];
                            let edge_labels_y = ["top", "center", "bottom"];

                            for (other_label, other_bounds) in self.hit_regions.iter() {
                                if other_label == &actor { continue; }
                                let other_x_edges = [
                                    other_bounds.x0 as f32,
                                    (other_bounds.x0 + other_bounds.x1) as f32 / 2.0,
                                    other_bounds.x1 as f32,
                                ];
                                let other_y_edges = [
                                    other_bounds.y0 as f32,
                                    (other_bounds.y0 + other_bounds.y1) as f32 / 2.0,
                                    other_bounds.y1 as f32,
                                ];

                                // X snap
                                for (_di, &de) in dragged_x_edges.iter().enumerate() {
                                    for (oi, &oe) in other_x_edges.iter().enumerate() {
                                        let candidate_nx: f32 = nx + (oe - de);
                                        if (candidate_nx - nx).abs() < threshold && (candidate_nx - nx).abs() > 0.001 {
                                            nx = candidate_nx;
                                            snapped_actor_v = true;
                                            snap_hud_text = Some(format!("{} {}", other_label, edge_labels[oi]));
                                        }
                                    }
                                }
                                // Y snap
                                for (_di, &de) in dragged_y_edges.iter().enumerate() {
                                    for (oi, &oe) in other_y_edges.iter().enumerate() {
                                        let candidate_ny: f32 = ny + (oe - de);
                                        if (candidate_ny - ny).abs() < threshold && (candidate_ny - ny).abs() > 0.001 {
                                            ny = candidate_ny;
                                            snapped_actor_h = true;
                                            snap_hud_text = Some(format!("{} {}", other_label, edge_labels_y[oi]));
                                        }
                                    }
                                }
                            }

                            // ── Layout container alignment snap ──
                            if let Some((container, _layout_type, _)) = self.find_layout_container(&actor) {
                                if let Some(container_props) = self.get_actor_props(&container) {
                                    // Snap to container center X
                                    if (nx - container_props.position[0]).abs() < threshold {
                                        nx = container_props.position[0];
                                        snapped_container = true;
                                        snap_hud_text = Some(format!("{} center X", container));
                                    }
                                    // Snap to container center Y
                                    if (ny - container_props.position[1]).abs() < threshold {
                                        ny = container_props.position[1];
                                        snapped_container = true;
                                        snap_hud_text = Some(format!("{} center Y", container));
                                    }
                                    // Snap to container left/right edges
                                    let c_hw = container_props.size[0] / 2.0;
                                    let c_left = container_props.position[0] - c_hw;
                                    let c_right = container_props.position[0] + c_hw;
                                    if (nx - c_left).abs() < threshold {
                                        nx = c_left;
                                        snapped_container = true;
                                        snap_hud_text = Some(format!("{} left", container));
                                    }
                                    if (nx - c_right).abs() < threshold {
                                        nx = c_right;
                                        snapped_container = true;
                                        snap_hud_text = Some(format!("{} right", container));
                                    }
                                    // Snap to container top/bottom edges
                                    let c_hh = container_props.size[1] / 2.0;
                                    let c_top = container_props.position[1] - c_hh;
                                    let c_bottom = container_props.position[1] + c_hh;
                                    if (ny - c_top).abs() < threshold {
                                        ny = c_top;
                                        snapped_container = true;
                                        snap_hud_text = Some(format!("{} top", container));
                                    }
                                    if (ny - c_bottom).abs() < threshold {
                                        ny = c_bottom;
                                        snapped_container = true;
                                        snap_hud_text = Some(format!("{} bottom", container));
                                    }
                                }
                            }

                            // ── Previous keyframe position snap ──
                            if let Some(track) = self.timeline.and_then(|t| t.get_track(&actor)) {
                                if let Some(ref pos_track) = track.position {
                                    // Find the nearest keyframe time before current time
                                    let prev_kf_time = pos_track.keyframes
                                        .range(..time_ms)
                                        .next_back()
                                        .map(|(&t, _)| t);
                                    if let Some(kf_ms) = prev_kf_time {
                                        if let Some(kf_props) = self.get_actor_props_at_time(&actor, kf_ms) {
                                            // Snap X to keyframe X
                                            if (nx - kf_props.position[0]).abs() < threshold {
                                                nx = kf_props.position[0];
                                                snapped_keyframe = true;
                                                snap_hud_text = Some(format!("prev keyframe ({:.2}s)", kf_ms as f64 / 1000.0));
                                            }
                                            // Snap Y to keyframe Y
                                            if (ny - kf_props.position[1]).abs() < threshold {
                                                ny = kf_props.position[1];
                                                snapped_keyframe = true;
                                                snap_hud_text = Some(format!("prev keyframe ({:.2}s)", kf_ms as f64 / 1000.0));
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ── Record snap lines for rendering ──
                        if snapped_guide_h || snapped_actor_h || snapped_container || snapped_keyframe {
                            self.preview.snap_lines_h.push(ny);
                        }
                        if snapped_guide_v || snapped_actor_v || snapped_container || snapped_keyframe {
                            self.preview.snap_lines_v.push(nx);
                        }
                        if snapped_guide_h || snapped_guide_v || snapped_actor_h || snapped_actor_v || snapped_container || snapped_keyframe {
                            // Priority: guide > keyframe > container > actor
                            let use_guide_color = snapped_guide_h || snapped_guide_v;
                            let use_keyframe_color = snapped_keyframe;
                            self.preview.snap_line_color = Some(
                                if use_guide_color { AMBER }
                                else if use_keyframe_color { ACCENT_CYAN }
                                else if snapped_container { ACCENT_BLUE }
                                else { GREEN }
                            );
                            self.preview.snap_hud_label = snap_hud_text;
                        }

                        let binding = self
                            .timeline
                            .and_then(|t| t.get_track(&actor))
                            .map(|tr| tr.position_binding.get(time_ms, PositionBinding::Absolute))
                            .unwrap_or(PositionBinding::Absolute);

                        match binding {
                            PositionBinding::SceneAnchor { anchor, .. } => {
                                let anchor_pt = animatix::timeline::scene_anchor_point(anchor, self.scene_dimensions);
                                let new_offset = [nx - anchor_pt.x as f32, ny - anchor_pt.y as f32];
                                self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                    actor,
                                    property: "offset".into(),
                                    value: PropertyValue::Vec2(new_offset),
                                    create_keyframe: self.keyframe_mode,
                                }));
                            }
                            PositionBinding::ScenePercent { .. } => {
                                let w = self.scene_dimensions.width.max(1) as f32;
                                let h = self.scene_dimensions.height.max(1) as f32;
                                self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                    actor,
                                    property: "at".into(),
                                    value: PropertyValue::Vec2([nx / w, ny / h]),
                                    create_keyframe: self.keyframe_mode,
                                }));
                            }
                            _ => {
                                self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                    actor,
                                    property: "position".into(),
                                    value: PropertyValue::Vec2([nx, ny]),
                                    create_keyframe: self.keyframe_mode,
                                }));
                            }
                        }
                    }
                }
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
                        0 => [-1.0_f32, -1.0],
                        1 => [1.0, -1.0],
                        2 => [1.0, 1.0],
                        3 => [-1.0, 1.0],
                        4 => [0.0, -1.0],
                        5 => [1.0, 0.0],
                        6 => [0.0, 1.0],
                        7 => [-1.0, 0.0],
                        _ => [1.0, 1.0],
                    };

                    let min_size = 10.0;
                    let mut new_w = start_size[0];
                    let mut new_h = start_size[1];

                    if sign[0] != 0.0 {
                        new_w = (start_size[0] + sign[0] * dx_local).max(min_size);
                    }
                    if sign[1] != 0.0 {
                        new_h = (start_size[1] + sign[1] * dy_local).max(min_size);
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
                        new_w = (start_size[0] * s).max(min_size);
                        new_h = (start_size[1] * s).max(min_size);
                    }

                    let cos_rot = start_rotation.cos();
                    let sin_rot = start_rotation.sin();
                    let old_anchor_local = [anchor_local[0], anchor_local[1]];
                    let new_anchor_local = [
                        old_anchor_local[0] * new_w / start_size[0].max(1.0),
                        old_anchor_local[1] * new_h / start_size[1].max(1.0),
                    ];

                    let anchor_world_x = start_position[0]
                        + old_anchor_local[0] * cos_rot
                        - old_anchor_local[1] * sin_rot;
                    let anchor_world_y = start_position[1]
                        + old_anchor_local[0] * sin_rot
                        + old_anchor_local[1] * cos_rot;

                    let new_pos_x = anchor_world_x
                        - new_anchor_local[0] * cos_rot
                        + new_anchor_local[1] * sin_rot;
                    let new_pos_y = anchor_world_y
                        - new_anchor_local[0] * sin_rot
                        - new_anchor_local[1] * cos_rot;

                    if resize_mode == preview::ResizeMode::Scale {
                        let ratio = new_w / start_size[0].max(1.0);
                        let new_scale = (start_scale * ratio).max(0.01);
                        self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                            actor: actor.clone(),
                            property: "scale".into(),
                            value: PropertyValue::Float(new_scale),
                            create_keyframe: self.keyframe_mode,
                        }));
                    } else {
                        self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                            actor: actor.clone(),
                            property: "size".into(),
                            value: PropertyValue::Vec2([new_w, new_h]),
                            create_keyframe: self.keyframe_mode,
                        }));
                    }

                    let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                    let binding = self
                        .timeline
                        .and_then(|t| t.get_track(&actor))
                        .map(|tr| tr.position_binding.get(time_ms, PositionBinding::Absolute))
                        .unwrap_or(PositionBinding::Absolute);

                    match binding {
                        PositionBinding::SceneAnchor { anchor, .. } => {
                            let anchor_pt = animatix::timeline::scene_anchor_point(anchor, self.scene_dimensions);
                            let new_offset = [new_pos_x - anchor_pt.x as f32, new_pos_y - anchor_pt.y as f32];
                            self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                actor,
                                property: "offset".into(),
                                value: PropertyValue::Vec2(new_offset),
                                create_keyframe: self.keyframe_mode,
                            }));
                        }
                        PositionBinding::ScenePercent { .. } => {
                            let w = self.scene_dimensions.width.max(1) as f32;
                            let h = self.scene_dimensions.height.max(1) as f32;
                            self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                actor,
                                property: "at".into(),
                                value: PropertyValue::Vec2([new_pos_x / w, new_pos_y / h]),
                                create_keyframe: self.keyframe_mode,
                            }));
                        }
                        _ => {
                            self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                actor,
                                property: "position".into(),
                                value: PropertyValue::Vec2([new_pos_x, new_pos_y]),
                                create_keyframe: self.keyframe_mode,
                            }));
                        }
                    }
                }
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
                        let step = self.rotation_snap_degrees.to_radians();
                        new_rot = (new_rot / step).round() * step;
                    }
                    self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                        actor,
                        property: "rotation".into(),
                        value: PropertyValue::Float(new_rot),
                        create_keyframe: self.keyframe_mode,
                    }));
                }
                DragState::Reorder {
                    actor,
                    container,
                    source_index: _,
                    target_index: _,
                    start_mouse: _,
                    layout_type,
                } => {
                    let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                    if let Some(timeline) = self.timeline {
                        let order = timeline.get_child_order(&container, time_ms);
                        let siblings: Vec<String> = order.into_iter().filter(|l| l != &actor).collect();
                        let positions: Vec<f32> = siblings
                            .iter()
                            .map(|label| {
                                self.hit_regions
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
                                        self.get_actor_props(label).map(|p| {
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
                        if let DragState::Reorder { target_index, .. } = &mut *self.drag_state {
                            *target_index = insert_at;
                        }
                    }
                }
                DragState::EditVertices {
                    actor,
                    vertex,
                    start_points,
                    start_scene,
                } => {
                    let dx = (scene.x - start_scene.x) as f32;
                    let dy = (scene.y - start_scene.y) as f32;

                    let mut new_points = start_points.clone();
                    if let Some(p) = self.get_actor_props(&actor) {
                        // Inverse-transform the world delta back to local space
                        let cos = (-p.rotation).cos();
                        let sin = (-p.rotation).sin();
                        let local_dx = dx * cos - dy * sin;
                        let local_dy = dx * sin + dy * cos;

                        if let Some(pt) = new_points.get_mut(vertex) {
                            pt[0] += local_dx;
                            pt[1] += local_dy;
                        }
                    }

                    self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                        actor,
                        property: "points".into(),
                        value: PropertyValue::PointList(new_points),
                        create_keyframe: self.keyframe_mode,
                    }));
                }
                DragState::MovePivot {
                    actor,
                    start_offset,
                    start_scene,
                } => {
                    let dx = (scene.x - start_scene.x) as f32;
                    let dy = (scene.y - start_scene.y) as f32;

                    if let Some(p) = self.get_actor_props(&actor) {
                        // Inverse-transform the world delta back to local space
                        let cos = (-p.rotation).cos();
                        let sin = (-p.rotation).sin();
                        let local_dx = dx * cos - dy * sin;
                        let local_dy = dx * sin + dy * cos;

                        let new_offset = [start_offset[0] + local_dx, start_offset[1] + local_dy];
                        self.pivot_offsets.insert(actor, new_offset);
                    }
                }
                DragState::None => {}
            }
        }

        let pointer_released = ui.input(|i| i.pointer.any_released());
        if is_dragging
            && (response.drag_stopped() || pointer_released || (!ui.input(|i| i.pointer.any_down()) && is_dragging))
        {
            if let DragState::Reorder {
                actor,
                container,
                source_index,
                target_index,
                ..
            } = self.drag_state.clone()
            {
                if source_index != target_index {
                    let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                    if let Some(timeline) = self.timeline {
                        let mut new_order = timeline.get_child_order(&container, time_ms);
                        if let Some(pos) = new_order.iter().position(|label| label == &actor) {
                            let item = new_order.remove(pos);
                            let insert_at = target_index.min(new_order.len());
                            new_order.insert(insert_at, item);
                            self.commands.push_back(Command::PropertyEdit(PropertyEdit {
                                actor: container,
                                property: "child_order".into(),
                                value: PropertyValue::StringList(new_order),
                                create_keyframe: self.keyframe_mode,
                            }));
                        }
                    }
                }
            }
            self.commands.push_back(Command::DragEnded);
        }

        // Marquee selection end
        if pointer_released && self.selection.marquee_start.is_some() {
            if let (Some(start), Some(current)) = (self.selection.marquee_start, self.selection.marquee_current) {
                let marquee_rect = egui::Rect::from_two_pos(start, current);
                let multi = ui.input(|i| i.modifiers.shift || i.modifiers.ctrl || i.modifiers.command);
                if !multi {
                    self.selected_actors.clear();
                }
                for (label, bounds) in self.hit_regions {
                    let center = egui::pos2(
                        ((bounds.x0 + bounds.x1) / 2.0) as f32,
                        ((bounds.y0 + bounds.y1) / 2.0) as f32,
                    );
                    if marquee_rect.contains(center) {
                        if multi && self.selected_actors.contains(label) {
                            self.selected_actors.remove(label);
                        } else {
                            self.selected_actors.insert(label.clone());
                        }
                    }
                }
            }
            self.selection.marquee_start = None;
            self.selection.marquee_current = None;
        }

        false
    }

    /// Handle click-to-select in the preview.
    fn handle_preview_selection(
        &mut self,
        ui: &mut egui::Ui,
        _preview_rect: egui::Rect,
        response: &egui::Response,
    ) {
        let is_dragging = !matches!(self.drag_state, DragState::None);

        if response.secondary_clicked() && !is_dragging {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let scene_dimensions = self.scene_dimensions;
                let zoom = self.preview.preview_zoom;
                let pan = self.preview.preview_pan;
                selection::handle_right_click(
                    self.selection,
                    self.hit_regions,
                    click_pos,
                    move |screen| preview_screen_to_scene(scene_dimensions, _preview_rect, screen, zoom, pan),
                );
            }
        }

        let mut menu_item_clicked = false;
        if self.selection.context_menu_open {
            let (selected, close, _rect) = selection::draw_context_menu(
                ui,
                self.selection,
                self.selected_actors,
            );
            menu_item_clicked = close;
            if let Some(actor) = selected {
                self.selected_actors.clear();
                self.selected_actors.insert(actor);
            }
            if close {
                self.selection.context_menu_open = false;
            }
        }

        let mut suppress_click = false;
        if self.selection.context_menu_open && !menu_item_clicked {
            if ui.input(|i| i.pointer.primary_clicked()) {
                self.selection.context_menu_open = false;
                suppress_click = true;
                self.selected_actors.clear();
            }
        }

        if response.clicked() && !is_dragging && !self.selection.context_menu_open && !suppress_click {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let scene_dimensions = self.scene_dimensions;
                let zoom = self.preview.preview_zoom;
                let pan = self.preview.preview_pan;
                let modifiers = ui.ctx().input(|i| i.modifiers);
                selection::handle_click(
                    self.selection,
                    self.selected_actors,
                    self.hit_regions,
                    click_pos,
                    move |screen| preview_screen_to_scene(scene_dimensions, _preview_rect, screen, zoom, pan),
                    &modifiers,
                );
                // During a transition, if the clicked actor is not in the active scene,
                // switch to the scene that contains it
                if let Some(comp) = self.composition {
                    if let Some(actor) = self.selected_actors.iter().next().cloned() {
                        let active_has_actor = self.active_scene.as_ref().is_some_and(|scene| {
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

    /// Render cursor feedback for the preview.
    fn render_preview_cursor_feedback(&self, ui: &egui::Ui, preview_rect: egui::Rect) {
        let is_dragging = !matches!(self.drag_state, DragState::None);
        let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());
        let hit_radius = preview::HANDLE_HIT_RADIUS * ui.ctx().pixels_per_point();

        if !is_dragging && !self.selection.context_menu_open {
            if let Some(mouse) = raw_pointer_pos {
                let scene = self.preview_screen_to_scene(preview_rect, mouse);

                let over_handle = self.selected_actors.iter().next().and_then(|a| {
                    let props = self.get_actor_props(a)?;
                    // Check pivot first (higher priority than scale/rotate for feedback)
                    let pivot_world_pt = preview::pivot_world(&props);
                    let pivot_screen = self.preview_scene_to_screen(
                        preview_rect,
                        kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64),
                    );
                    if preview::hit_test_pivot(mouse, pivot_screen, hit_radius) {
                        return Some(9usize);
                    }
                    let handle_world = preview::world_handle_positions(&props);
                    let handle_screen: [Pos2; 8] =
                        std::array::from_fn(|i| self.preview_scene_to_screen(preview_rect, handle_world[i]));
                    if let Some(idx) = preview::hit_test_handle(mouse, &handle_screen, hit_radius) {
                        Some(idx)
                    } else {
                        let rot_world = preview::rotation_handle_world(&props);
                        let rot_screen = self.preview_scene_to_screen(preview_rect, rot_world);
                        if preview::hit_test_rotation_handle(mouse, rot_screen, hit_radius) {
                            Some(8usize)
                        } else {
                            None
                        }
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
                    egui::Tooltip::always_open(
                        ui.ctx().clone(),
                        ui.layer_id(),
                        egui::Id::new("handle_tooltip"),
                        egui::PopupAnchor::Pointer,
                    )
                    .show(|ui| {
                        ui.label(egui::RichText::new(tooltip).size(crate::app::theme::FONT_SIZE_S));
                    });
                } else {
                    let is_over_selected = self
                        .selected_actors
                        .iter()
                        .next()
                        .and_then(|a| {
                            self.hit_regions
                                .iter()
                                .find(|(l, _)| l == a)
                                .map(|(_, b)| b.contains(scene))
                        })
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

    /// Render hover highlight and cycle indicator overlays.
    fn render_preview_overlays(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
    ) {
        if self.selection.context_menu_open {
            return;
        }

        let pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos()).filter(|p| preview_rect.contains(*p));
        if let Some(hovered) = self.selection.hovered_actor.as_ref() {
            if !self.selected_actors.contains(hovered) {
                if let Some(hover_rect) = preview::selection_screen_rect(
                    &HashSet::from([hovered.clone()]),
                    self.hit_regions,
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                    self.preview.preview_zoom,
                    self.preview.preview_pan,
                ) {
                    selection::draw_hover_highlight(ui.painter(), hovered, hover_rect);
                }
            }
        }

        // Smart snap guides during drag
        if let DragState::Move { primary, .. } | DragState::Scale { actor: primary, .. } = &self.drag_state {
            self.draw_snap_guides(ui, preview_rect, primary);
        }

        // ── Snap HUD label ──
        if let Some(ref label) = self.preview.snap_hud_label {
            if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                let hud_pos = mouse + Vec2::new(12.0, -24.0);
                let galley = ui.painter().layout_no_wrap(
                    label.clone(),
                    egui::FontId::proportional(FONT_SIZE_S),
                    GREEN,
                );
                let padding = Vec2::new(8.0, 4.0);
                let bg_size = galley.size() + padding * 2.0;
                let bg_rect = egui::Rect::from_min_size(hud_pos, bg_size);
                ui.painter().rect_filled(bg_rect, 3.0, snap_guide_label_bg());
                ui.painter().rect_stroke(
                    bg_rect,
                    3.0,
                    egui::Stroke::new(1.0, snap_guide_line()),
                    egui::StrokeKind::Outside,
                );
                ui.painter().galley(hud_pos + padding, galley, GREEN);
            }
        }

        if let Some(mouse) = pointer_pos {
            selection::draw_cycle_indicator(
                ui.painter(),
                mouse,
                self.selection.cycle_index,
                self.selection.click_candidates.len(),
            );
        }
    }

    /// Draw alignment guides when dragging an actor near other actors' edges or centers.
    fn draw_snap_guides(&self, ui: &mut egui::Ui, preview_rect: egui::Rect, primary: &str) {
        let primary_props = self.get_actor_props(primary);
        let primary_rect = if let Some(p) = primary_props {
            let hw = p.size[0] / 2.0;
            let hh = p.size[1] / 2.0;
            let corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for corner in &corners {
                let world = preview::local_to_world(*corner, p.position, p.rotation);
                let screen = preview::scene_to_screen(
                    world, preview_rect, self.scene_dimensions, preview_rect.size(),
                    self.preview.preview_zoom, self.preview.preview_pan,
                );
                min_x = min_x.min(screen.x);
                min_y = min_y.min(screen.y);
                max_x = max_x.max(screen.x);
                max_y = max_y.max(screen.y);
            }
            egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y))
        } else {
            self.hit_regions.iter().find(|(l, _)| l == primary).map(|(_, bounds)| {
                let tl = preview::scene_to_screen(
                    kurbo::Point::new(bounds.x0, bounds.y0), preview_rect, self.scene_dimensions,
                    preview_rect.size(), self.preview.preview_zoom, self.preview.preview_pan,
                );
                let br = preview::scene_to_screen(
                    kurbo::Point::new(bounds.x1, bounds.y1), preview_rect, self.scene_dimensions,
                    preview_rect.size(), self.preview.preview_zoom, self.preview.preview_pan,
                );
                egui::Rect::from_min_max(tl, br)
            }).unwrap_or(preview_rect)
        };

        let threshold = 8.0; // pixels
        let guide_color = accent_subtle();
        let guide_stroke = Stroke::new(1.0, guide_color);

        for (label, bounds) in self.hit_regions {
            if label == primary || self.selected_actors.contains(label) {
                continue;
            }
            let tl = preview::scene_to_screen(
                kurbo::Point::new(bounds.x0, bounds.y0), preview_rect, self.scene_dimensions,
                preview_rect.size(), self.preview.preview_zoom, self.preview.preview_pan,
            );
            let br = preview::scene_to_screen(
                kurbo::Point::new(bounds.x1, bounds.y1), preview_rect, self.scene_dimensions,
                preview_rect.size(), self.preview.preview_zoom, self.preview.preview_pan,
            );
            let other_rect = egui::Rect::from_min_max(tl, br);

            // Check alignments
            let px = [primary_rect.min.x, primary_rect.max.x, primary_rect.center().x];
            let py = [primary_rect.min.y, primary_rect.max.y, primary_rect.center().y];
            let ox = [other_rect.min.x, other_rect.max.x, other_rect.center().x];
            let oy = [other_rect.min.y, other_rect.max.y, other_rect.center().y];

            for &px in &px {
                for &ox in &ox {
                    if (px - ox).abs() < threshold {
                        ui.painter().line_segment(
                            [egui::pos2(px, preview_rect.min.y), egui::pos2(px, preview_rect.max.y)],
                            guide_stroke,
                        );
                    }
                }
            }
            for &py in &py {
                for &oy in &oy {
                    if (py - oy).abs() < threshold {
                        ui.painter().line_segment(
                            [egui::pos2(preview_rect.min.x, py), egui::pos2(preview_rect.max.x, py)],
                            guide_stroke,
                        );
                    }
                }
            }
        }
    }

    /// Render the actual preview content (vello scene).
    fn render_preview_content(&self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        match self.preview_texture_id {
            Some(texture_id) => {
                let zoom = self.preview.preview_zoom;
                let pan = self.preview.preview_pan;
                let scene_w = self.scene_dimensions.width.max(1) as f32;
                let scene_h = self.scene_dimensions.height.max(1) as f32;

                if (zoom - 1.0).abs() > 0.001 || pan != Vec2::new(scene_w / 2.0, scene_h / 2.0) {
                    // Apply zoom/pan via UV coordinates
                    let half_inv_zx = 0.5 / zoom.max(0.01);
                    let half_inv_zy = 0.5 / zoom.max(0.01);
                    let uv_cx = (pan.x / scene_w).clamp(0.0, 1.0);
                    let uv_cy = (pan.y / scene_h).clamp(0.0, 1.0);
                    let uv_rect = egui::Rect::from_min_max(
                        egui::pos2((uv_cx - half_inv_zx).clamp(0.0, 1.0), (uv_cy - half_inv_zy).clamp(0.0, 1.0)),
                        egui::pos2((uv_cx + half_inv_zx).clamp(0.0, 1.0), (uv_cy + half_inv_zy).clamp(0.0, 1.0)),
                    );
                    let image = egui::Image::new((texture_id, preview_rect.size())).uv(uv_rect);
                    ui.put(preview_rect, image);
                } else {
                    ui.put(preview_rect, egui::Image::new((texture_id, preview_rect.size())));
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

    fn render_preview_selection_overlay(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        is_dragging: bool,
    ) {
        // Multi-selection: draw union bounding box with shared handles
        if self.selected_actors.len() > 1 {
            let mut screen_rects = Vec::new();
            for actor in self.selected_actors.iter() {
                if let Some(props) = self.get_actor_props(actor) {
                    let hw = props.size[0] / 2.0;
                    let hh = props.size[1] / 2.0;
                    let local_corners: [[f32; 2]; 4] = [
                        [-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh],
                    ];
                    let mut min_x = f32::INFINITY;
                    let mut min_y = f32::INFINITY;
                    let mut max_x = f32::NEG_INFINITY;
                    let mut max_y = f32::NEG_INFINITY;
                    for corner in &local_corners {
                        let world = preview::local_to_world(*corner, props.position, props.rotation);
                        let screen = preview::scene_to_screen(
                            world, preview_rect, self.scene_dimensions,
                            preview_rect.size(), self.preview.preview_zoom, self.preview.preview_pan,
                        );
                        min_x = min_x.min(screen.x);
                        min_y = min_y.min(screen.y);
                        max_x = max_x.max(screen.x);
                        max_y = max_y.max(screen.y);
                    }
                    screen_rects.push(egui::Rect::from_min_max(
                        egui::pos2(min_x, min_y), egui::pos2(max_x, max_y),
                    ));
                } else if let Some((_, bounds)) = self.hit_regions.iter().find(|(l, _)| l == actor) {
                    let top_left = preview::scene_to_screen(
                        kurbo::Point::new(bounds.x0, bounds.y0), preview_rect, self.scene_dimensions,
                        preview_rect.size(), self.preview.preview_zoom, self.preview.preview_pan,
                    );
                    let bottom_right = preview::scene_to_screen(
                        kurbo::Point::new(bounds.x1, bounds.y1), preview_rect, self.scene_dimensions,
                        preview_rect.size(), self.preview.preview_zoom, self.preview.preview_pan,
                    );
                    screen_rects.push(egui::Rect::from_min_max(top_left, bottom_right));
                }
            }
            preview::draw_multi_selection_overlay(
                ui.painter(), &screen_rects, is_dragging, ui.ctx().pixels_per_point(),
            );
            return;
        }

        // Single selection: draw per-actor overlay with handles
        for actor in self.selected_actors.iter() {
            let props = self.get_actor_props(actor);
            let fallback = self.hit_regions
                .iter()
                .find(|(l, _)| l == actor)
                .map(|(_, bounds)| {
                    let top_left = preview::scene_to_screen(
                        kurbo::Point::new(bounds.x0, bounds.y0),
                        preview_rect,
                        self.scene_dimensions,
                        preview_rect.size(),
                        self.preview.preview_zoom,
                        self.preview.preview_pan,
                    );
                    let bottom_right = preview::scene_to_screen(
                        kurbo::Point::new(bounds.x1, bounds.y1),
                        preview_rect,
                        self.scene_dimensions,
                        preview_rect.size(),
                        self.preview.preview_zoom,
                        self.preview.preview_pan,
                    );
                    egui::Rect::from_min_max(top_left, bottom_right)
                });
            preview::draw_selection_overlay(
                ui.painter(),
                props.as_ref(),
                fallback,
                is_dragging,
                preview_rect,
                self.scene_dimensions,
                preview_rect.size(),
                ui.ctx().pixels_per_point(),
                self.preview.preview_zoom,
                self.preview.preview_pan,
            );

            // Draw polygon vertex handles
            let time_ms = (self.preview.current_time_s * 1000.0) as u64;
            let points = self.timeline
                .and_then(|t| t.get_track(actor))
                .and_then(|tr| tr.points.as_ref().map(|pt| pt.evaluate(time_ms)))
                .filter(|pts| !pts.is_empty());
            if let (Some(ref p), Some(pts)) = (props, points) {
                let active_vertex = match &self.drag_state {
                    DragState::EditVertices { actor: drag_actor, vertex, .. } => {
                        if drag_actor == actor { Some(*vertex) } else { None }
                    }
                    _ => None,
                };
                preview::draw_vertex_handles(
                    ui.painter(),
                    p,
                    &pts,
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                    active_vertex,
                    ui.ctx().pixels_per_point(),
                    self.preview.preview_zoom,
                    self.preview.preview_pan,
                );
            }

            // Ghost Edit / Onion Skin: show outlines at prev/next keyframe times
            if !is_dragging {
                if let Some(timeline) = self.timeline {
                    let current_time_ms = (self.preview.current_time_s * 1000.0) as u64;
                    let keyframe_times = timeline.keyframe_times_s();
                    let mut prev_time_ms: Option<u64> = None;
                    let mut next_time_ms: Option<u64> = None;
                    for &time_s in &keyframe_times {
                        let time_ms = (time_s * 1000.0) as u64;
                        if time_ms < current_time_ms {
                            prev_time_ms = Some(time_ms);
                        } else if time_ms > current_time_ms && next_time_ms.is_none() {
                            next_time_ms = Some(time_ms);
                        }
                    }

                    // Draw prev keyframe ghost (green, 30% opacity)
                    if let Some(prev_ms) = prev_time_ms {
                        if let Some(prev_props) = self.get_actor_props_at_time(actor, prev_ms) {
                            let ghost_color = ghost_prev();
                            preview::draw_ghost_overlay(
                                ui.painter(), &prev_props, preview_rect, self.scene_dimensions,
                                preview_rect.size(), self.preview.preview_zoom, self.preview.preview_pan,
                                ghost_color,
                            );
                        }
                    }
                    // Draw next keyframe ghost (blue, 30% opacity)
                    if let Some(next_ms) = next_time_ms {
                        if let Some(next_props) = self.get_actor_props_at_time(actor, next_ms) {
                            let ghost_color = ghost_next();
                            preview::draw_ghost_overlay(
                                ui.painter(), &next_props, preview_rect, self.scene_dimensions,
                                preview_rect.size(), self.preview.preview_zoom, self.preview.preview_pan,
                                ghost_color,
                            );
                        }
                    }
                }
            }

            if let DragState::Reorder {
                actor: drag_actor,
                container,
                target_index,
                layout_type,
                ..
            } = self.drag_state.clone()
            {
                if &drag_actor == actor {
                    let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                    if let Some(timeline) = self.timeline {
                        let order = timeline.get_child_order(&container, time_ms);
                        let siblings: Vec<(String, [f32; 2])> = order
                            .into_iter()
                            .filter(|label| label != actor)
                            .filter_map(|label| self.get_actor_props(&label).map(|p| (label, p.position)))
                            .collect();
                        if let Some(props) = props.as_ref() {
                            preview::draw_reorder_overlay(
                                ui.painter(),
                                props,
                                target_index,
                                &siblings,
                                preview_rect,
                                self.scene_dimensions,
                                preview_rect.size(),
                                layout_type == animatix::timeline::LayoutType::Row,
                                self.preview.preview_zoom,
                                self.preview.preview_pan,
                            );
                        }
                    }
                }
            }
        }

        // Draw marquee selection rectangle
        if let (Some(start), Some(current)) = (self.selection.marquee_start, self.selection.marquee_current) {
            let marquee_rect = egui::Rect::from_two_pos(start, current);
            let fill = accent_faint();
            let stroke = Stroke::new(1.0, accent_subtle());
            ui.painter().rect_filled(marquee_rect, 0.0, fill);
            ui.painter().rect_stroke(marquee_rect, 0.0, stroke, egui::StrokeKind::Outside);
        }
    }

    pub(super) fn preview_ui(&mut self, ui: &mut egui::Ui) {
        const PLAYING_TEXT: Color32 = crate::app::theme::PLAYING_TEXT;

        panel_frame().show(ui, |ui| {
        ui.vertical(|ui| {
            let header_avail = ui.available_width();
            let header_h = ROW_S;
            let (header_rect, _) = ui.allocate_exact_size(Vec2::new(header_avail, header_h), egui::Sense::hover());
            let baseline_y = header_rect.center().y;

            ui.painter().text(
                egui::pos2(header_rect.min.x, baseline_y),
                egui::Align2::LEFT_CENTER,
                "Preview",
                egui::FontId::new(FONT_SIZE_L, egui::FontFamily::Proportional),
                TEXT_SECONDARY,
            );

            let (badge_label, badge_fill, badge_text) = if self.preview.is_playing {
                ("Playing", GREEN, PLAYING_TEXT)
            } else {
                ("Paused", BORDER, TEXT_SECONDARY)
            };
            let badge_w = badge_label.len() as f32 * 7.0 + 16.0;
            let badge_rect = egui::Rect::from_min_size(
                egui::pos2(header_rect.max.x - badge_w, header_rect.min.y + 2.0),
                Vec2::new(badge_w, header_h - 4.0),
            );
            ui.painter().rect_filled(badge_rect, RADIUS_L, badge_fill);
            ui.painter().text(
                badge_rect.center(),
                egui::Align2::CENTER_CENTER,
                badge_label,
                egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
                badge_text,
            );

            // Grid toggle button in header (right of badge)
            let grid_btn_rect = egui::Rect::from_min_size(
                egui::pos2(badge_rect.min.x - header_h - 4.0, header_rect.min.y + 2.0),
                Vec2::new(header_h, header_h - 4.0),
            );
            let grid_btn = ui.allocate_rect(grid_btn_rect, egui::Sense::click());
            let grid_icon = if *self.grid_enabled {
                egui_phosphor::regular::GRID_FOUR
            } else {
                egui_phosphor::regular::GRID_NINE
            };
            let grid_color = if *self.grid_enabled { ACCENT_BLUE } else { TEXT_MUTED };
            ui.painter().text(
                grid_btn_rect.center(),
                egui::Align2::CENTER_CENTER,
                grid_icon,
                egui::FontId::new(FONT_SIZE_L, egui::FontFamily::Proportional),
                grid_color,
            );
            if grid_btn.clicked() {
                *self.grid_enabled = !*self.grid_enabled;
            }

            // Reset zoom/pan button (left of grid toggle)
            let reset_btn_rect = egui::Rect::from_min_size(
                egui::pos2(grid_btn_rect.min.x - header_h - 4.0, header_rect.min.y + 2.0),
                Vec2::new(header_h, header_h - 4.0),
            );
            let reset_btn = ui.allocate_rect(reset_btn_rect, egui::Sense::click());
            let reset_color = if self.preview.preview_zoom != 1.0 || self.preview.preview_pan != Vec2::ZERO {
                ACCENT_BLUE
            } else {
                TEXT_MUTED
            };
            ui.painter().text(
                reset_btn_rect.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::ARROWS_OUT_CARDINAL,
                egui::FontId::new(FONT_SIZE_L, egui::FontFamily::Proportional),
                reset_color,
            );
            if reset_btn.clicked() {
                self.preview.preview_zoom = 1.0;
                self.preview.preview_pan = Vec2::ZERO;
                self.preview.status = "Zoom/Pan reset".to_string();
            }

            // Diff mode toggle (left of reset)
            let diff_btn_rect = egui::Rect::from_min_size(
                egui::pos2(reset_btn_rect.min.x - header_h - 4.0, header_rect.min.y + 2.0),
                Vec2::new(header_h, header_h - 4.0),
            );
            let diff_btn = ui.allocate_rect(diff_btn_rect, egui::Sense::click());
            let diff_color = if self.preview.diff_mode { ACCENT_BLUE } else { TEXT_MUTED };
            ui.painter().text(
                diff_btn_rect.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::COLUMNS,
                egui::FontId::new(FONT_SIZE_L, egui::FontFamily::Proportional),
                diff_color,
            );
            if diff_btn.clicked() {
                self.preview.diff_mode = !self.preview.diff_mode;
                if self.preview.diff_mode {
                    self.preview.status = "Diff mode: showing before/after".to_string();
                } else {
                    self.preview.diff_before_source = None;
                    self.preview.status = "Diff mode off".to_string();
                }
            }

            // Scene slices toggle (left of diff)
            let slices_btn_rect = egui::Rect::from_min_size(
                egui::pos2(diff_btn_rect.min.x - header_h - 4.0, header_rect.min.y + 2.0),
                Vec2::new(header_h, header_h - 4.0),
            );
            let slices_btn = ui.allocate_rect(slices_btn_rect, egui::Sense::click());
            let slices_color = if self.preview.scene_slices.enabled { ACCENT_BLUE } else { TEXT_MUTED };
            ui.painter().text(
                slices_btn_rect.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::SQUARE_SPLIT_HORIZONTAL,
                egui::FontId::new(FONT_SIZE_L, egui::FontFamily::Proportional),
                slices_color,
            );
            if slices_btn.clicked() {
                self.preview.scene_slices.toggle();
                self.preview.status = if self.preview.scene_slices.enabled {
                    "Scene slices enabled".to_string()
                } else {
                    "Scene slices disabled".to_string()
                };
            }

            ui.add_space(SPACE_S);

            // Scene slice tabs
            if self.preview.scene_slices.enabled {
                crate::app::preview::scene_slices::render_slice_tabs(ui, &mut self.preview.scene_slices);
                ui.add_space(SPACE_S);
            }

            let available = ui.available_size_before_wrap();
            let desired = fit_preview(
                self.scene_dimensions,
                Vec2::new(available.x.max(200.0), available.y.max(180.0)),
            );
            let (preview_rect, response) = ui.allocate_exact_size(desired, egui::Sense::click_and_drag());
            ui.painter().rect_stroke(
                preview_rect,
                RADIUS_L,
                Stroke::new(1.0, BORDER),
                egui::StrokeKind::Outside,
            );
            ui.painter().rect_filled(preview_rect, RADIUS_L, BG_BASE);

            // ── Rulers ──
            let ruler_bg = BG_PANEL;
            let ruler_tick_color = TEXT_MUTED;
            let ruler_text_color = TEXT_MUTED;
            let ruler_label_color = TEXT_SECONDARY;

            // Ruler rects around the preview
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
            let ruler_stroke = Stroke::new(1.0, BORDER);

            // Corner filler
            ui.painter().rect_filled(corner_rect, 0.0, ruler_bg);
            ui.painter().rect_stroke(corner_rect, 0.0, ruler_stroke, egui::StrokeKind::Outside);

            // Compute visible scene bounds
            let scene_tl = preview_screen_to_scene(
                self.scene_dimensions, preview_rect, preview_rect.left_top(),
                self.preview.preview_zoom, self.preview.preview_pan,
            );
            let scene_br = preview_screen_to_scene(
                self.scene_dimensions, preview_rect, preview_rect.right_bottom(),
                self.preview.preview_zoom, self.preview.preview_pan,
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
                    self.scene_dimensions, preview_rect,
                    kurbo::Point::new(tick_x as f64, scene_tl.y),
                    self.preview.preview_zoom, self.preview.preview_pan,
                );
                if screen_pt.x >= h_ruler_rect.min.x && screen_pt.x <= h_ruler_rect.max.x {
                    let rel_x = screen_pt.x - h_ruler_rect.min.x;
                    let is_major = (tick_x as i32) % (h_interval as i32 * 5) == 0;
                    let tick_h = if is_major { RULER_SIZE * 0.6 } else { RULER_SIZE * 0.3 };
                    ui.painter().line_segment(
                        [egui::pos2(h_ruler_rect.min.x + rel_x, h_ruler_rect.max.y),
                         egui::pos2(h_ruler_rect.min.x + rel_x, h_ruler_rect.max.y - tick_h)],
                        Stroke::new(1.0, if is_major { ruler_label_color } else { ruler_tick_color }),
                    );
                    if is_major {
                        let label = format!("{}", tick_x as i32);
                        ui.painter().text(
                            egui::pos2(h_ruler_rect.min.x + rel_x, h_ruler_rect.min.y + RULER_SIZE * 0.3),
                            egui::Align2::CENTER_CENTER,
                            label,
                            egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
                            ruler_text_color,
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
                    self.scene_dimensions, preview_rect,
                    kurbo::Point::new(scene_tl.x, tick_y as f64),
                    self.preview.preview_zoom, self.preview.preview_pan,
                );
                if screen_pt.y >= v_ruler_rect.min.y && screen_pt.y <= v_ruler_rect.max.y {
                    let rel_y = screen_pt.y - v_ruler_rect.min.y;
                    let is_major = (tick_y as i32) % (v_interval as i32 * 5) == 0;
                    let tick_w = if is_major { RULER_SIZE * 0.6 } else { RULER_SIZE * 0.3 };
                    ui.painter().line_segment(
                        [egui::pos2(v_ruler_rect.max.x, v_ruler_rect.min.y + rel_y),
                         egui::pos2(v_ruler_rect.max.x - tick_w, v_ruler_rect.min.y + rel_y)],
                        Stroke::new(1.0, if is_major { ruler_label_color } else { ruler_tick_color }),
                    );
                    if is_major {
                        let label = format!("{}", tick_y as i32);
                        ui.painter().text(
                            egui::pos2(v_ruler_rect.min.x + RULER_SIZE * 0.3, v_ruler_rect.min.y + rel_y),
                            egui::Align2::CENTER_CENTER,
                            label,
                            egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
                            ruler_text_color,
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

            // Track ruler drag: (is_vertical, start_scene_value)
            if h_ruler_resp.drag_started() {
                if let Some(mouse) = raw_pointer_pos {
                    let scene = self.preview_screen_to_scene(preview_rect, mouse);
                    ui.data_mut(|d| d.insert_temp(ruler_drag_id, Some((false, scene.y as f32))));
                }
            }
            if v_ruler_resp.drag_started() {
                if let Some(mouse) = raw_pointer_pos {
                    let scene = self.preview_screen_to_scene(preview_rect, mouse);
                    ui.data_mut(|d| d.insert_temp(ruler_drag_id, Some((true, scene.x as f32))));
                }
            }

            // While dragging from ruler, show ghost guide line at current scene position
            let ruler_drag_active: Option<(bool, f32)> = ui.data(|d| d.get_temp(ruler_drag_id));
            if let Some((is_vertical, _start_val)) = ruler_drag_active {
                if let Some(mouse) = raw_pointer_pos {
                    let scene = self.preview_screen_to_scene(preview_rect, mouse);
                    let guide_color = AMBER;
                    if is_vertical {
                        // Vertical ghost guide
                        let ghost_screen = self.preview_scene_to_screen(preview_rect, kurbo::Point::new(scene.x, 0.0));
                        if ghost_screen.x >= preview_rect.min.x && ghost_screen.x <= preview_rect.max.x {
                            ui.painter().line_segment(
                                [egui::pos2(ghost_screen.x, preview_rect.min.y),
                                 egui::pos2(ghost_screen.x, preview_rect.max.y)],
                                Stroke::new(1.0, guide_color),
                            );
                        }
                    } else {
                        // Horizontal ghost guide
                        let ghost_screen = self.preview_scene_to_screen(preview_rect, kurbo::Point::new(0.0, scene.y));
                        if ghost_screen.y >= preview_rect.min.y && ghost_screen.y <= preview_rect.max.y {
                            ui.painter().line_segment(
                                [egui::pos2(preview_rect.min.x, ghost_screen.y),
                                 egui::pos2(preview_rect.max.x, ghost_screen.y)],
                                Stroke::new(1.0, guide_color),
                            );
                        }
                    }
                }
            }

            // When ruler drag ends, create a guide
            let pointer_released = ui.input(|i| i.pointer.any_released());
            if let Some((is_vertical, _start_val)) = ruler_drag_active {
                if pointer_released || h_ruler_resp.drag_stopped() || v_ruler_resp.drag_stopped() {
                    if let Some(mouse) = raw_pointer_pos {
                        if preview_rect.contains(mouse) {
                            let scene = self.preview_screen_to_scene(preview_rect, mouse);
                            if is_vertical {
                                self.preview.vertical_guides.push(scene.x as f32);
                            } else {
                                self.preview.horizontal_guides.push(scene.y as f32);
                            }
                        }
                    }
                    ui.data_mut(|d| d.remove::<Option<(bool, f32)>>(ruler_drag_id));
                }
            }

            // ── Draw existing guides ──
            let guide_color = AMBER;
            for &guide_y in &self.preview.horizontal_guides {
                let screen_pt = self.preview_scene_to_screen(preview_rect, kurbo::Point::new(0.0, guide_y as f64));
                if screen_pt.y >= preview_rect.min.y && screen_pt.y <= preview_rect.max.y {
                    ui.painter().line_segment(
                        [egui::pos2(preview_rect.min.x, screen_pt.y),
                         egui::pos2(preview_rect.max.x, screen_pt.y)],
                        Stroke::new(1.0, guide_color),
                    );
                }
            }
            for &guide_x in &self.preview.vertical_guides {
                let screen_pt = self.preview_scene_to_screen(preview_rect, kurbo::Point::new(guide_x as f64, 0.0));
                if screen_pt.x >= preview_rect.min.x && screen_pt.x <= preview_rect.max.x {
                    ui.painter().line_segment(
                        [egui::pos2(screen_pt.x, preview_rect.min.y),
                         egui::pos2(screen_pt.x, preview_rect.max.y)],
                        Stroke::new(1.0, guide_color),
                    );
                }
            }

            // ── Scroll zoom ──
            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta);
                if scroll.y != 0.0 {
                    let zoom_factor = 1.0 + scroll.y * 0.001;
                    let new_zoom = (self.preview.preview_zoom * zoom_factor).clamp(0.1, 10.0);
                    let prev_zoom = self.preview.preview_zoom;
                    // Zoom toward cursor: keep the scene point under cursor fixed
                    if let Some(cursor) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                        let cursor_in_rect = preview_rect.contains(cursor);
                        if cursor_in_rect && prev_zoom > 0.01 {
                            let center = preview_rect.center().to_vec2();
                            let cursor_offset = cursor - preview_rect.min;
                            let rel = cursor_offset - center;
                            // Compute the scene point at cursor (pre-zoom)
                            let base_scale_x = self.scene_dimensions.width as f64 / preview_rect.width().max(1.0) as f64;
                            let base_scale_y = self.scene_dimensions.height as f64 / preview_rect.height().max(1.0) as f64;
                            let old_scale_x = base_scale_x / prev_zoom as f64;
                            let old_scale_y = base_scale_y / prev_zoom as f64;
                            let scene_at_cursor = kurbo::Point::new(
                                self.preview.preview_pan.x as f64 + rel.x as f64 * old_scale_x,
                                self.preview.preview_pan.y as f64 + rel.y as f64 * old_scale_y,
                            );
                            // Set new zoom
                            self.preview.preview_zoom = new_zoom;
                            // Adjust pan so cursor stays on the same scene point
                            let new_scale_x = base_scale_x / new_zoom as f64;
                            let new_scale_y = base_scale_y / new_zoom as f64;
                            self.preview.preview_pan = Vec2::new(
                                (scene_at_cursor.x - rel.x as f64 * new_scale_x) as f32,
                                (scene_at_cursor.y - rel.y as f64 * new_scale_y) as f32,
                            );
                            self.preview.status = format!("Zoom: {:.0}%", self.preview.preview_zoom * 100.0);
                        }
                    } else {
                        self.preview.preview_zoom = new_zoom;
                        self.preview.status = format!("Zoom: {:.0}%", self.preview.preview_zoom * 100.0);
                    }
                }
            }

            // ── Middle-click pan ──
            if ui.input(|i| i.pointer.middle_down()) {
                if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                    if preview_rect.contains(mouse) {
                        let delta = ui.input(|i| i.pointer.delta());
                        if delta != Vec2::ZERO {
                            let base_scale_x = self.scene_dimensions.width as f64 / preview_rect.width().max(1.0) as f64;
                            let base_scale_y = self.scene_dimensions.height as f64 / preview_rect.height().max(1.0) as f64;
                            let scale_x = base_scale_x / self.preview.preview_zoom.max(0.01) as f64;
                            let scale_y = base_scale_y / self.preview.preview_zoom.max(0.01) as f64;
                            self.preview.preview_pan = Vec2::new(
                                self.preview.preview_pan.x - delta.x as f32 * scale_x as f32,
                                self.preview.preview_pan.y - delta.y as f32 * scale_y as f32,
                            );
                        }
                    }
                }
            }

            // Clear snap lines from previous frame
            self.preview.snap_lines_h.clear();
            self.preview.snap_lines_v.clear();
            self.preview.snap_line_color = None;
            self.preview.snap_hud_label = None;

            // ── Time Lens (Space-drag HUD) ──
            let mut all_kf: Vec<f64> = if let Some(tl) = self.timeline {
                tl.root_actor_labels().iter().flat_map(|label| {
                    tl.get_track(label).map(|track| {
                        crate::app::panels::inspector::collect_all_keyframe_times(track)
                    }).unwrap_or_default()
                }).collect()
            } else {
                Vec::new()
            };
            all_kf.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            all_kf.dedup_by(|a, b| (*a - *b).abs() < 0.001);
            if let Some(new_time) = self.preview.time_lens.update_and_show(
                ui,
                self.preview.current_time_s,
                self.preview.duration_s,
                &all_kf,
            ) {
                self.commands.push_back(Command::ScrubTo(new_time));
            }

            let is_dragging = !matches!(self.drag_state, DragState::None);
            if self.handle_preview_drag(ui, preview_rect, &response) {
                return;
            }

            let pointer_pos = ui
                .ctx()
                .input(|i| i.pointer.latest_pos())
                .filter(|p| preview_rect.contains(*p));
            let scene_dimensions = self.scene_dimensions;
            let zoom = self.preview.preview_zoom;
            let pan = self.preview.preview_pan;
            let screen_to_scene = move |screen: egui::Pos2| preview_screen_to_scene(scene_dimensions, preview_rect, screen, zoom, pan);

            if !self.selection.context_menu_open {
                selection::update_hover(
                    self.selection,
                    self.hit_regions,
                    pointer_pos,
                    screen_to_scene,
                    is_dragging,
                );
            } else {
                self.selection.hovered_actor = None;
            }

            self.handle_preview_selection(ui, preview_rect, &response);
            self.render_preview_cursor_feedback(ui, preview_rect);
            self.render_preview_overlays(ui, preview_rect);
            self.render_preview_content(ui, preview_rect);

            // ── Diff mode overlay ──
            if self.preview.diff_mode {
                let split_x = preview_rect.center().x;
                // Vertical divider
                ui.painter().line_segment(
                    [egui::pos2(split_x, preview_rect.min.y), egui::pos2(split_x, preview_rect.max.y)],
                    Stroke::new(2.0, AMBER),
                );
                // Labels
                ui.painter().text(
                    egui::pos2(preview_rect.min.x + SPACE_M, preview_rect.min.y + SPACE_S),
                    egui::Align2::LEFT_TOP,
                    format!("{} Before", egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE),
                    egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                    TEXT_SECONDARY,
                );
                ui.painter().text(
                    egui::pos2(preview_rect.max.x - SPACE_M, preview_rect.min.y + SPACE_S),
                    egui::Align2::RIGHT_TOP,
                    format!("After {}", egui_phosphor::regular::CLOCK_CLOCKWISE),
                    egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                    ACCENT_BLUE,
                );
                // Note: true dual rendering requires a second preview texture.
                // The left half currently shows the same as the right half.
            }

            // Draw grid overlay
            if *self.grid_enabled {
                preview::grid::draw_grid(
                    ui.painter(),
                    self.scene_dimensions,
                    preview_rect,
                    self.preview.preview_zoom,
                    self.preview.preview_pan,
                    *self.grid_size,
                );
            }

            // ── Draw snap indicator lines ──
            if let Some(color) = self.preview.snap_line_color {
                for &sy in &self.preview.snap_lines_h {
                    let screen_pt = self.preview_scene_to_screen(preview_rect, kurbo::Point::new(0.0, sy as f64));
                    if screen_pt.y >= preview_rect.min.y && screen_pt.y <= preview_rect.max.y {
                        ui.painter().line_segment(
                            [egui::pos2(preview_rect.min.x, screen_pt.y),
                             egui::pos2(preview_rect.max.x, screen_pt.y)],
                            Stroke::new(1.0, color),
                        );
                    }
                }
                for &sx in &self.preview.snap_lines_v {
                    let screen_pt = self.preview_scene_to_screen(preview_rect, kurbo::Point::new(sx as f64, 0.0));
                    if screen_pt.x >= preview_rect.min.x && screen_pt.x <= preview_rect.max.x {
                        ui.painter().line_segment(
                            [egui::pos2(screen_pt.x, preview_rect.min.y),
                             egui::pos2(screen_pt.x, preview_rect.max.y)],
                            Stroke::new(1.0, color),
                        );
                    }
                }
            }

            self.render_preview_selection_overlay(ui, preview_rect, is_dragging);

            // Floating property cards for selected actors
            if !is_dragging && self.selected_actors.len() == 1 {
                if let Some(actor) = self.selected_actors.iter().next() {
                    if let Some(props) = self.get_actor_props(actor) {
                        let screen_pos = preview::scene_to_screen(
                            kurbo::Point::new(props.position[0] as f64, props.position[1] as f64),
                            preview_rect, self.scene_dimensions, preview_rect.size(),
                            self.preview.preview_zoom, self.preview.preview_pan,
                        );
                        preview::floating_card::show_floating_card(
                            ui, actor, &props, screen_pos, self.commands,
                        );
                    }
                }
            }

            // NOTE: errors are shown in the diagnostics banner above the canvas,
            // not as an overlay, to avoid duplicating the same message.
        });
        });
    }
}