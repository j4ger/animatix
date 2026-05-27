use super::super::*;
use egui::Align2;

impl WorkspaceViewer<'_> {
    /// Render cursor feedback for the preview.
    pub(super) fn render_preview_cursor_feedback(&self, ui: &egui::Ui, preview_rect: egui::Rect) {
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
                        ui.label(egui::RichText::new(tooltip).size(crate::app::design_tokens::FONT_SIZE_S));
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

    /// Render the actual preview content (vello scene).
    pub(super) fn render_preview_content(&self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        match self.preview_texture_id {
            Some(texture_id) => {
                let zoom = self.preview.preview_zoom;
                let pan = self.preview.preview_pan;
                let scene_w = self.scene_dimensions.width.max(1) as f32;
                let scene_h = self.scene_dimensions.height.max(1) as f32;

                // Compute letterboxed display rect (preserves scene aspect ratio)
                let tx = preview::PreviewTransform::new(
                    self.scene_dimensions, preview_rect, zoom, pan,
                );
                let display_rect = tx.display_rect();

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
                    let image = egui::Image::new((texture_id, display_rect.size())).uv(uv_rect);
                    ui.put(display_rect, image);
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

    pub(super) fn render_preview_selection_overlay(
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

            // ── Measurement lines during drag ──
            if is_dragging {
                let measurement_color = ACCENT_BLUE;
                let text_color = TEXT_PRIMARY;
                let font = egui::FontId::monospace(FONT_SIZE_XS);

                match &self.drag_state {
                    DragState::Move { primary, actors, start_scene } => {
                        if let Some(props) = self.get_actor_props(primary) {
                            let start_screen = preview::scene_to_screen(
                                kurbo::Point::new(start_scene.x, start_scene.y),
                                preview_rect, self.scene_dimensions, preview_rect.size(),
                                self.preview.preview_zoom, self.preview.preview_pan,
                            );
                            let current_screen = preview::scene_to_screen(
                                kurbo::Point::new(props.position[0] as f64, props.position[1] as f64),
                                preview_rect, self.scene_dimensions, preview_rect.size(),
                                self.preview.preview_zoom, self.preview.preview_pan,
                            );

                            // Horizontal measurement line
                            let y = (start_screen.y + current_screen.y) / 2.0;
                            ui.painter().line_segment(
                                [Pos2::new(start_screen.x.min(current_screen.x), y), Pos2::new(start_screen.x.max(current_screen.x), y)],
                                Stroke::new(1.0, measurement_color),
                            );
                            let dx = props.position[0] - start_scene.x as f32;
                            ui.painter().text(
                                Pos2::new((start_screen.x + current_screen.x) / 2.0, y - 8.0),
                                Align2::CENTER_BOTTOM,
                                format!("Δx: {:+.0}", dx),
                                font.clone(),
                                text_color,
                            );

                            // Vertical measurement line
                            let x = (start_screen.x + current_screen.x) / 2.0;
                            ui.painter().line_segment(
                                [Pos2::new(x, start_screen.y.min(current_screen.y)), Pos2::new(x, start_screen.y.max(current_screen.y))],
                                Stroke::new(1.0, measurement_color),
                            );
                            let dy = props.position[1] - start_scene.y as f32;
                            ui.painter().text(
                                Pos2::new(x + 4.0, (start_screen.y + current_screen.y) / 2.0),
                                Align2::LEFT_CENTER,
                                format!("Δy: {:+.0}", dy),
                                font.clone(),
                                text_color,
                            );
                        }
                    }
                    DragState::Scale { actor, start_size, .. } => {
                        if let Some(props) = self.get_actor_props(actor) {
                            let screen_pos = preview::scene_to_screen(
                                kurbo::Point::new(props.position[0] as f64, props.position[1] as f64),
                                preview_rect, self.scene_dimensions, preview_rect.size(),
                                self.preview.preview_zoom, self.preview.preview_pan,
                            );
                            let bottom_right = preview::scene_to_screen(
                                kurbo::Point::new(
                                    props.position[0] as f64 + props.size[0] as f64 / 2.0,
                                    props.position[1] as f64 + props.size[1] as f64 / 2.0,
                                ),
                                preview_rect, self.scene_dimensions, preview_rect.size(),
                                self.preview.preview_zoom, self.preview.preview_pan,
                            );
                            // Width label at bottom edge
                            ui.painter().text(
                                Pos2::new(screen_pos.x, bottom_right.y + 12.0),
                                Align2::CENTER_TOP,
                                format!("w: {:.0} → {:.0}", start_size[0], props.size[0]),
                                font.clone(),
                                text_color,
                            );
                            // Height label at right edge
                            ui.painter().text(
                                Pos2::new(bottom_right.x + 4.0, screen_pos.y),
                                Align2::LEFT_CENTER,
                                format!("h: {:.0} → {:.0}", start_size[1], props.size[1]),
                                font.clone(),
                                text_color,
                            );
                        }
                    }
                    DragState::Rotate { actor, start_rotation, .. } => {
                        if let Some(props) = self.get_actor_props(actor) {
                            let screen_pos = preview::scene_to_screen(
                                kurbo::Point::new(props.position[0] as f64, props.position[1] as f64),
                                preview_rect, self.scene_dimensions, preview_rect.size(),
                                self.preview.preview_zoom, self.preview.preview_pan,
                            );
                            let start_deg = start_rotation.to_degrees();
                            let current_deg = props.rotation.to_degrees();
                            ui.painter().text(
                                Pos2::new(screen_pos.x, screen_pos.y - props.size[1] / 2.0 - 16.0),
                                Align2::CENTER_BOTTOM,
                                format!("{:.0}° → {:.0}°", start_deg, current_deg),
                                font.clone(),
                                text_color,
                            );
                        }
                    }
                    _ => {}
                }
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
}