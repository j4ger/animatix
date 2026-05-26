use super::*;

pub mod input;
pub mod overlay;
pub mod render;

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

    pub(super) fn preview_ui(&mut self, ui: &mut egui::Ui) {
        const PLAYING_TEXT: Color32 = crate::app::design_tokens::PLAYING_TEXT;

        panel_frame().show(ui, |ui| {
        ui.vertical(|ui| {
            // ── Header toolbar ──
            let header_avail = ui.available_width();
            let header_h = ROW_L;
            let show_labels = header_avail > 520.0;
            let show_all_buttons = header_avail > 340.0;

            let (header_rect, _) = ui.allocate_exact_size(Vec2::new(header_avail, header_h), egui::Sense::hover());

            ui.scope_builder(egui::UiBuilder::new().max_rect(header_rect), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);

                    // Left: panel title — small, uppercase, muted
                    ui.add(
                        egui::Label::new(
                            RichText::new("PREVIEW").size(FONT_SIZE_S).color(TEXT_MUTED),
                        )
                        .selectable(false),
                    );

                    ui.add_space(SPACE_S);

                    // ── Button group 1: View ──
                    let grid_icon = if self.preview.overlay.show_grid {
                        egui_phosphor::regular::GRID_FOUR
                    } else {
                        egui_phosphor::regular::GRID_NINE
                    };
                    if crate::app::components::toolbar_toggle_button(
                        ui,
                        grid_icon,
                        Some("Grid"),
                        "Toggle grid overlay (G)",
                        self.preview.overlay.show_grid,
                        show_labels,
                    )
                    .clicked()
                    {
                        self.preview.overlay.show_grid = !self.preview.overlay.show_grid;
                    }

                    if crate::app::components::toolbar_action_button(
                        ui,
                        egui_phosphor::regular::ARROWS_OUT_CARDINAL,
                        Some("Reset"),
                        "Reset zoom and pan to default (0)",
                        show_labels,
                    )
                    .clicked()
                    {
                        self.preview.preview_zoom = 1.0;
                        let scene_w = self.scene_dimensions.width as f32;
                        let scene_h = self.scene_dimensions.height as f32;
                        self.preview.preview_pan = Vec2::new(scene_w / 2.0, scene_h / 2.0);
                        self.preview.status = "Zoom/Pan reset".to_string();
                    }

                    crate::app::components::toolbar_separator(ui);

                    // ── Button group 2: Modes ──
                    if show_all_buttons {
                        if crate::app::components::toolbar_toggle_button(
                            ui,
                            egui_phosphor::regular::SQUARE_SPLIT_HORIZONTAL,
                            Some("Slices"),
                            "Toggle scene slice tabs (Shift+S)",
                            self.preview.scene_slices.enabled,
                            show_labels,
                        )
                        .clicked()
                        {
                            self.preview.scene_slices.toggle();
                            self.preview.status = if self.preview.scene_slices.enabled {
                                "Scene slices enabled".to_string()
                            } else {
                                "Scene slices disabled".to_string()
                            };
                        }

                        crate::app::components::toolbar_separator(ui);

                        // ── Button group 3: Overlays — menu only, no click toggle ──
                        let any_overlay_on = self.preview.overlay.show_grid
                            || self.preview.overlay.show_guides
                            || self.preview.overlay.show_hover_highlight
                            || self.preview.overlay.show_snap_guides
                            || self.preview.overlay.show_scene_bounds
                            || self.preview.overlay.show_actor_labels
                            || self.preview.overlay.show_safe_area;
                        let eye_response = crate::app::components::toolbar_toggle_button(
                            ui,
                            egui_phosphor::regular::EYE,
                            Some("Overlays"),
                            "Overlay options — right-click for menu",
                            any_overlay_on,
                            show_labels,
                        );
                        eye_response.context_menu(|ui| {
                            ui.checkbox(&mut self.preview.overlay.show_scene_bounds, "Scene bounds");
                            ui.checkbox(&mut self.preview.overlay.show_grid, "Grid");
                            ui.checkbox(&mut self.preview.overlay.show_guides, "Guides");
                            ui.checkbox(&mut self.preview.overlay.show_actor_labels, "Actor labels");
                            ui.checkbox(&mut self.preview.overlay.show_safe_area, "Safe area");
                            ui.checkbox(&mut self.preview.overlay.show_snap_guides, "Snap guides");
                            ui.checkbox(&mut self.preview.overlay.show_hover_highlight, "Hover highlight");
                        });
                    } else {
                        // Very narrow: overflow menu for Modes + Overlays
                        let overflow = crate::app::components::toolbar_action_button(
                            ui,
                            egui_phosphor::regular::LIST,
                            None,
                            "Additional preview controls (slices, overlays)",
                            false,
                        );
                        overflow.context_menu(|ui| {
                            ui.checkbox(&mut self.preview.overlay.show_grid, "Grid");
                            let mut slices = self.preview.scene_slices.enabled;
                            if ui.checkbox(&mut slices, "Scene slices").changed() {
                                self.preview.scene_slices.enabled = slices;
                            }
                            ui.checkbox(&mut self.preview.overlay.show_scene_bounds, "Scene bounds");
                            ui.checkbox(&mut self.preview.overlay.show_guides, "Guides");
                            ui.checkbox(&mut self.preview.overlay.show_actor_labels, "Actor labels");
                            ui.checkbox(&mut self.preview.overlay.show_safe_area, "Safe area");
                            ui.checkbox(&mut self.preview.overlay.show_snap_guides, "Snap guides");
                            ui.checkbox(&mut self.preview.overlay.show_hover_highlight, "Hover highlight");
                        });
                    }

                    // Push remaining space to the right
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);

                        // Playback status badge — pill style
                        let (badge_label, badge_fill, badge_text, badge_stroke) = if self.preview.is_playing {
                            ("Playing", green_faint(), PLAYING_TEXT, Some(Stroke::new(1.0, GREEN)))
                        } else {
                            ("Paused", BG_WIDGET, TEXT_SECONDARY, Some(Stroke::new(1.0, BORDER)))
                        };
                        egui::Frame::new()
                            .fill(badge_fill)
                            .corner_radius(egui::CornerRadius::same(RADIUS_L as u8))
                            .inner_margin(egui::Margin::symmetric(8, 2))
                            .stroke(badge_stroke.unwrap_or(Stroke::NONE))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(badge_label)
                                            .size(FONT_SIZE_S)
                                            .color(badge_text)
                                            .strong(),
                                    )
                                    .selectable(false),
                                );
                            });
                    });
                });
            });

            ui.add_space(SPACE_S);

            // Scene slice tabs
            if self.preview.scene_slices.enabled {
                crate::app::preview::scene_slices::render_slice_tabs(ui, &mut self.preview.scene_slices);
                ui.add_space(SPACE_S);
            }

            let available = ui.available_size_before_wrap();
            // Reserve space for rulers so they don't overlap the header
            let preview_available = Vec2::new(
                (available.x - RULER_SIZE).max(200.0),
                (available.y - RULER_SIZE).max(180.0),
            );
            let desired = fit_preview(self.scene_dimensions, preview_available);
            let total_size = desired + Vec2::new(RULER_SIZE, RULER_SIZE);
            let (total_rect, _) = ui.allocate_exact_size(total_size, egui::Sense::hover());
            let preview_rect = egui::Rect::from_min_size(
                egui::pos2(total_rect.min.x + RULER_SIZE, total_rect.min.y + RULER_SIZE),
                desired,
            );
            let response = ui.allocate_rect(preview_rect, egui::Sense::click_and_drag());
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
            if self.preview.overlay.show_guides {
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
                                self.preview.preview_pan.x - delta.x * scale_x as f32,
                                self.preview.preview_pan.y - delta.y * scale_y as f32,
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

            // ── Scene bounds overlay ──
            if self.preview.overlay.show_scene_bounds {
                let bounds_rect = preview::scene_to_screen(
                    kurbo::Point::new(0.0, 0.0),
                    preview_rect, self.scene_dimensions, preview_rect.size(),
                    self.preview.preview_zoom, self.preview.preview_pan,
                );
                let bounds_br = preview::scene_to_screen(
                    kurbo::Point::new(self.scene_dimensions.width as f64, self.scene_dimensions.height as f64),
                    preview_rect, self.scene_dimensions, preview_rect.size(),
                    self.preview.preview_zoom, self.preview.preview_pan,
                );
                let bounds_screen = egui::Rect::from_min_max(bounds_rect, bounds_br);
                ui.painter().rect_stroke(
                    bounds_screen,
                    0.0,
                    Stroke::new(1.0, BORDER_HOVER),
                    egui::StrokeKind::Inside,
                );
            }

            // ── Actor labels overlay ──
            if self.preview.overlay.show_actor_labels {
                for (label, bounds) in self.hit_regions {
                    let center = preview::scene_to_screen(
                        kurbo::Point::new((bounds.x0 + bounds.x1) / 2.0, bounds.y0 - 4.0),
                        preview_rect, self.scene_dimensions, preview_rect.size(),
                        self.preview.preview_zoom, self.preview.preview_pan,
                    );
                    ui.painter().text(
                        center,
                        egui::Align2::CENTER_BOTTOM,
                        label,
                        egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
                        TEXT_MUTED,
                    );
                }
            }

            // Draw grid overlay
            if self.preview.overlay.show_grid {
                preview::grid::draw_grid(
                    ui.painter(),
                    self.scene_dimensions,
                    preview_rect,
                    self.preview.preview_zoom,
                    self.preview.preview_pan,
                    self.preview.overlay.grid_size,
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