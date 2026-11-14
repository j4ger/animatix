use super::*;

/// Compute the screen-space bounding box for the selected actor using hit_regions.
/// This is a fallback for actors without explicit position/size/rotation tracks.
pub(crate) fn selection_screen_rect(
    selected_actors: &HashSet<String>,
    hit_regions: &[(String, kurbo::Rect)],
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    zoom: f32,
    pan: Vec2,
) -> Option<egui::Rect> {
    let first = selected_actors.iter().next()?;
    let (_, bounds) = hit_regions.iter().find(|(l, _)| l == first)?;
    let top_left = PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan)
        .scene_to_screen(kurbo::Point::new(bounds.x0, bounds.y0));
    let bottom_right = PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan)
        .scene_to_screen(kurbo::Point::new(bounds.x1, bounds.y1));
    Some(egui::Rect::from_min_max(top_left, bottom_right))
}

// ─── Selection Overlay ──────────────────────────────────────────────────────
//
// Overlay generation lives in `overlay_ops.rs` so it can be tested without a
// live egui painter.

// ─── Preview Helpers ────────────────────────────────────────────────────────────

pub(crate) fn fit_preview(dimensions: SceneDimensions, available: Vec2) -> Vec2 {
    let (scene_w, scene_h) = if dimensions.width == 0 || dimensions.height == 0 {
        (DEFAULT_PREVIEW_SIZE.width as f32, DEFAULT_PREVIEW_SIZE.height as f32)
    } else {
        (dimensions.width as f32, dimensions.height as f32)
    };
    let scale = (available.x / scene_w).min(available.y / scene_h);
    Vec2::new(scene_w * scale, scene_h * scale)
}

#[cfg(test)]
pub(crate) fn timeline_fraction(current_time_s: f64, duration_s: f64) -> f32 {
    (current_time_s / duration_s.max(0.1)).clamp(0.0, 1.0) as f32
}

#[cfg(test)]
pub(crate) fn time_from_pointer_x(rect: egui::Rect, pointer_x: f32, duration_s: f64) -> f64 {
    let width = rect.width().max(1.0);
    let normalized = ((pointer_x - rect.left()) / width).clamp(0.0, 1.0) as f64;
    normalized * duration_s.max(0.1)
}

#[cfg(test)]
pub(crate) fn timeline_tick_times(duration_s: f64) -> Vec<f64> {
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
