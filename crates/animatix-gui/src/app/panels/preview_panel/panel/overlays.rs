//! Preview panel body.

use super::*;

pub(super) fn draw_preview_overlays(
    ctx: &mut PreviewContext<'_>,
    ui: &mut egui::Ui,
    preview_rect: egui::Rect,
    theme: &eparts::Theme,
    _is_dragging: bool,
) {
    let overlay_tx = ctx.preview_transform(preview_rect);
    // ── Scene bounds overlay ──
    if ctx.preview.overlay.show_scene_bounds {
        let ops = crate::app::preview::overlay_ops::scene_bounds_ops(theme, overlay_tx);
        crate::app::preview::overlay_ops::execute_overlay_ops(ui.painter(), &ops, &overlay_tx);
    }

    // ── Actor labels overlay ──
    if ctx.preview.overlay.show_actor_labels {
        let ops =
            crate::app::preview::overlay_ops::actor_label_ops(theme, ctx.hit_regions, overlay_tx);
        crate::app::preview::overlay_ops::execute_overlay_ops(ui.painter(), &ops, &overlay_tx);
    }

    // Draw grid overlay
    if ctx.preview.overlay.show_grid {
        let ops = crate::app::preview::overlay_ops::grid_ops(
            theme,
            overlay_tx,
            ctx.preview.overlay.grid_size,
        );
        crate::app::preview::overlay_ops::execute_overlay_ops(ui.painter(), &ops, &overlay_tx);
    }

    // ── Layout debug overlay ──
    if ctx.debug_layout {
        ctx.render_layout_debug(ui, preview_rect);
    }

    // ── Motion paths ──
    ctx.render_motion_paths(ui, preview_rect);

    // ── Draw snap indicator lines ──
    if let Some(color) = ctx.preview.snap.snap_line_color {
        let ops = crate::app::preview::overlay_ops::snap_guide_ops(
            color,
            &ctx.preview.snap.snap_lines_h,
            &ctx.preview.snap.snap_lines_v,
            overlay_tx,
        );
        crate::app::preview::overlay_ops::execute_overlay_ops(ui.painter(), &ops, &overlay_tx);
    }

    ctx.render_review_selection_overlay(ui, preview_rect);
}
