use super::DEFAULT_PREVIEW_SIZE;
use animatix::timeline::SceneDimensions;
use egui::{Color32, Pos2, Stroke, Vec2};

// ─── Drag State ─────────────────────────────────────────────────────────────

/// Tracks the current drag interaction on the preview canvas.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(super) enum DragState {
    None,
    Move {
        actor: String,
        start_scene: kurbo::Point,
        start_position: [f32; 2],
    },
    Scale {
        actor: String,
        handle: usize,
        start_scene: kurbo::Point,
        start_size: [f32; 2],
    },
    Rotate {
        actor: String,
        start_scene: kurbo::Point,
        start_rotation: f32,
    },
}

// ─── Coordinate Mapping ─────────────────────────────────────────────────────

/// Convert scene coordinates to screen coordinates for the preview canvas.
fn scene_to_screen(
    scene_pos: kurbo::Point,
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    desired: Vec2,
) -> Pos2 {
    let scale_x = desired.x as f64 / scene_dimensions.width as f64;
    let scale_y = desired.y as f64 / scene_dimensions.height as f64;
    Pos2::new(
        (preview_rect.min.x as f64 + scene_pos.x * scale_x) as f32,
        (preview_rect.min.y as f64 + scene_pos.y * scale_y) as f32,
    )
}

// ─── Selection Bounds ───────────────────────────────────────────────────────

/// Compute the screen-space bounding box for the selected actor.
pub(super) fn selection_screen_rect(
    selected_actor: &str,
    hit_regions: &[(String, kurbo::Rect)],
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    desired: Vec2,
) -> Option<egui::Rect> {
    let (_, bounds) = hit_regions.iter().find(|(l, _)| l == selected_actor)?;
    let top_left = scene_to_screen(
        kurbo::Point::new(bounds.x0, bounds.y0),
        preview_rect,
        scene_dimensions,
        desired,
    );
    let bottom_right = scene_to_screen(
        kurbo::Point::new(bounds.x1, bounds.y1),
        preview_rect,
        scene_dimensions,
        desired,
    );
    Some(egui::Rect::from_min_max(top_left, bottom_right))
}

// ─── Selection Overlay ──────────────────────────────────────────────────────

const SELECTION_COLOR: Color32 = Color32::from_rgb(84, 110, 255);
const HANDLE_SIZE: f32 = 6.0;
pub(super) const ROTATION_OFFSET: f32 = 20.0;
pub(super) const ROTATION_RADIUS: f32 = 4.0;

/// Draw the selection bounding box, scale handles, and rotation handle.
pub(super) fn draw_selection_overlay(
    painter: &egui::Painter,
    sel_rect: egui::Rect,
    is_dragging: bool,
) {
    // Bounding box outline
    let stroke = if is_dragging {
        Stroke::new(
            1.5,
            Color32::from_rgba_unmultiplied(84, 110, 255, 140),
        )
    } else {
        Stroke::new(1.5, SELECTION_COLOR)
    };
    painter.rect_stroke(sel_rect, 0.0, stroke, egui::StrokeKind::Outside);

    // Dashed overlay during drag
    if is_dragging {
        let dash_len = 6.0;
        let gap_len = 4.0;
        let dash_color = Color32::from_rgba_unmultiplied(255, 255, 255, 80);
        let dash_stroke = Stroke::new(1.0, dash_color);
        let corners = [
            sel_rect.left_top(),
            sel_rect.right_top(),
            sel_rect.right_bottom(),
            sel_rect.left_bottom(),
        ];
        for i in 0..4 {
            let start = corners[i];
            let end = corners[(i + 1) % 4];
            let total = start.distance(end);
            let mut pos = 0.0;
            while pos < total {
                let t0 = pos / total;
                let t1 = ((pos + dash_len).min(total)) / total;
                let p0 = Pos2::new(
                    start.x + (end.x - start.x) * t0,
                    start.y + (end.y - start.y) * t0,
                );
                let p1 = Pos2::new(
                    start.x + (end.x - start.x) * t1,
                    start.y + (end.y - start.y) * t1,
                );
                painter.line_segment([p0, p1], dash_stroke);
                pos += dash_len + gap_len;
            }
        }
    }

    // 8 scale handles (corners + edge midpoints)
    let handle_positions = scale_handle_positions(sel_rect);
    for pos in &handle_positions {
        let handle_rect =
            egui::Rect::from_center_size(*pos, Vec2::new(HANDLE_SIZE, HANDLE_SIZE));
        painter.rect_filled(handle_rect, 1.0, Color32::WHITE);
        painter.rect_stroke(
            handle_rect,
            1.0,
            Stroke::new(1.0, SELECTION_COLOR),
            egui::StrokeKind::Outside,
        );
    }

    // Rotation handle: circle connected to top-center by line
    let top_center = Pos2::new(sel_rect.center().x, sel_rect.top());
    let rot_center = Pos2::new(top_center.x, top_center.y - ROTATION_OFFSET);
    painter.line_segment([top_center, rot_center], Stroke::new(1.0, SELECTION_COLOR));
    painter.circle_filled(rot_center, ROTATION_RADIUS, Color32::WHITE);
    painter.circle_stroke(
        rot_center,
        ROTATION_RADIUS,
        Stroke::new(1.0, SELECTION_COLOR),
    );
}

/// Returns the 8 scale handle center positions: 4 corners + 4 edge midpoints.
pub(super) fn scale_handle_positions(sel_rect: egui::Rect) -> [Pos2; 8] {
    [
        // Corners
        sel_rect.left_top(),
        sel_rect.right_top(),
        sel_rect.right_bottom(),
        sel_rect.left_bottom(),
        // Edge midpoints
        Pos2::new(sel_rect.center().x, sel_rect.top()),
        Pos2::new(sel_rect.right(), sel_rect.center().y),
        Pos2::new(sel_rect.center().x, sel_rect.bottom()),
        Pos2::new(sel_rect.left(), sel_rect.center().y),
    ]
}

// ─── Preview Helpers ────────────────────────────────────────────────────────

pub(super) fn fit_preview(dimensions: SceneDimensions, available: Vec2) -> Vec2 {
    let aspect = if dimensions.width == 0 || dimensions.height == 0 {
        DEFAULT_PREVIEW_SIZE.width as f32 / DEFAULT_PREVIEW_SIZE.height as f32
    } else {
        dimensions.width as f32 / dimensions.height as f32
    };
    let width_limited_height = available.x / aspect;
    if width_limited_height <= available.y {
        Vec2::new(available.x, width_limited_height)
    } else {
        Vec2::new(available.y * aspect, available.y)
    }
}

pub(super) fn timeline_fraction(current_time_s: f64, duration_s: f64) -> f32 {
    (current_time_s / duration_s.max(0.1)).clamp(0.0, 1.0) as f32
}

pub(super) fn time_from_pointer_x(rect: egui::Rect, pointer_x: f32, duration_s: f64) -> f64 {
    let width = rect.width().max(1.0);
    let normalized = ((pointer_x - rect.left()) / width).clamp(0.0, 1.0) as f64;
    normalized * duration_s.max(0.1)
}

pub(super) fn timeline_tick_times(duration_s: f64) -> Vec<f64> {
    let duration_s = duration_s.max(0.1);
    let step = if duration_s <= 2.0 {
        0.25
    } else if duration_s <= 5.0 {
        0.5
    } else if duration_s <= 15.0 {
        1.0
    } else if duration_s <= 45.0 {
        5.0
    } else {
        10.0
    };

    let mut ticks = Vec::new();
    let mut tick = 0.0;
    while tick < duration_s {
        ticks.push(tick);
        tick += step;
    }
    ticks.push(duration_s);
    ticks
}
