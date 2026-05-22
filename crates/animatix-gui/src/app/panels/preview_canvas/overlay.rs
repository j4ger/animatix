use super::super::*;

impl WorkspaceViewer<'_> {
    /// Render hover highlight and cycle indicator overlays.
    pub(super) fn render_preview_overlays(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
    ) {
        if self.selection.context_menu_open {
            return;
        }

        let pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos()).filter(|p| preview_rect.contains(*p));

        // Hover highlight
        if self.preview.overlay.show_hover_highlight {
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
        }

        // Smart snap guides during drag
        if self.preview.overlay.show_snap_guides {
            if let DragState::Move { primary, .. } | DragState::Scale { actor: primary, .. } = &self.drag_state {
                self.draw_snap_guides(ui, preview_rect, primary);
            }
        }

        // ── Snap HUD label ──
        if self.preview.overlay.show_snap_guides {
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
        }

        // Cycle indicator (click-through multiple overlapping actors)
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
    pub(super) fn draw_snap_guides(&self, ui: &mut egui::Ui, preview_rect: egui::Rect, primary: &str) {
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
}