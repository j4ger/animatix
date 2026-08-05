//! Preview panel: canvas with rulers, zoom/pan, drag interaction, and overlays.

use animatix::timeline::SceneDimensions;
use egui::Vec2;

use crate::app::commands::{ActorCommand, DocumentCommand, PlaybackCommand};
use crate::app::design_tokens::semantic::{border, status, surface, text};
use crate::app::design_tokens::spatial::{RADIUS_L, STROKE_WIDTH, preview as preview_spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::panels::{RULER_SIZE, nice_tick_interval};
pub(crate) use crate::app::preview::context::PreviewContext;
use crate::app::preview::{self, DragState, fit_preview, selection};

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
                    ctx.preview.viewport.preview_zoom =
                        desired.x / ctx.scene_dimensions.width as f32;
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
                ui.painter().rect_stroke(
                    preview_rect,
                    RADIUS_L,
                    egui::Stroke::new(STROKE_WIDTH, border::DEFAULT),
                    egui::StrokeKind::Outside,
                );
                ui.painter().rect_filled(preview_rect, RADIUS_L, surface::BASE);

                // ── Rulers ──
                let ruler_bg = surface::PANEL;
                let ruler_tick_color = text::MUTED;
                let ruler_text_color = text::MUTED;
                let ruler_label_color = text::SECONDARY;

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
                let ruler_stroke = egui::Stroke::new(STROKE_WIDTH, border::DEFAULT);

                ui.painter().rect_filled(corner_rect, 0.0, ruler_bg);
                ui.painter()
                    .rect_stroke(corner_rect, 0.0, ruler_stroke, egui::StrokeKind::Outside);

                let scene_tl = preview_screen_to_scene(
                    ctx.scene_dimensions,
                    preview_rect,
                    preview_rect.left_top(),
                    ctx.preview.viewport.preview_zoom,
                    ctx.preview.viewport.preview_pan,
                );
                let scene_br = preview_screen_to_scene(
                    ctx.scene_dimensions,
                    preview_rect,
                    preview_rect.right_bottom(),
                    ctx.preview.viewport.preview_zoom,
                    ctx.preview.viewport.preview_pan,
                );
                let visible_w = (scene_br.x - scene_tl.x) as f32;
                let visible_h = (scene_br.y - scene_tl.y) as f32;

                // Horizontal ruler
                ui.painter().rect_filled(h_ruler_rect, 0.0, ruler_bg);
                ui.painter().rect_stroke(
                    h_ruler_rect,
                    0.0,
                    ruler_stroke,
                    egui::StrokeKind::Outside,
                );
                let h_interval =
                    nice_tick_interval(visible_w, h_ruler_rect.width() / 60.0).max(1.0);
                let h_start = ((scene_tl.x as f32) / h_interval).floor() as i32 * h_interval as i32;
                let h_end = ((scene_br.x as f32) / h_interval).ceil() as i32 * h_interval as i32;
                let mut tick_x = h_start as f32;
                while tick_x <= h_end as f32 {
                    let screen_pt = preview_scene_to_screen(
                        ctx.scene_dimensions,
                        preview_rect,
                        kurbo::Point::new(tick_x as f64, scene_tl.y),
                        ctx.preview.viewport.preview_zoom,
                        ctx.preview.viewport.preview_pan,
                    );
                    if screen_pt.x >= h_ruler_rect.min.x && screen_pt.x <= h_ruler_rect.max.x {
                        let rel_x = screen_pt.x - h_ruler_rect.min.x;
                        let is_major = (tick_x as i32) % (h_interval as i32 * 5) == 0;
                        let tick_h = if is_major {
                            RULER_SIZE * 0.6
                        } else {
                            RULER_SIZE * 0.3
                        };
                        ui.painter().line_segment(
                            [
                                egui::pos2(h_ruler_rect.min.x + rel_x, h_ruler_rect.max.y),
                                egui::pos2(h_ruler_rect.min.x + rel_x, h_ruler_rect.max.y - tick_h),
                            ],
                            egui::Stroke::new(
                                STROKE_WIDTH,
                                if is_major {
                                    ruler_label_color
                                } else {
                                    ruler_tick_color
                                },
                            ),
                        );
                        if is_major {
                            ui.painter().text(
                                egui::pos2(
                                    h_ruler_rect.min.x + rel_x,
                                    h_ruler_rect.min.y + RULER_SIZE * 0.3,
                                ),
                                egui::Align2::CENTER_CENTER,
                                format!("{}", tick_x as i32),
                                TextRole::Micro.font_id(),
                                ruler_text_color,
                            );
                        }
                    }
                    tick_x += h_interval;
                }

                // Vertical ruler
                ui.painter().rect_filled(v_ruler_rect, 0.0, ruler_bg);
                ui.painter().rect_stroke(
                    v_ruler_rect,
                    0.0,
                    ruler_stroke,
                    egui::StrokeKind::Outside,
                );
                let v_interval =
                    nice_tick_interval(visible_h, v_ruler_rect.height() / 60.0).max(1.0);
                let v_start = ((scene_tl.y as f32) / v_interval).floor() as i32 * v_interval as i32;
                let v_end = ((scene_br.y as f32) / v_interval).ceil() as i32 * v_interval as i32;
                let mut tick_y = v_start as f32;
                while tick_y <= v_end as f32 {
                    let screen_pt = preview_scene_to_screen(
                        ctx.scene_dimensions,
                        preview_rect,
                        kurbo::Point::new(scene_tl.x, tick_y as f64),
                        ctx.preview.viewport.preview_zoom,
                        ctx.preview.viewport.preview_pan,
                    );
                    if screen_pt.y >= v_ruler_rect.min.y && screen_pt.y <= v_ruler_rect.max.y {
                        let rel_y = screen_pt.y - v_ruler_rect.min.y;
                        let is_major = (tick_y as i32) % (v_interval as i32 * 5) == 0;
                        let tick_w = if is_major {
                            RULER_SIZE * 0.6
                        } else {
                            RULER_SIZE * 0.3
                        };
                        ui.painter().line_segment(
                            [
                                egui::pos2(v_ruler_rect.max.x, v_ruler_rect.min.y + rel_y),
                                egui::pos2(v_ruler_rect.max.x - tick_w, v_ruler_rect.min.y + rel_y),
                            ],
                            egui::Stroke::new(
                                STROKE_WIDTH,
                                if is_major {
                                    ruler_label_color
                                } else {
                                    ruler_tick_color
                                },
                            ),
                        );
                        if is_major {
                            ui.painter().text(
                                egui::pos2(
                                    v_ruler_rect.min.x + RULER_SIZE * 0.3,
                                    v_ruler_rect.min.y + rel_y,
                                ),
                                egui::Align2::CENTER_CENTER,
                                format!("{}", tick_y as i32),
                                TextRole::Micro.font_id(),
                                ruler_text_color,
                            );
                        }
                    }
                    tick_y += v_interval;
                }

                // ── Ruler drag interaction ──
                let ruler_drag_id = ui.id().with("guide_ruler_drag_v2");
                let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());
                let h_ruler_resp = ui.allocate_rect(h_ruler_rect, egui::Sense::drag());
                let v_ruler_resp = ui.allocate_rect(v_ruler_rect, egui::Sense::drag());

                if h_ruler_resp.drag_started() {
                    if let Some(mouse) = raw_pointer_pos {
                        let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                        ui.data_mut(|d| {
                            d.insert_temp(ruler_drag_id, Some((false, scene.y as f32, mouse)))
                        });
                    }
                }
                if v_ruler_resp.drag_started() {
                    if let Some(mouse) = raw_pointer_pos {
                        let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                        ui.data_mut(|d| {
                            d.insert_temp(ruler_drag_id, Some((true, scene.x as f32, mouse)))
                        });
                    }
                }

                let ruler_drag_active: Option<(bool, f32, egui::Pos2)> =
                    ui.data(|d| d.get_temp(ruler_drag_id));
                if let Some((is_vertical, _start_val, _start_pos)) = ruler_drag_active {
                    if let Some(mouse) = raw_pointer_pos {
                        let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                        let guide_color = status::WARNING;
                        if is_vertical {
                            let ghost_screen = ctx.preview_scene_to_screen(
                                preview_rect,
                                kurbo::Point::new(scene.x, 0.0),
                            );
                            if ghost_screen.x >= preview_rect.min.x
                                && ghost_screen.x <= preview_rect.max.x
                            {
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(ghost_screen.x, preview_rect.min.y),
                                        egui::pos2(ghost_screen.x, preview_rect.max.y),
                                    ],
                                    egui::Stroke::new(STROKE_WIDTH, guide_color),
                                );
                            }
                        } else {
                            let ghost_screen = ctx.preview_scene_to_screen(
                                preview_rect,
                                kurbo::Point::new(0.0, scene.y),
                            );
                            if ghost_screen.y >= preview_rect.min.y
                                && ghost_screen.y <= preview_rect.max.y
                            {
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(preview_rect.min.x, ghost_screen.y),
                                        egui::pos2(preview_rect.max.x, ghost_screen.y),
                                    ],
                                    egui::Stroke::new(STROKE_WIDTH, guide_color),
                                );
                            }
                        }
                    }
                }

                let pointer_released = ui.input(|i| i.pointer.any_released());
                if let Some((is_vertical, _start_val, start_pos)) = ruler_drag_active {
                    if pointer_released
                        || h_ruler_resp.drag_stopped()
                        || v_ruler_resp.drag_stopped()
                    {
                        if let Some(mouse) = raw_pointer_pos {
                            // Require at least 5 px of movement to avoid accidental clicks
                            let dragged_far_enough = (mouse - start_pos).length() >= 5.0;
                            if dragged_far_enough && preview_rect.contains(mouse) {
                                let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
                                if is_vertical {
                                    ctx.preview.guides.vertical_guides.push(scene.x as f32);
                                } else {
                                    ctx.preview.guides.horizontal_guides.push(scene.y as f32);
                                }
                            }
                        }
                        ui.data_mut(|d| d.remove::<Option<(bool, f32, egui::Pos2)>>(ruler_drag_id));
                    }
                }

                // ── Draw existing guides ──
                if ctx.preview.overlay.show_guides {
                    let guide_color = status::WARNING;
                    for &guide_y in &ctx.preview.guides.horizontal_guides {
                        let screen_pt = ctx.preview_scene_to_screen(
                            preview_rect,
                            kurbo::Point::new(0.0, guide_y as f64),
                        );
                        if screen_pt.y >= preview_rect.min.y && screen_pt.y <= preview_rect.max.y {
                            ui.painter().line_segment(
                                [
                                    egui::pos2(preview_rect.min.x, screen_pt.y),
                                    egui::pos2(preview_rect.max.x, screen_pt.y),
                                ],
                                egui::Stroke::new(STROKE_WIDTH, guide_color),
                            );
                        }
                    }
                    for &guide_x in &ctx.preview.guides.vertical_guides {
                        let screen_pt = ctx.preview_scene_to_screen(
                            preview_rect,
                            kurbo::Point::new(guide_x as f64, 0.0),
                        );
                        if screen_pt.x >= preview_rect.min.x && screen_pt.x <= preview_rect.max.x {
                            ui.painter().line_segment(
                                [
                                    egui::pos2(screen_pt.x, preview_rect.min.y),
                                    egui::pos2(screen_pt.x, preview_rect.max.y),
                                ],
                                egui::Stroke::new(STROKE_WIDTH, guide_color),
                            );
                        }
                    }
                }

                // ── Scroll zoom ──
                if response.hovered() {
                    let scroll = ui.input(|i| i.smooth_scroll_delta);
                    if scroll.y != 0.0 {
                        let zoom_factor = 1.0 + scroll.y * 0.001;
                        let new_zoom = (ctx.preview.viewport.preview_zoom * zoom_factor)
                            .clamp(preview_spatial::MIN_ZOOM, 10.0);
                        let prev_zoom = ctx.preview.viewport.preview_zoom;
                        if let Some(cursor) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                            let cursor_in_rect = preview_rect.contains(cursor);
                            if cursor_in_rect && prev_zoom > 0.01 {
                                let scene_at_cursor =
                                    ctx.preview_screen_to_scene(preview_rect, cursor);
                                let rel = cursor - preview_rect.center();
                                ctx.preview.viewport.preview_zoom = new_zoom;
                                let tx = preview::PreviewTransform::new(
                                    ctx.scene_dimensions,
                                    preview_rect,
                                    new_zoom,
                                    Vec2::ZERO,
                                );
                                let (new_scale, _) = tx.scale();
                                let new_pan = Vec2::new(
                                    (scene_at_cursor.x - rel.x as f64 * new_scale) as f32,
                                    (scene_at_cursor.y - rel.y as f64 * new_scale) as f32,
                                );
                                ctx.preview.viewport.preview_pan =
                                    ctx.clamp_pan(new_pan, preview_rect);
                                ctx.preview.status = format!(
                                    "Zoom: {:.0}%",
                                    ctx.preview.viewport.preview_zoom * 100.0
                                );
                            }
                        } else {
                            ctx.preview.viewport.preview_zoom = new_zoom;
                            ctx.preview.status =
                                format!("Zoom: {:.0}%", ctx.preview.viewport.preview_zoom * 100.0);
                        }
                    }
                }

                // ── Middle-click pan ──
                if ui.input(|i| i.pointer.middle_down()) {
                    if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                        if preview_rect.contains(mouse) {
                            let delta = ui.input(|i| i.pointer.delta());
                            if delta != Vec2::ZERO {
                                let tx = preview::PreviewTransform::new(
                                    ctx.scene_dimensions,
                                    preview_rect,
                                    ctx.preview.viewport.preview_zoom,
                                    Vec2::ZERO,
                                );
                                let (scale, _) = tx.scale();
                                let new_pan = Vec2::new(
                                    ctx.preview.viewport.preview_pan.x - delta.x * scale as f32,
                                    ctx.preview.viewport.preview_pan.y - delta.y * scale as f32,
                                );
                                ctx.preview.viewport.preview_pan =
                                    ctx.clamp_pan(new_pan, preview_rect);
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
                let wants_keyboard = ui.ctx().egui_wants_keyboard_input();
                let t_held = !wants_keyboard
                    && ui.input(|i| i.key_pressed(egui::Key::T) || i.key_down(egui::Key::T));

                let all_kf = if t_held {
                    let mut all_kf: Vec<f64> = if let Some(tl) = ctx.timeline {
                        tl.root_actor_labels()
                            .iter()
                            .flat_map(|label| {
                                tl.get_track(label)
                                    .map(animatix::timeline::collect_all_keyframe_times)
                                    .unwrap_or_default()
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    all_kf.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    all_kf.dedup_by(|a, b| (*a - *b).abs() < 0.001);
                    all_kf
                } else {
                    Vec::new()
                };
                if let Some(new_time) = ctx.preview.time_lens.update_and_show(
                    ui,
                    ctx.preview.playback.current_time_s(),
                    ctx.preview.playback.duration_s,
                    &all_kf,
                ) {
                    ctx.commands.push_back(PlaybackCommand::ScrubTo(new_time).into());
                }

                let is_dragging = !matches!(ctx.drag_state, DragState::None);
                crate::app::preview::gesture_router::GestureRouter::handle_preview_gestures(
                    ctx,
                    ui,
                    preview_rect,
                    &response,
                );

                let pointer_pos = ui
                    .ctx()
                    .input(|i| i.pointer.latest_pos())
                    .filter(|p| preview_rect.contains(*p));
                let scene_dimensions = ctx.scene_dimensions;
                let zoom = ctx.preview.viewport.preview_zoom;
                let pan = ctx.preview.viewport.preview_pan;
                let screen_to_scene = move |screen: egui::Pos2| {
                    preview_screen_to_scene(scene_dimensions, preview_rect, screen, zoom, pan)
                };

                if !ctx.selection.context_menu_open {
                    let unlocked_hit_regions: Vec<(String, kurbo::Rect)> = ctx
                        .hit_regions
                        .iter()
                        .filter(|(label, _)| {
                            !ctx.timeline
                                .and_then(|t| t.get_track(label))
                                .map(|tr| tr.locked)
                                .unwrap_or(false)
                        })
                        .cloned()
                        .collect();
                    selection::update_hover(
                        ctx.selection,
                        &unlocked_hit_regions,
                        pointer_pos,
                        screen_to_scene,
                        is_dragging,
                    );
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
                        kurbo::Point::new(0.0, 0.0),
                        preview_rect,
                        ctx.scene_dimensions,
                        preview_rect.size(),
                        ctx.preview.viewport.preview_zoom,
                        ctx.preview.viewport.preview_pan,
                    );
                    let bounds_br = preview::scene_to_screen(
                        kurbo::Point::new(
                            ctx.scene_dimensions.width as f64,
                            ctx.scene_dimensions.height as f64,
                        ),
                        preview_rect,
                        ctx.scene_dimensions,
                        preview_rect.size(),
                        ctx.preview.viewport.preview_zoom,
                        ctx.preview.viewport.preview_pan,
                    );
                    let bounds_screen =
                        egui::Rect::from_min_max(bounds_rect, bounds_br).intersect(preview_rect);
                    if bounds_screen.is_positive() {
                        ui.painter().rect_stroke(
                            bounds_screen,
                            0.0,
                            egui::Stroke::new(STROKE_WIDTH, border::HOVER),
                            egui::StrokeKind::Inside,
                        );
                    }
                }

                // ── Actor labels overlay ──
                if ctx.preview.overlay.show_actor_labels {
                    for (label, bounds) in ctx.hit_regions {
                        let center = preview::scene_to_screen(
                            kurbo::Point::new((bounds.x0 + bounds.x1) / 2.0, bounds.y0 - 4.0),
                            preview_rect,
                            ctx.scene_dimensions,
                            preview_rect.size(),
                            ctx.preview.viewport.preview_zoom,
                            ctx.preview.viewport.preview_pan,
                        );
                        ui.painter().text(
                            center,
                            egui::Align2::CENTER_BOTTOM,
                            label,
                            TextRole::Micro.font_id(),
                            text::MUTED,
                        );
                    }
                }

                // Draw grid overlay
                if ctx.preview.overlay.show_grid {
                    preview::grid::draw_grid(
                        ui.painter(),
                        ctx.scene_dimensions,
                        preview_rect,
                        ctx.preview.viewport.preview_zoom,
                        ctx.preview.viewport.preview_pan,
                        ctx.preview.overlay.grid_size,
                    );
                }

                // ── Layout debug overlay ──
                if ctx.debug_layout {
                    ctx.render_layout_debug(ui, preview_rect);
                }

                // ── Motion paths ──
                ctx.render_motion_paths(ui, preview_rect);

                // ── Draw snap indicator lines ──
                if let Some(color) = ctx.preview.snap.snap_line_color {
                    for &sy in &ctx.preview.snap.snap_lines_h {
                        let screen_pt = ctx.preview_scene_to_screen(
                            preview_rect,
                            kurbo::Point::new(0.0, sy as f64),
                        );
                        if screen_pt.y >= preview_rect.min.y && screen_pt.y <= preview_rect.max.y {
                            ui.painter().line_segment(
                                [
                                    egui::pos2(preview_rect.min.x, screen_pt.y),
                                    egui::pos2(preview_rect.max.x, screen_pt.y),
                                ],
                                egui::Stroke::new(STROKE_WIDTH, color),
                            );
                        }
                    }
                    for &sx in &ctx.preview.snap.snap_lines_v {
                        let screen_pt = ctx.preview_scene_to_screen(
                            preview_rect,
                            kurbo::Point::new(sx as f64, 0.0),
                        );
                        if screen_pt.x >= preview_rect.min.x && screen_pt.x <= preview_rect.max.x {
                            ui.painter().line_segment(
                                [
                                    egui::pos2(screen_pt.x, preview_rect.min.y),
                                    egui::pos2(screen_pt.x, preview_rect.max.y),
                                ],
                                egui::Stroke::new(STROKE_WIDTH, color),
                            );
                        }
                    }
                }

                ctx.render_preview_selection_overlay(ui, preview_rect, is_dragging);

                // ── File drop ──
                if response.hovered() {
                    let dropped_files = ui.input(|i| i.raw.dropped_files.clone());
                    for file in dropped_files {
                        if let Some(path) = file.path {
                            let ext = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            let path_str = path.to_string_lossy().to_string();

                            // .amx files: open directly instead of creating an actor
                            if ext == "amx" {
                                ctx.commands.push_back(DocumentCommand::OpenFile(path).into());
                                continue;
                            }

                            let drop_pos =
                                if let Some(mouse) = ui.ctx().input(|i| i.pointer.latest_pos()) {
                                    let scene = preview_screen_to_scene(
                                        ctx.scene_dimensions,
                                        preview_rect,
                                        mouse,
                                        ctx.preview.viewport.preview_zoom,
                                        ctx.preview.viewport.preview_pan,
                                    );
                                    [scene.x as f32, scene.y as f32]
                                } else {
                                    [
                                        ctx.scene_dimensions.width as f32 / 2.0,
                                        ctx.scene_dimensions.height as f32 / 2.0,
                                    ]
                                };
                            let label = crate::app::utils::labels::unique_label(
                                None,
                                if ext == "svg" { "svg" } else { "image" },
                            );
                            let (ty, props) = if ext == "svg" {
                                (
                                    "Svg".to_string(),
                                    vec![animatix_syntax::ast::Property {
                                        name: "url".into(),
                                        value: animatix_syntax::ast::Expr::Str(path_str),
                                        value_span: None,
                                        trailing_comment: None,
                                    }],
                                )
                            } else {
                                (
                                    "Image".to_string(),
                                    vec![animatix_syntax::ast::Property {
                                        name: "url".into(),
                                        value: animatix_syntax::ast::Expr::Str(path_str),
                                        value_span: None,
                                        trailing_comment: None,
                                    }],
                                )
                            };
                            ctx.commands.push_back(
                                ActorCommand::CreateActor {
                                    ty,
                                    label,
                                    position: drop_pos,
                                    props,
                                }
                                .into(),
                            );
                        }
                    }
                }

                // Inline text editor (double-click on text actors)
                ctx.render_inline_text_editor(ui, preview_rect);

                // Floating property cards for selected actors (hide when inline editing)
                if !is_dragging
                    && ctx.selected_actors.len() == 1
                    && ctx.preview.inline_edit.is_none()
                {
                    if let Some(actor) = ctx.selected_actors.iter().next() {
                        if let Some(props) = ctx.get_actor_props(actor) {
                            let screen_pos = preview::scene_to_screen(
                                kurbo::Point::new(
                                    props.position[0] as f64,
                                    props.position[1] as f64,
                                ),
                                preview_rect,
                                ctx.scene_dimensions,
                                preview_rect.size(),
                                ctx.preview.viewport.preview_zoom,
                                ctx.preview.viewport.preview_pan,
                            );
                            preview::property_popup::show_property_popup(
                                ui,
                                actor,
                                &props,
                                screen_pos,
                                ctx.commands,
                                is_dragging,
                                ctx.timeline,
                                ctx.preview.playback.current_time_s(),
                                ctx.scene_dimensions,
                                ctx.preview.viewport.preview_zoom,
                                preview_rect,
                                ctx.preview.viewport.preview_pan,
                            );
                        }
                    }
                }
            });
        });
}
