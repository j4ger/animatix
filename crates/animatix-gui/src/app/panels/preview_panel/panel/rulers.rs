//! Preview panel body.

use super::*;

pub(super) fn draw_rulers(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    theme: &eparts::Theme,
) {
    let h_ruler_rect = horizontal_ruler_rect(preview_rect);
    let v_ruler_rect = vertical_ruler_rect(preview_rect);
    draw_ruler_corner(ui, theme, preview_rect);
    draw_horizontal_ruler(ctx, ui, preview_rect, theme);
    draw_vertical_ruler(ctx, ui, preview_rect, theme);
    handle_ruler_drag(ctx, ui, preview_rect, theme, h_ruler_rect, v_ruler_rect);
}

pub(super) fn horizontal_ruler_rect(preview_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(preview_rect.min.x, preview_rect.min.y - RULER_SIZE),
        Vec2::new(preview_rect.width(), RULER_SIZE),
    )
}

pub(super) fn vertical_ruler_rect(preview_rect: egui::Rect) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(preview_rect.min.x - RULER_SIZE, preview_rect.min.y),
        Vec2::new(RULER_SIZE, preview_rect.height()),
    )
}

pub(super) fn draw_ruler_corner(
    ui: &mut egui::Ui,
    theme: &eparts::Theme,
    preview_rect: egui::Rect,
) {
    let ruler_bg = theme.palette.surface.panel;
    let corner_rect = egui::Rect::from_min_size(
        egui::pos2(preview_rect.min.x - RULER_SIZE, preview_rect.min.y - RULER_SIZE),
        Vec2::new(RULER_SIZE, RULER_SIZE),
    );
    let ruler_stroke = egui::Stroke::new(STROKE_WIDTH, theme.palette.border.default);
    ui.painter().rect_filled(corner_rect, 0.0, ruler_bg);
    ui.painter()
        .rect_stroke(corner_rect, 0.0, ruler_stroke, egui::StrokeKind::Outside);
}

pub(super) fn draw_horizontal_ruler(
    ctx: &PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    theme: &eparts::Theme,
) {
    let ruler_bg = theme.palette.surface.panel;
    let ruler_tick_color = theme.palette.text.muted;
    let ruler_text_color = theme.palette.text.muted;
    let ruler_label_color = theme.palette.text.secondary;
    let ruler_stroke = egui::Stroke::new(STROKE_WIDTH, theme.palette.border.default);
    let h_ruler_rect = egui::Rect::from_min_size(
        egui::pos2(preview_rect.min.x, preview_rect.min.y - RULER_SIZE),
        Vec2::new(preview_rect.width(), RULER_SIZE),
    );
    let scene_tl = ctx.preview_screen_to_scene(preview_rect, preview_rect.left_top());
    let scene_br = ctx.preview_screen_to_scene(preview_rect, preview_rect.right_bottom());
    let visible_w = (scene_br.x - scene_tl.x) as f32;
    // Horizontal ruler
    ui.painter().rect_filled(h_ruler_rect, 0.0, ruler_bg);
    ui.painter()
        .rect_stroke(h_ruler_rect, 0.0, ruler_stroke, egui::StrokeKind::Outside);
    let h_interval = nice_tick_interval(visible_w, h_ruler_rect.width() / 60.0).max(1.0);
    let h_start = ((scene_tl.x as f32) / h_interval).floor() as i32 * h_interval as i32;
    let h_end = ((scene_br.x as f32) / h_interval).ceil() as i32 * h_interval as i32;
    let mut tick_x = h_start as f32;
    while tick_x <= h_end as f32 {
        let screen_pt =
            ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(tick_x as f64, scene_tl.y));
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
                    egui::pos2(h_ruler_rect.min.x + rel_x, h_ruler_rect.min.y + RULER_SIZE * 0.3),
                    egui::Align2::CENTER_CENTER,
                    format!("{}", tick_x as i32),
                    TextRole::Micro.font_id(),
                    ruler_text_color,
                );
            }
        }
        tick_x += h_interval;
    }
}

pub(super) fn draw_vertical_ruler(
    ctx: &PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    theme: &eparts::Theme,
) {
    let ruler_bg = theme.palette.surface.panel;
    let ruler_tick_color = theme.palette.text.muted;
    let ruler_text_color = theme.palette.text.muted;
    let ruler_label_color = theme.palette.text.secondary;
    let ruler_stroke = egui::Stroke::new(STROKE_WIDTH, theme.palette.border.default);
    let v_ruler_rect = egui::Rect::from_min_size(
        egui::pos2(preview_rect.min.x - RULER_SIZE, preview_rect.min.y),
        Vec2::new(RULER_SIZE, preview_rect.height()),
    );
    let scene_tl = ctx.preview_screen_to_scene(preview_rect, preview_rect.left_top());
    let scene_br = ctx.preview_screen_to_scene(preview_rect, preview_rect.right_bottom());
    let visible_h = (scene_br.y - scene_tl.y) as f32;
    // Vertical ruler
    ui.painter().rect_filled(v_ruler_rect, 0.0, ruler_bg);
    ui.painter()
        .rect_stroke(v_ruler_rect, 0.0, ruler_stroke, egui::StrokeKind::Outside);
    let v_interval = nice_tick_interval(visible_h, v_ruler_rect.height() / 60.0).max(1.0);
    let v_start = ((scene_tl.y as f32) / v_interval).floor() as i32 * v_interval as i32;
    let v_end = ((scene_br.y as f32) / v_interval).ceil() as i32 * v_interval as i32;
    let mut tick_y = v_start as f32;
    while tick_y <= v_end as f32 {
        let screen_pt =
            ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(scene_tl.x, tick_y as f64));
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
                    egui::pos2(v_ruler_rect.min.x + RULER_SIZE * 0.3, v_ruler_rect.min.y + rel_y),
                    egui::Align2::CENTER_CENTER,
                    format!("{}", tick_y as i32),
                    TextRole::Micro.font_id(),
                    ruler_text_color,
                );
            }
        }
        tick_y += v_interval;
    }
}

pub(super) fn handle_ruler_drag(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    theme: &eparts::Theme,
    h_ruler_rect: egui::Rect,
    v_ruler_rect: egui::Rect,
) {
    // ── Ruler drag interaction ──
    let ruler_drag_id = ui.id().with("guide_ruler_drag_v2");
    let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());
    let h_ruler_resp = ui.allocate_rect(h_ruler_rect, egui::Sense::drag());
    let v_ruler_resp = ui.allocate_rect(v_ruler_rect, egui::Sense::drag());

    if h_ruler_resp.drag_started() {
        if let Some(mouse) = raw_pointer_pos {
            let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
            ui.data_mut(|d| d.insert_temp(ruler_drag_id, Some((false, scene.y as f32, mouse))));
        }
    }
    if v_ruler_resp.drag_started() {
        if let Some(mouse) = raw_pointer_pos {
            let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
            ui.data_mut(|d| d.insert_temp(ruler_drag_id, Some((true, scene.x as f32, mouse))));
        }
    }

    let ruler_drag_active: Option<(bool, f32, egui::Pos2)> = ui.data(|d| d.get_temp(ruler_drag_id));
    if let Some((is_vertical, _start_val, _start_pos)) = ruler_drag_active {
        if let Some(mouse) = raw_pointer_pos {
            let scene = ctx.preview_screen_to_scene(preview_rect, mouse);
            let guide_color = theme.palette.status.warning;
            if is_vertical {
                let ghost_screen =
                    ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(scene.x, 0.0));
                if ghost_screen.x >= preview_rect.min.x && ghost_screen.x <= preview_rect.max.x {
                    ui.painter().line_segment(
                        [
                            egui::pos2(ghost_screen.x, preview_rect.min.y),
                            egui::pos2(ghost_screen.x, preview_rect.max.y),
                        ],
                        egui::Stroke::new(STROKE_WIDTH, guide_color),
                    );
                }
            } else {
                let ghost_screen =
                    ctx.preview_scene_to_screen(preview_rect, kurbo::Point::new(0.0, scene.y));
                if ghost_screen.y >= preview_rect.min.y && ghost_screen.y <= preview_rect.max.y {
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
        if pointer_released || h_ruler_resp.drag_stopped() || v_ruler_resp.drag_stopped() {
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
}
