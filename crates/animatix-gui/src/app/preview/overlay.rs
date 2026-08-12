//! Unified overlay toggle system for the preview canvas.
//!
//! All preview overlays (grid, guides, labels, etc.) are controlled through
//! [`PreviewOverlay`] which lives in [`PreviewPaneState`](crate::app::PreviewPaneState).

use crate::app::preview::performance::PerformanceMetrics;

/// Toggle-able overlays for the preview canvas.
#[derive(Debug, Clone)]
pub struct PreviewOverlay {
    /// Show scene bounds outline.
    pub show_scene_bounds: bool,
    /// Show grid overlay.
    pub show_grid: bool,
    /// Show ruler guides (horizontal/vertical drag-from-ruler guides).
    pub show_guides: bool,
    /// Show actor name labels near selected actors.
    pub show_actor_labels: bool,
    /// Show snap guides during drag.
    pub show_snap_guides: bool,
    /// Show hover highlight around hovered actors.
    pub show_hover_highlight: bool,
    /// Show motion paths for position keyframes.
    pub show_motion_paths: bool,
    /// Show performance HUD overlay.
    pub show_performance_hud: bool,
    /// Grid size in pixels.
    pub grid_size: f32,
    /// Theme captured from the preview frame for drag/overlay rendering.
    pub(crate) current_theme: eparts::Theme,
}

impl PreviewOverlay {
    pub(crate) fn set_theme(&mut self, theme: eparts::Theme) {
        self.current_theme = theme;
    }

    pub(crate) fn current_theme(&self) -> eparts::Theme {
        self.current_theme
    }
}

impl Default for PreviewOverlay {
    fn default() -> Self {
        Self {
            show_scene_bounds: true,
            show_grid: true,
            show_guides: true,
            show_actor_labels: false,
            show_snap_guides: true,
            show_hover_highlight: true,
            show_motion_paths: true,
            show_performance_hud: false,
            grid_size: 20.0,
            current_theme: eparts::Theme::dark(),
        }
    }
}

/// Render the performance HUD in the top-right corner of the preview.
pub fn render_performance_hud(
    painter: &egui::Painter,
    theme: eparts::Theme,
    rect: egui::Rect,
    metrics: &PerformanceMetrics,
) {
    let hud_rect = egui::Rect::from_min_size(
        egui::Pos2::new(rect.right() - 220.0, rect.top() + 4.0),
        egui::Vec2::new(216.0, 110.0),
    );

    // Background
    painter.rect_filled(hud_rect, 6.0, theme.surface.base.linear_multiply(0.9));
    painter.rect_stroke(
        hud_rect,
        6.0,
        egui::Stroke::new(1.0, theme.border.default),
        egui::StrokeKind::Outside,
    );

    let text_color = theme.text.primary;
    let font = egui::FontId::monospace(11.0);
    let label_w = 90.0;
    let x = hud_rect.left() + 8.0;
    let line_h = 16.0;

    // Helper to draw a metric row
    let draw_row =
        |painter: &egui::Painter, y: f32, label: &str, value: &str, color: egui::Color32| {
            painter.text(
                egui::Pos2::new(x, y),
                egui::Align2::LEFT_TOP,
                label,
                font.clone(),
                theme.text.muted,
            );
            painter.text(
                egui::Pos2::new(x + label_w, y),
                egui::Align2::LEFT_TOP,
                value,
                font.clone(),
                color,
            );
        };

    let mut y = hud_rect.top() + 8.0;
    draw_row(painter, y, "FPS", &format!("{:.0}", metrics.fps), text_color);
    y += line_h;
    draw_row(painter, y, "Rebuild", &format!("{:.1} ms", metrics.rebuild_time_ms), text_color);
    y += line_h;
    draw_row(painter, y, "Render", &format!("{:.1} ms", metrics.render_time_ms), text_color);
    y += line_h;
    draw_row(painter, y, "GPU Mem", &format!("{:.0} MB", metrics.gpu_memory_mb), text_color);
    y += line_h;
    draw_row(
        painter,
        y,
        "Preview",
        if metrics.is_stale { "STALE" } else { "FRESH" },
        if metrics.is_stale {
            theme.status.warning
        } else {
            theme.status.success
        },
    );
    y += line_h;

    // Draw a mini FPS sparkline (last 30 frames)
    if metrics.fps_history.len() >= 2 {
        let spark_y = y + 4.0;
        let spark_h = 24.0;
        let spark_rect =
            egui::Rect::from_min_size(egui::Pos2::new(x, spark_y), egui::Vec2::new(200.0, spark_h));

        // Background
        painter.rect_filled(spark_rect, 2.0, egui::Color32::from_black_alpha(100));

        // Plot fps history
        let max_fps = metrics.fps_history.iter().cloned().fold(0.0, f64::max).max(1.0) as f32;
        let step_x = spark_rect.width() / (metrics.fps_history.len() - 1) as f32;

        let points: Vec<egui::Pos2> = metrics
            .fps_history
            .iter()
            .enumerate()
            .map(|(i, &f)| {
                let px = spark_rect.left() + i as f32 * step_x;
                let py = spark_rect.bottom() - (f as f32 / max_fps) * spark_h;
                egui::Pos2::new(px, py)
            })
            .collect();

        if points.len() >= 2 {
            painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, theme.status.success)));
        }
    }
}
