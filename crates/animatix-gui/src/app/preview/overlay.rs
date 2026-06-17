//! Unified overlay toggle system for the preview canvas.
//!
//! All preview overlays (grid, guides, labels, etc.) are controlled through
//! [`PreviewOverlay`] which lives in [`PreviewPaneState`](crate::app::PreviewPaneState).

use crate::app::design_tokens::semantic::accent::PRIMARY as ACCENT_BLUE;
use crate::app::design_tokens::semantic::border::DEFAULT as BORDER;
use crate::app::design_tokens::semantic::status::SUCCESS as GREEN;
use crate::app::design_tokens::semantic::status::WARNING as AMBER;
use crate::app::design_tokens::semantic::surface::BASE as BG_BASE;
use crate::app::design_tokens::semantic::text::MUTED as TEXT_MUTED;
use crate::app::design_tokens::semantic::text::PRIMARY as TEXT_PRIMARY;
use crate::app::preview::performance::PerformanceMetrics;
use animatix::timeline::SceneDimensions;
use egui::Color32;

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
        }
    }
}

/// Render the performance HUD in the top-right corner of the preview.
pub fn render_performance_hud(
    painter: &egui::Painter,
    rect: egui::Rect,
    metrics: &PerformanceMetrics,
) {
    let hud_rect = egui::Rect::from_min_size(
        egui::Pos2::new(rect.right() - 220.0, rect.top() + 4.0),
        egui::Vec2::new(216.0, 110.0),
    );

    // Background
    painter.rect_filled(hud_rect, 6.0, BG_BASE.linear_multiply(0.9));
    painter.rect_stroke(hud_rect, 6.0, egui::Stroke::new(1.0, BORDER), egui::StrokeKind::Outside);

    let text_color = TEXT_PRIMARY;
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
                TEXT_MUTED,
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
        if metrics.is_stale { AMBER } else { GREEN },
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
            painter.add(egui::Shape::line(points, egui::Stroke::new(1.5, GREEN)));
        }
    }
}

/// Render layout debug overlay showing container bounds, slot outlines, and sizes.
///
/// `size` is the full width/height (position is center). The function divides
/// by 2 internally to compute top-left/bottom-right corners.
pub fn render_layout_debug(
    painter: &egui::Painter,
    timeline: &animatix::timeline::Timeline,
    time_ms: u64,
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    zoom: f32,
    pan: egui::Vec2,
    draw_spacing: bool,
) {
    // Build a preview transform for coordinate conversion
    let tx = crate::app::preview::PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan);

    // Container color (blue-ish)
    let container_color = ACCENT_BLUE;
    // Child slot color (amber)
    let slot_color = Color32::from_rgba_premultiplied(AMBER.r(), AMBER.g(), AMBER.b(), 150);
    // Size label color
    let size_color = AMBER;
    // Spacing region color (semi-transparent red)
    let spacing_color = egui::Color32::from_rgba_premultiplied(200, 80, 80, 60);

    // Iterate over all container metadata entries
    for (container_label, metadata) in timeline.container_metadata() {
        let Some(track) = timeline.get_track(container_label) else {
            continue;
        };

        // Get container position (center) and size (full width/height)
        let pos = track.position.as_ref().map(|p| p.evaluate(time_ms)).unwrap_or([0.0, 0.0]);
        let size = track.size.as_ref().map(|s| s.evaluate(time_ms)).unwrap_or([0.0, 0.0]);
        let half_w = size[0] / 2.0;
        let half_h = size[1] / 2.0;

        // Container bounds: from center to top-left/bottom-right corners
        let container_tl = tx
            .scene_to_screen(kurbo::Point::new((pos[0] - half_w) as f64, (pos[1] - half_h) as f64));
        let container_br = tx
            .scene_to_screen(kurbo::Point::new((pos[0] + half_w) as f64, (pos[1] + half_h) as f64));
        let container_rect = egui::Rect::from_min_max(container_tl, container_br);

        // Skip if the container is off-screen
        if !container_rect.intersects(preview_rect) {
            continue;
        }

        // Draw container outline
        painter.rect_stroke(
            container_rect,
            0.0,
            egui::Stroke::new(1.5, container_color),
            egui::StrokeKind::Outside,
        );

        // Draw container type label
        let kind_str = format!("{:?}", metadata.layout_type);
        painter.text(
            egui::Pos2::new(container_rect.left() + 2.0, container_rect.top() + 2.0),
            egui::Align2::LEFT_TOP,
            format!("{} ({})", container_label, kind_str),
            egui::FontId::monospace(10.0),
            container_color,
        );

        // Draw children layout slots
        let layout_children = timeline.layout_children_for(container_label);
        for child in &layout_children {
            let Some(child_track) = timeline.get_track(&child.label) else {
                continue;
            };
            let child_pos =
                child_track.position.as_ref().map(|p| p.evaluate(time_ms)).unwrap_or([0.0, 0.0]);
            let child_size =
                child_track.size.as_ref().map(|s| s.evaluate(time_ms)).unwrap_or([0.0, 0.0]);

            let child_tl = tx.scene_to_screen(kurbo::Point::new(
                (child_pos[0] - child_size[0] / 2.0) as f64,
                (child_pos[1] - child_size[1] / 2.0) as f64,
            ));
            let child_br = tx.scene_to_screen(kurbo::Point::new(
                (child_pos[0] + child_size[0] / 2.0) as f64,
                (child_pos[1] + child_size[1] / 2.0) as f64,
            ));
            let child_rect = egui::Rect::from_min_max(child_tl, child_br);

            // Draw child slot outline
            painter.rect_stroke(
                child_rect,
                0.0,
                egui::Stroke::new(1.0, slot_color),
                egui::StrokeKind::Outside,
            );

            // Draw intrinsic size label
            let layout_s = child_track.layout_size_get(time_ms).unwrap_or([0.0, 0.0]);
            painter.text(
                egui::Pos2::new(child_rect.left() + 2.0, child_rect.bottom() - 12.0),
                egui::Align2::LEFT_BOTTOM,
                format!("{:.0}×{:.0}", layout_s[0], layout_s[1]),
                egui::FontId::monospace(9.0),
                size_color,
            );
        }

        // Draw gap/padding if spacing debug is on
        if draw_spacing {
            // Determine layout axis
            let is_row = metadata.layout_type == animatix::timeline::LayoutType::Row;
            let gap = metadata.gap;

            // Collect ordered children with their positions and sizes
            let children_ordered: Vec<([f32; 2], [f32; 2])> = layout_children
                .iter()
                .filter_map(|child| {
                    let track = timeline.get_track(&child.label)?;
                    let child_pos =
                        track.position.as_ref().map(|p| p.evaluate(time_ms)).unwrap_or([0.0, 0.0]);
                    let child_size =
                        track.size.as_ref().map(|s| s.evaluate(time_ms)).unwrap_or([0.0, 0.0]);
                    Some((child_pos, child_size))
                })
                .collect();

            if children_ordered.len() >= 2 && gap > 0.0 {
                for pair in children_ordered.windows(2) {
                    let (pos_a, size_a) = pair[0];
                    let (pos_b, size_b) = pair[1];

                    // Compute gap center and size along the layout axis
                    let gap_center;
                    let gap_size;
                    if is_row {
                        // Right edge of child a = center.x + width/2
                        // Left  edge of child b = center.x - width/2
                        let right_a = pos_a[0] + size_a[0] / 2.0;
                        let left_b = pos_b[0] - size_b[0] / 2.0;
                        gap_center = (right_a + left_b) / 2.0;
                        gap_size = (left_b - right_a).abs().max(1.0);
                    } else {
                        // Bottom edge of child a = center.y + height/2
                        // Top    edge of child b = center.y - height/2
                        let bottom_a = pos_a[1] + size_a[1] / 2.0;
                        let top_b = pos_b[1] - size_b[1] / 2.0;
                        gap_center = (bottom_a + top_b) / 2.0;
                        gap_size = (top_b - bottom_a).abs().max(1.0);
                    }

                    if gap_size > 0.0 {
                        if is_row {
                            let gap_tl = tx.scene_to_screen(kurbo::Point::new(
                                (gap_center - gap_size / 2.0) as f64,
                                (pos_a[1] - size_a[1] / 2.0) as f64,
                            ));
                            let gap_br = tx.scene_to_screen(kurbo::Point::new(
                                (gap_center + gap_size / 2.0) as f64,
                                (pos_a[1] + size_a[1] / 2.0) as f64,
                            ));
                            let gap_rect = egui::Rect::from_min_max(gap_tl, gap_br);
                            painter.rect_filled(gap_rect, 0.0, spacing_color);
                        } else {
                            let gap_tl = tx.scene_to_screen(kurbo::Point::new(
                                (pos_a[0] - size_a[0] / 2.0) as f64,
                                (gap_center - gap_size / 2.0) as f64,
                            ));
                            let gap_br = tx.scene_to_screen(kurbo::Point::new(
                                (pos_a[0] + size_a[0] / 2.0) as f64,
                                (gap_center + gap_size / 2.0) as f64,
                            ));
                            let gap_rect = egui::Rect::from_min_max(gap_tl, gap_br);
                            painter.rect_filled(gap_rect, 0.0, spacing_color);
                        }
                    }
                }
            }
        }
    }
}
