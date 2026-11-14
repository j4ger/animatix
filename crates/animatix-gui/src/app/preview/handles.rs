use super::*;
use crate::app::design_tokens::spatial::preview::ROTATION_OFFSET as PREVIEW_ROTATION_OFFSET;

pub(crate) fn pivot_world(props: &ActorProps) -> [f32; 2] {
    let rotated = rotate_vec(props.pivot_offset, props.rotation);
    [
        props.position[0] + rotated[0],
        props.position[1] + rotated[1],
    ]
}

/// Indices: 0‑3 corners (TL, TR, BR, BL), 4‑7 edge midpoints (T, R, B, L).
fn local_handle_positions(size: [f32; 2]) -> [(f32, f32); 8] {
    let hw = size[0] / 2.0;
    let hh = size[1] / 2.0;
    [
        (-hw, -hh), // 0: top-left corner
        (hw, -hh),  // 1: top-right corner
        (hw, hh),   // 2: bottom-right corner
        (-hw, hh),  // 3: bottom-left corner
        (0.0, -hh), // 4: top edge mid
        (hw, 0.0),  // 5: right edge mid
        (0.0, hh),  // 6: bottom edge mid
        (-hw, 0.0), // 7: left edge mid
    ]
}

/// The anchor point in local space (opposite of the handle being dragged).
pub(crate) fn handle_anchor_local(handle: usize, size: [f32; 2]) -> [f32; 2] {
    let positions = local_handle_positions(size);
    // The opposite handle is (handle + 2) % 8 for corners of a quadrilateral —
    // or the opposite edge for midpoints.
    let opposite = match handle {
        0 => positions[2], // TL → BR
        1 => positions[3], // TR → BL
        2 => positions[0], // BR → TL
        3 => positions[1], // BL → TR
        4 => positions[6], // top-mid → bottom-mid
        5 => positions[7], // right-mid → left-mid
        6 => positions[4], // bottom-mid → top-mid
        7 => positions[5], // left-mid → right-mid
        _ => positions[0],
    };
    [opposite.0, opposite.1]
}

/// Whether a handle constrains scaling to a single axis.
pub(crate) fn handle_constrains_axis(handle: usize) -> bool {
    handle >= 4 // edge midpoints
}

// ─── Coordinate Transforms ──────────────────────────────────────────────────

/// Rotate a 2D vector by `angle` radians.
fn rotate_vec(v: [f32; 2], angle: f32) -> [f32; 2] {
    let cos = angle.cos();
    let sin = angle.sin();
    [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos]
}

/// Transform a local‑space point to world space.
pub(crate) fn local_to_world(local: [f32; 2], position: [f32; 2], rotation: f32) -> kurbo::Point {
    let rotated = rotate_vec(local, rotation);
    kurbo::Point::new((position[0] + rotated[0]) as f64, (position[1] + rotated[1]) as f64)
}

/// Compute the 8 handle centre positions in **world (scene) space**.
pub(crate) fn world_handle_positions(props: &ActorProps) -> [kurbo::Point; 8] {
    let local = local_handle_positions(props.size);
    std::array::from_fn(|i| {
        local_to_world([local[i].0, local[i].1], props.position, props.rotation)
    })
}

/// Compute the rotation handle centre in world space.
pub(crate) fn rotation_handle_world(props: &ActorProps) -> kurbo::Point {
    let offset_local = [0.0_f32, -(props.size[1] / 2.0 + PREVIEW_ROTATION_OFFSET)];
    local_to_world(offset_local, props.position, props.rotation)
}

// ─── Scene ↔ Screen mapping ─────────────────────────────────────────────────

// ─── Selection Bounds (fallback when props unavailable) ─────────────────────

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
