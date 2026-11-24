//! Preview panel body.

use super::*;

mod interaction;
mod overlays;
mod rulers;

use interaction::*;
use overlays::*;
use rulers::*;

pub(crate) fn preview_panel_ui(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    shortcuts: &crate::app::interaction::keyboard::ShortcutRegistry,
) {
    let theme = eparts::theme(ui);
    let in_presentation = ctx.preview.playback.presentation_mode;
    let sp = crate::app::design_tokens::spatial::spatial(ui);

    if !in_presentation {
        let bar_frame = egui::Frame::new()
            .fill(theme.palette.surface.panel)
            .inner_margin(egui::Margin::symmetric(sp.base.space_5 as i8, 0));
        let height = sp.base.row_l + sp.base.space_5;
        egui::Panel::top("preview_header")
            .exact_size(height)
            .frame(bar_frame)
            .show(ui, |ui| ui.horizontal_centered(|ui| super::hud::preview_header_ui(ctx, ui)));
        egui::Panel::bottom("preview_transport")
            .exact_size(height)
            .frame(bar_frame)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    crate::app::shell::transport::transport_ui(
                        ui,
                        ctx.preview,
                        ctx.commands,
                        shortcuts,
                    );
                });
            });
    }

    // Frame the editor matte independently of the rendered image and its rulers.
    let stage = if in_presentation {
        egui::Frame::new()
    } else {
        egui::Frame::new()
            .outer_margin(egui::Margin::symmetric(sp.base.space_4 as i8, sp.base.space_2 as i8))
            .inner_margin(egui::Margin::same(sp.base.space_4 as i8))
            .corner_radius(crate::app::design_tokens::spatial::RADIUS_L)
            .stroke(egui::Stroke::new(STROKE_WIDTH, theme.palette.border.subtle))
            .shadow(theme.elevation_raised())
    };
    stage.fill(theme.palette.surface.base).show(ui, |ui| {
        ui.vertical(|ui| {
            let ruler_inset = if in_presentation {
                Vec2::ZERO
            } else {
                ruler_inset(ui)
            };

            apply_fit_zoom_request(ctx);
            let (preview_rect, response) = allocate_canvas(ctx, ui, ruler_inset);
            draw_canvas_background(ui, &theme, preview_rect, in_presentation);

            handle_scroll_zoom(ctx, ui, preview_rect, &response);
            handle_middle_click_pan(ctx, ui, preview_rect);
            clear_frame_snap_state(ctx);
            let guide_in_progress = if in_presentation {
                None
            } else {
                draw_rulers(ctx, ui, preview_rect, ruler_inset, &theme)
            };

            let is_dragging = ctx.selection.marquee_start.is_some();
            if !in_presentation {
                handle_canvas_interaction(ctx, ui, preview_rect, &response, is_dragging);
            }

            ctx.render_preview_content(ui, preview_rect);

            if in_presentation {
                handle_presentation_input(ctx, ui, preview_rect, &response);
                return;
            }

            // The preview image is opaque. Guides must be painted after it.
            draw_guides(ctx, ui, preview_rect, &theme, guide_in_progress);
            draw_preview_overlays(ctx, ui, preview_rect, &theme, is_dragging);
            handle_file_drop(ctx, ui, preview_rect, &response);
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
    ruler_inset: Vec2,
) -> (egui::Rect, egui::Response) {
    let avail_rect = ui.available_rect_before_wrap();
    let avail_size = avail_rect.size();
    let preview_available = (avail_size - ruler_inset).max(Vec2::splat(10.0));
    let desired = fit_preview(ctx.scene_dimensions, preview_available);
    let total_size = desired + ruler_inset;

    let offset_x = ((avail_size.x - total_size.x) * 0.5).max(0.0);
    let offset_y = ((avail_size.y - total_size.y) * 0.5).max(0.0);
    let total_origin = egui::pos2(avail_rect.min.x + offset_x, avail_rect.min.y + offset_y);
    let total_rect = egui::Rect::from_min_size(total_origin, total_size);
    let preview_rect = egui::Rect::from_min_size(total_rect.min + ruler_inset, desired);
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
        let shadow = theme.elevation.raised;
        ui.painter().add(shadow.as_shape(preview_rect, egui::CornerRadius::ZERO));
        ui.painter().rect_stroke(
            preview_rect,
            0.0,
            egui::Stroke::new(STROKE_WIDTH, theme.palette.border.default),
            egui::StrokeKind::Outside,
        );
        ui.painter().rect_filled(preview_rect, 0.0, theme.palette.surface.base);
    } else {
        ui.painter().rect_filled(preview_rect, 0.0, theme.palette.surface.base);
    }
}

#[cfg(test)]
mod tests;
