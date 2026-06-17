//! Grid overlay rendering for the preview canvas.

use crate::app::design_tokens::semantic::canvas::grid_line;
use crate::app::design_tokens::spatial::STROKE_WIDTH;
use animatix::timeline::SceneDimensions;
use egui::{Pos2, Stroke};

/// Draw a grid overlay on the preview canvas.
pub fn draw_grid(
    painter: &egui::Painter,
    scene_dimensions: SceneDimensions,
    preview_rect: egui::Rect,
    zoom: f32,
    pan: egui::Vec2,
    grid_size: f32,
) {
    let grid_color = grid_line();
    let tx = super::PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan);

    let scene_tl = tx.screen_to_scene(preview_rect.left_top());
    let scene_br = tx.screen_to_scene(preview_rect.right_bottom());
    let x0 = (scene_tl.x / grid_size as f64).floor() as i32 * grid_size as i32;
    let y0 = (scene_tl.y / grid_size as f64).floor() as i32 * grid_size as i32;
    let x1 = (scene_br.x / grid_size as f64).ceil() as i32 * grid_size as i32;
    let y1 = (scene_br.y / grid_size as f64).ceil() as i32 * grid_size as i32;

    let mut x = x0 as f32;
    while x <= x1 as f32 {
        let screen_pt = tx.scene_to_screen(kurbo::Point::new(x as f64, 0.0));
        if screen_pt.x >= preview_rect.min.x && screen_pt.x <= preview_rect.max.x {
            painter.line_segment(
                [Pos2::new(screen_pt.x, preview_rect.min.y), Pos2::new(screen_pt.x, preview_rect.max.y)],
                Stroke::new(STROKE_WIDTH, grid_color),
            );
        }
        x += grid_size;
    }
    let mut y = y0 as f32;
    while y <= y1 as f32 {
        let screen_pt = tx.scene_to_screen(kurbo::Point::new(0.0, y as f64));
        if screen_pt.y >= preview_rect.min.y && screen_pt.y <= preview_rect.max.y {
            painter.line_segment(
                [Pos2::new(preview_rect.min.x, screen_pt.y), Pos2::new(preview_rect.max.x, screen_pt.y)],
                Stroke::new(STROKE_WIDTH, grid_color),
            );
        }
        y += grid_size;
    }
}
