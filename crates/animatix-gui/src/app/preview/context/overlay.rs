//! Canvas content, overlay HUD, guides, motion paths, and layout debug drawing.

use std::collections::HashSet;

use egui::Vec2;

use crate::app::design_tokens::spatial::STROKE_WIDTH;
use crate::app::design_tokens::spatial::preview::MIN_ZOOM as PREVIEW_MIN_ZOOM;
use crate::app::design_tokens::typography::TextRole;
use crate::app::preview::overlay_ops::{
    PreviewOverlayOp, cycle_indicator_ops, execute_overlay_ops, hover_highlight_ops,
    layout_debug_ops, motion_path_ops,
};
use crate::app::preview::{self, DragState};

use super::PreviewContext;

impl PreviewContext<'_> {
    pub(crate) fn render_preview_content(&self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        let theme = eparts::theme(ui);
        match self.preview_texture_id {
            Some(texture_id) => {
                let zoom = self.preview.viewport.preview_zoom;
                let pan = self.preview.viewport.preview_pan;
                let scene_w = self.scene_dimensions.width.max(1) as f32;
                let scene_h = self.scene_dimensions.height.max(1) as f32;
                let tx =
                    preview::PreviewTransform::new(self.scene_dimensions, preview_rect, zoom, pan);
                let display_rect = tx.display_rect();

                if (zoom - 1.0).abs() > 0.001 || pan != Vec2::new(scene_w / 2.0, scene_h / 2.0) {
                    let half_inv_zx = 0.5 / zoom.max(PREVIEW_MIN_ZOOM);
                    let half_inv_zy = 0.5 / zoom.max(PREVIEW_MIN_ZOOM);
                    let uv_cx = (pan.x / scene_w).clamp(0.0, 1.0);
                    let uv_cy = (pan.y / scene_h).clamp(0.0, 1.0);
                    let uv_rect = egui::Rect::from_min_max(
                        egui::pos2(
                            (uv_cx - half_inv_zx).clamp(0.0, 1.0),
                            (uv_cy - half_inv_zy).clamp(0.0, 1.0),
                        ),
                        egui::pos2(
                            (uv_cx + half_inv_zx).clamp(0.0, 1.0),
                            (uv_cy + half_inv_zy).clamp(0.0, 1.0),
                        ),
                    );
                    ui.put(
                        display_rect,
                        egui::Image::new((texture_id, display_rect.size())).uv(uv_rect),
                    );
                } else {
                    ui.put(display_rect, egui::Image::new((texture_id, display_rect.size())));
                }
            },
            None => {
                if !self.has_scene {
                    ui.painter().text(
                        preview_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "No scene to preview",
                        egui::TextStyle::Body.resolve(ui.style()),
                        theme.palette.text.muted,
                    );
                    let hint_pos = preview_rect.center() + egui::vec2(0.0, 20.0);
                    ui.painter().text(
                        hint_pos,
                        egui::Align2::CENTER_CENTER,
                        "Open a file or create a scene to get started",
                        egui::TextStyle::Body.resolve(ui.style()),
                        theme.palette.text.muted,
                    );
                } else {
                    ui.painter().text(
                        preview_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "Preview initializing…",
                        egui::TextStyle::Body.resolve(ui.style()),
                        theme.palette.text.muted,
                    );
                }
            },
        }
    }

    pub(crate) fn render_preview_overlays(&mut self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        let theme = eparts::theme(ui);
        if self.selection.context_menu_open {
            return;
        }

        let tx = self.preview_transform(preview_rect);
        let mut ops: Vec<PreviewOverlayOp> = Vec::new();
        if let Some(mouse) =
            ui.ctx().input(|i| i.pointer.latest_pos()).filter(|p| preview_rect.contains(*p))
        {
            if !self
                .selected_actors
                .contains(self.selection.hovered_actor.as_deref().unwrap_or(""))
            {
                ops.extend(cycle_indicator_ops(
                    &theme,
                    mouse,
                    self.selection.cycle_index,
                    self.selection.click_candidates.len(),
                ));
            }
        }

        if self.preview.overlay.show_hover_highlight {
            if let Some(hovered) = self.selection.hovered_actor.as_ref() {
                if !self.selected_actors.contains(hovered) {
                    if let Some(hover_rect) = preview::selection_screen_rect(
                        &HashSet::from([hovered.clone()]),
                        self.hit_regions,
                        preview_rect,
                        self.scene_dimensions,
                        preview_rect.size(),
                        self.preview.viewport.preview_zoom,
                        self.preview.viewport.preview_pan,
                    ) {
                        let scene_min = tx.screen_to_scene(hover_rect.min);
                        let scene_max = tx.screen_to_scene(hover_rect.max);
                        let hover_scene = kurbo::Rect::new(
                            scene_min.x.min(scene_max.x),
                            scene_min.y.min(scene_max.y),
                            scene_min.x.max(scene_max.x),
                            scene_min.y.max(scene_max.y),
                        );
                        ops.extend(hover_highlight_ops(&theme, hovered, hover_scene, tx));
                    }
                }
            }
        }
        execute_overlay_ops(ui.painter(), &ops, &tx);

        if self.preview.overlay.show_snap_guides {
            if let DragState::Move { primary, .. } | DragState::Scale { actor: primary, .. } =
                &self.drag_state
            {
                self.draw_snap_guides(ui, preview_rect, primary);
            }
        }

        if self.preview.overlay.show_snap_guides {
            if let Some(ref label) = self.preview.snap.snap_hud_label {
                if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                    let hud_pos = mouse + Vec2::new(12.0, -24.0);
                    let galley = ui.painter().layout_no_wrap(
                        label.clone(),
                        TextRole::BodyS.font_id(),
                        theme.palette.status.success,
                    );
                    let padding = Vec2::new(8.0, 4.0);
                    let bg_rect = egui::Rect::from_min_size(hud_pos, galley.size() + padding * 2.0);
                    ui.painter().rect_filled(bg_rect, 3.0, theme.palette.overlay.tooltip_bg);
                    ui.painter().rect_stroke(
                        bg_rect,
                        3.0,
                        egui::Stroke::new(
                            STROKE_WIDTH,
                            crate::app::design_tokens::semantic::canvas::snap_guide_line(),
                        ),
                        egui::StrokeKind::Outside,
                    );
                    ui.painter().galley(hud_pos + padding, galley, theme.palette.status.success);
                }
            }
        }

        // ── Performance HUD ──
        if self.preview.overlay.show_performance_hud {
            preview::overlay::render_performance_hud(
                ui.painter(),
                theme,
                preview_rect,
                self.performance_metrics,
            );
        }

        // ── Rebuild indicator pill ──
        if self.rebuild_in_progress {
            let pill_text = "Rebuilding…";
            let galley = ui.painter().layout_no_wrap(
                pill_text.to_string(),
                TextRole::Micro.font_id(),
                theme.palette.text.muted,
            );
            let padding = Vec2::new(8.0, 4.0);
            let pill_size = galley.size() + padding * 2.0;
            let pill_rect = egui::Rect::from_min_size(
                preview_rect.right_bottom() - pill_size - egui::vec2(8.0, 8.0),
                pill_size,
            );
            ui.painter().rect_filled(pill_rect, 3.0, theme.palette.surface.widget);
            ui.painter().galley(
                pill_rect.left_center() + egui::vec2(padding.x, -galley.size().y / 2.0),
                galley,
                theme.palette.text.muted,
            );
        }
    }

    pub(crate) fn draw_snap_guides(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        primary: &str,
    ) {
        let primary_props = self.get_actor_props(primary);
        let primary_rect = if let Some(p) = primary_props {
            let hw = p.size[0] / 2.0;
            let hh = p.size[1] / 2.0;
            let corners = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;
            for corner in &corners {
                let world = preview::local_to_world(*corner, p.position, p.rotation);
                let screen = preview::scene_to_screen(
                    world,
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                    self.preview.viewport.preview_zoom,
                    self.preview.viewport.preview_pan,
                );
                min_x = min_x.min(screen.x);
                min_y = min_y.min(screen.y);
                max_x = max_x.max(screen.x);
                max_y = max_y.max(screen.y);
            }
            egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y))
        } else {
            self.hit_regions
                .iter()
                .find(|(l, _)| l == primary)
                .map(|(_, bounds)| {
                    let tl = preview::scene_to_screen(
                        kurbo::Point::new(bounds.x0, bounds.y0),
                        preview_rect,
                        self.scene_dimensions,
                        preview_rect.size(),
                        self.preview.viewport.preview_zoom,
                        self.preview.viewport.preview_pan,
                    );
                    let br = preview::scene_to_screen(
                        kurbo::Point::new(bounds.x1, bounds.y1),
                        preview_rect,
                        self.scene_dimensions,
                        preview_rect.size(),
                        self.preview.viewport.preview_zoom,
                        self.preview.viewport.preview_pan,
                    );
                    egui::Rect::from_min_max(tl, br)
                })
                .unwrap_or(preview_rect)
        };

        let theme = eparts::theme(ui);
        let threshold = 8.0;
        let guide_color = theme.palette.accent.subtle;
        let guide_stroke = egui::Stroke::new(STROKE_WIDTH, guide_color);

        for (label, bounds) in self.hit_regions {
            if label == primary || self.selected_actors.contains(label) {
                continue;
            }
            let tl = preview::scene_to_screen(
                kurbo::Point::new(bounds.x0, bounds.y0),
                preview_rect,
                self.scene_dimensions,
                preview_rect.size(),
                self.preview.viewport.preview_zoom,
                self.preview.viewport.preview_pan,
            );
            let br = preview::scene_to_screen(
                kurbo::Point::new(bounds.x1, bounds.y1),
                preview_rect,
                self.scene_dimensions,
                preview_rect.size(),
                self.preview.viewport.preview_zoom,
                self.preview.viewport.preview_pan,
            );
            let other_rect = egui::Rect::from_min_max(tl, br);

            let px = [
                primary_rect.min.x,
                primary_rect.max.x,
                primary_rect.center().x,
            ];
            let py = [
                primary_rect.min.y,
                primary_rect.max.y,
                primary_rect.center().y,
            ];
            let ox = [other_rect.min.x, other_rect.max.x, other_rect.center().x];
            let oy = [other_rect.min.y, other_rect.max.y, other_rect.center().y];

            for &px in &px {
                for &ox in &ox {
                    if (px - ox).abs() < threshold {
                        ui.painter().line_segment(
                            [
                                egui::pos2(px, preview_rect.min.y),
                                egui::pos2(px, preview_rect.max.y),
                            ],
                            guide_stroke,
                        );
                    }
                }
            }
            for &py in &py {
                for &oy in &oy {
                    if (py - oy).abs() < threshold {
                        ui.painter().line_segment(
                            [
                                egui::pos2(preview_rect.min.x, py),
                                egui::pos2(preview_rect.max.x, py),
                            ],
                            guide_stroke,
                        );
                    }
                }
            }
        }
    }

    pub(crate) fn render_motion_paths(&mut self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        let theme = eparts::theme(ui);
        if !self.preview.overlay.show_motion_paths {
            return;
        }

        let Some(timeline) = self.timeline else {
            return;
        };

        let tx = self.preview_transform(preview_rect);
        let ops = motion_path_ops(
            &theme,
            timeline,
            self.selected_actors.iter().cloned(),
            self.preview.playback.current_time_s(),
            tx,
        );
        execute_overlay_ops(ui.painter(), &ops, &tx);
    }

    /// Render layout debug overlay showing container bounds, slot outlines, and sizes.
    pub(crate) fn render_layout_debug(&self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        let theme = eparts::theme(ui);
        let Some(timeline) = self.timeline else {
            return;
        };
        if !self.debug_layout {
            return;
        }
        let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
        let tx = self.preview_transform(preview_rect);
        let ops = layout_debug_ops(&theme, timeline, time_ms, tx, self.debug_spacing);
        execute_overlay_ops(ui.painter(), &ops, &tx);
    }
}
