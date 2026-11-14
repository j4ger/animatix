//! Preview panel body.

use super::*;

mod interaction;
mod overlays;
mod rulers;

use interaction::*;
use overlays::*;
use rulers::*;

pub(crate) fn preview_panel_ui(ctx: &mut PreviewContext<'_>, ui: &mut egui::Ui) {
    let theme = eparts::theme(ui);
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::ZERO)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                let in_presentation = ctx.preview.playback.presentation_mode;
                let ruler_size = if in_presentation { 0.0 } else { RULER_SIZE };

                apply_fit_zoom_request(ctx);
                let (preview_rect, response) = allocate_canvas(ctx, ui, ruler_size);
                draw_canvas_background(ui, &theme, preview_rect, in_presentation);

                if !in_presentation {
                    draw_rulers(ctx, ui, preview_rect, &theme);
                    draw_existing_guides(ctx, ui, preview_rect, &theme);
                }

                handle_scroll_zoom(ctx, ui, preview_rect, &response);
                handle_middle_click_pan(ctx, ui, preview_rect);
                clear_frame_snap_state(ctx);

                let is_dragging = !matches!(ctx.drag_state, DragState::None);
                if !in_presentation {
                    handle_canvas_interaction(ctx, ui, preview_rect, &response, is_dragging);
                }

                ctx.render_preview_content(ui, preview_rect);

                if in_presentation {
                    handle_presentation_input(ctx, ui, preview_rect, &response);
                    return;
                }

                draw_preview_overlays(ctx, ui, preview_rect, &theme, is_dragging);
                handle_file_drop(ctx, ui, preview_rect, &response);
                show_selection_editors(ctx, ui, preview_rect, is_dragging);
            });
        });
}

fn apply_fit_zoom_request(ctx: &mut PreviewContext<'_>) {
    if ctx.preview.fit_zoom_requested {
        ctx.preview.fit_zoom_requested = false;
        ctx.preview.viewport.preview_zoom = 1.0;
        ctx.preview.viewport.preview_pan = Vec2::new(
            ctx.scene_dimensions.width as f32 / 2.0,
            ctx.scene_dimensions.height as f32 / 2.0,
        );
    }
}

fn allocate_canvas(
    ctx: &PreviewContext<'_>,
    ui: &mut egui::Ui,
    ruler_size: f32,
) -> (egui::Rect, egui::Response) {
    let avail_rect = ui.available_rect_before_wrap();
    let avail_size = avail_rect.size();
    let preview_available =
        Vec2::new((avail_size.x - ruler_size).max(10.0), (avail_size.y - ruler_size).max(10.0));
    let desired = fit_preview(ctx.scene_dimensions, preview_available);
    let total_size = desired + Vec2::new(ruler_size, ruler_size);

    let offset_x = ((avail_size.x - total_size.x) * 0.5).max(0.0);
    let offset_y = ((avail_size.y - total_size.y) * 0.5).max(0.0);
    let total_origin = egui::pos2(avail_rect.min.x + offset_x, avail_rect.min.y + offset_y);
    let total_rect = egui::Rect::from_min_size(total_origin, total_size);
    let preview_rect = egui::Rect::from_min_size(
        egui::pos2(total_rect.min.x + ruler_size, total_rect.min.y + ruler_size),
        desired,
    );
    let _ = ui.allocate_rect(avail_rect, egui::Sense::hover());
    let response = ui.allocate_rect(preview_rect, egui::Sense::click_and_drag());
    (preview_rect, response)
}

fn draw_canvas_background(
    ui: &mut egui::Ui,
    theme: &eparts::Theme,
    preview_rect: egui::Rect,
    in_presentation: bool,
) {
    if !in_presentation {
        ui.painter().rect_stroke(
            preview_rect,
            RADIUS_L,
            egui::Stroke::new(STROKE_WIDTH, theme.palette.border.default),
            egui::StrokeKind::Outside,
        );
        ui.painter().rect_filled(preview_rect, RADIUS_L, theme.palette.surface.base);
    } else {
        ui.painter().rect_filled(preview_rect, 0.0, theme.palette.surface.base);
    }
}
