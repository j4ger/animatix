//! Graph Editor (F-Curve) — read-only view for the Inspector.
//!
//! Multi-property graph editor with color-coded curves. The sampling and
//! drawing math is shared with the interactive Curves panel via
//! [`crate::app::panels::curve_plot`]; this module only owns the Inspector's
//! fixed-height frame and legend chrome.

use animatix::timeline::AnimationTrack;
use egui::{Sense, Vec2};

use crate::app::commands::ActionQueue;
use crate::app::design_tokens::spatial::{RADIUS_M, STROKE_WIDTH, spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::panels::curve_plot;

/// Render a multi-property F-curve graph.
pub fn render_multi_fcurve(
    ui: &mut egui::Ui,
    track: &AnimationTrack,
    duration_s: f64,
    current_time_s: f64,
    _commands: &mut ActionQueue,
) {
    let sp = spatial(ui);
    let theme = eparts::theme(ui);
    let available = ui.available_width();
    let height = 160.0f32;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(available, height), Sense::hover());
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, RADIUS_M, theme.palette.surface.base);
    painter.rect_stroke(
        rect,
        RADIUS_M,
        egui::Stroke::new(STROKE_WIDTH, theme.palette.border.default),
        egui::StrokeKind::Outside,
    );

    let curves = curve_plot::collect_curves(track, theme);

    if curves.is_empty() {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "No keyframes to graph",
            TextRole::BodyS.font_id(),
            theme.palette.text.muted,
        );
        return;
    }

    // Legend with visibility toggles
    let visibility_id = ui.id().with("graph_visibility");
    let mut visibility = curve_plot::legend_visibility(ui, visibility_id, &curves);
    let legend_height = 18.0f32;
    let legend_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + sp.base.space_3, rect.min.y + sp.base.space_2),
        egui::pos2(rect.max.x - sp.base.space_3, rect.min.y + sp.base.space_2 + legend_height),
    );
    curve_plot::draw_legend(ui, legend_rect, &curves, &mut visibility, theme);
    ui.data_mut(|d| d.insert_temp(visibility_id, visibility.clone()));

    // Plot area
    let plot_rect = egui::Rect::from_min_max(
        egui::pos2(rect.min.x + sp.base.space_3, legend_rect.max.y + sp.base.space_2),
        egui::pos2(rect.max.x - sp.base.space_3, rect.max.y - sp.base.space_2),
    );

    let visible_curves: Vec<&curve_plot::CurveChannel> =
        curves.iter().filter(|c| *visibility.get(&c.label).unwrap_or(&true)).collect();
    if visible_curves.is_empty() {
        painter.text(
            plot_rect.center(),
            egui::Align2::CENTER_CENTER,
            "All curves hidden",
            TextRole::BodyS.font_id(),
            theme.palette.text.muted,
        );
        return;
    }

    let (min_val, max_val) = curve_plot::value_window(&visible_curves);
    let axis = curve_plot::Axis::new(plot_rect, duration_s, min_val, max_val);

    curve_plot::draw_value_grid(&painter, &axis, theme);
    for curve in &visible_curves {
        curve_plot::draw_curve(&painter, &axis, curve, current_time_s, theme);
    }
    curve_plot::draw_playhead(&painter, &axis, current_time_s, theme);

    // Hover: change cursor
    if response.hovered() {
        ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::Crosshair);
    }
}
