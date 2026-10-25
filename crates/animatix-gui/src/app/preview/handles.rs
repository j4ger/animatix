use super::*;

/// Compute the pivot point in world space.
pub(crate) fn pivot_world(props: &ActorProps) -> [f32; 2] {
    let rotated = rotate_vec(props.pivot_offset, props.rotation);
    [
        props.position[0] + rotated[0],
        props.position[1] + rotated[1],
    ]
}

use crate::app::design_tokens::spatial::preview::{
    HANDLE_SIZE as PREVIEW_HANDLE_SIZE, ROTATION_HIT_BUFFER as PREVIEW_ROTATION_HIT_BUFFER,
    ROTATION_OFFSET as PREVIEW_ROTATION_OFFSET, VERTEX_HIT_BUFFER as PREVIEW_VERTEX_HIT_BUFFER,
};

pub(crate) fn is_layout_managed(actor: &str, timeline: &Timeline, time_ms: u64) -> bool {
    timeline
        .get_entity_id(actor)
        .map(|id| {
            timeline
                .world()
                .geometry(id)
                .placement_mode
                .get(time_ms, PlacementMode::LayoutManaged)
                == PlacementMode::LayoutManaged
        })
        .unwrap_or(false)
}

// ─── Handle Layout in Local Space ───────────────────────────────────────────

/// Returns the 8 handle positions in **object‑local** coordinates
/// (origin at actor centre, axes un‑rotated).
///
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

/// Convert scene coordinates to screen coordinates for the preview canvas.
pub(crate) fn scene_to_screen(
    scene_pos: kurbo::Point,
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    _desired: Vec2,
    zoom: f32,
    pan: Vec2,
) -> Pos2 {
    let tx = PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan);
    tx.scene_to_screen(scene_pos)
}

// ─── Selection Bounds (fallback when props unavailable) ─────────────────────

/// Compute the screen-space bounding box for the selected actor using hit_regions.
/// This is a fallback for actors without explicit position/size/rotation tracks.
pub(crate) fn selection_screen_rect(
    selected_actors: &HashSet<String>,
    hit_regions: &[(String, kurbo::Rect)],
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    desired: Vec2,
    zoom: f32,
    pan: Vec2,
) -> Option<egui::Rect> {
    let first = selected_actors.iter().next()?;
    let (_, bounds) = hit_regions.iter().find(|(l, _)| l == first)?;
    let top_left = scene_to_screen(
        kurbo::Point::new(bounds.x0, bounds.y0),
        preview_rect,
        scene_dimensions,
        desired,
        zoom,
        pan,
    );
    let bottom_right = scene_to_screen(
        kurbo::Point::new(bounds.x1, bounds.y1),
        preview_rect,
        scene_dimensions,
        desired,
        zoom,
        pan,
    );
    Some(egui::Rect::from_min_max(top_left, bottom_right))
}

// ─── Selection Overlay ──────────────────────────────────────────────────────
//
// Selection/ghost/reorder overlay generation now lives in `overlay_ops.rs` so
// overlay behavior can be tested without a live egui painter.

// ─── Hit testing ────────────────────────────────────────────────────────────

/// Check which handle (0‑7) the given screen point is near.
/// Returns None if no handle is within hit radius.
pub(crate) fn hit_test_handle(
    screen_point: Pos2,
    handle_screen_positions: &[Pos2; 8],
    hit_radius: f32,
) -> Option<usize> {
    for (i, pos) in handle_screen_positions.iter().enumerate() {
        if screen_point.distance(*pos) <= hit_radius {
            return Some(i);
        }
    }
    None
}

/// Check if the screen point is near the rotation handle.
pub(crate) fn hit_test_rotation_handle(
    screen_point: Pos2,
    rot_screen: Pos2,
    hit_radius: f32,
) -> bool {
    screen_point.distance(rot_screen) <= hit_radius + PREVIEW_ROTATION_HIT_BUFFER
}

/// Check if the screen point is near the pivot crosshair.
pub(crate) fn hit_test_pivot(screen_point: Pos2, pivot_screen: Pos2, hit_radius: f32) -> bool {
    screen_point.distance(pivot_screen) <= hit_radius
}

/// Check if the screen point is near any polygon vertex.
/// Returns the vertex index if within hit radius.
pub(crate) fn hit_test_vertex(
    screen_point: Pos2,
    props: &ActorProps,
    points: &[[f32; 2]],
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    desired: Vec2,
    hit_radius: f32,
    zoom: f32,
    pan: Vec2,
) -> Option<usize> {
    for (i, &pt) in points.iter().enumerate() {
        let world = local_to_world(pt, props.position, props.rotation);
        let screen = scene_to_screen(world, preview_rect, scene_dimensions, desired, zoom, pan);
        if screen_point.distance(screen) <= hit_radius + PREVIEW_VERTEX_HIT_BUFFER {
            return Some(i);
        }
    }
    None
}

/// Draw small circles at each polygon vertex in screen space.
/// `pixels_per_point` scales vertex handle sizes for HiDPI displays.
pub(crate) fn draw_vertex_handles(
    painter: &egui::Painter,
    theme: eparts::Theme,
    props: &ActorProps,
    points: &[[f32; 2]],
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    desired: Vec2,
    active_vertex: Option<usize>,
    pixels_per_point: f32,
    zoom: f32,
    pan: Vec2,
) {
    const VERTEX_RADIUS: f32 = 4.0;
    for (i, &pt) in points.iter().enumerate() {
        let world = local_to_world(pt, props.position, props.rotation);
        let screen = scene_to_screen(world, preview_rect, scene_dimensions, desired, zoom, pan);
        let is_active = active_vertex == Some(i);
        let fill = if is_active {
            theme.palette.accent.primary
        } else {
            theme.palette.text.primary
        };
        let stroke_color = if is_active {
            theme.palette.status.warning
        } else {
            theme.palette.accent.primary
        };
        let radius = if is_active {
            (VERTEX_RADIUS + 1.5) * pixels_per_point
        } else {
            VERTEX_RADIUS * pixels_per_point
        };
        painter.circle_filled(screen, radius, fill);
        painter.circle_stroke(screen, radius, Stroke::new(STROKE_WIDTH, stroke_color));
    }
}

// ─── Callout Helpers ──────────────────────────────────────────────────────

// ─── Callout Handles ───────────────────────────────────────────────────────

/// Draw the tip and label handles for a selected Callout actor.
///
/// - Tip handle: diamond at `to` (scene space)
/// - Label handle: circle at `to + label_at` (scene space)
pub(crate) fn draw_callout_handles(
    painter: &egui::Painter,
    theme: eparts::Theme,
    tip_screen: Pos2,
    label_screen: Pos2,
    active_tip: bool,
    active_label: bool,
    pixels_per_point: f32,
) {
    let r = PREVIEW_HANDLE_SIZE * 0.7 * pixels_per_point;
    // Tip: diamond
    let tip_color = if active_tip {
        theme.palette.accent.hover
    } else {
        theme.palette.text.primary
    };
    let tip_pts = [
        Pos2::new(tip_screen.x, tip_screen.y - r * 1.4),
        Pos2::new(tip_screen.x + r * 1.4, tip_screen.y),
        Pos2::new(tip_screen.x, tip_screen.y + r * 1.4),
        Pos2::new(tip_screen.x - r * 1.4, tip_screen.y),
    ];
    for i in 0..4 {
        painter.line_segment([tip_pts[i], tip_pts[(i + 1) % 4]], Stroke::new(1.5, tip_color));
    }
    // Label: circle
    let lbl_color = if active_label {
        theme.palette.accent.hover
    } else {
        theme.palette.text.primary
    };
    painter.circle_filled(label_screen, r, lbl_color);
    painter.circle_stroke(label_screen, r, Stroke::new(STROKE_WIDTH, theme.palette.accent.primary));
}

/// Draw four side handles around a targeted callout's target bounds.
///
/// `active_place` is the currently-active `CalloutPlace` (highlights the active side).
pub(crate) fn draw_callout_place_handles(
    painter: &egui::Painter,
    theme: eparts::Theme,
    place_screens: [Pos2; 4],
    active_place: Option<animatix::timeline::animation_track::CalloutPlace>,
    pixels_per_point: f32,
) {
    use animatix::timeline::animation_track::CalloutPlace;
    let places = [
        CalloutPlace::Top,
        CalloutPlace::Bottom,
        CalloutPlace::Left,
        CalloutPlace::Right,
    ];
    let r = PREVIEW_HANDLE_SIZE * 0.55 * pixels_per_point;
    for (i, screen) in place_screens.iter().enumerate() {
        let active = active_place.map(|p| p == places[i]).unwrap_or(false);
        let fill = if active {
            theme.palette.accent.hover
        } else {
            theme.palette.surface.widget
        };
        let stroke_color = if active {
            theme.palette.accent.hover
        } else {
            theme.palette.accent.primary
        };
        painter.circle_filled(*screen, r, fill);
        painter.circle_stroke(*screen, r, Stroke::new(STROKE_WIDTH, stroke_color));
    }
}

/// Compute screen-space positions of the four side handles for a targeted callout's target bounds.
/// Order: [Top, Bottom, Left, Right].
pub(crate) fn callout_place_handle_screens(
    geo: &animatix::timeline::callout_geometry::CalloutGeometry,
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    desired: Vec2,
    zoom: f32,
    pan: Vec2,
) -> [Pos2; 4] {
    let c = geo.target_centre;
    let h = geo.target_half;
    let points = [
        [c[0], c[1] - h[1]], // Top
        [c[0], c[1] + h[1]], // Bottom
        [c[0] - h[0], c[1]], // Left
        [c[0] + h[0], c[1]], // Right
    ];
    points.map(|p| {
        scene_to_screen(
            kurbo::Point::new(p[0] as f64, p[1] as f64),
            preview_rect,
            scene_dimensions,
            desired,
            zoom,
            pan,
        )
    })
}

/// Draw the standoff drag handle on the callout tail (at `from` scene position).
pub(crate) fn draw_callout_standoff_handle(
    painter: &egui::Painter,
    theme: eparts::Theme,
    standoff_screen: Pos2,
    active: bool,
    pixels_per_point: f32,
) {
    let r = PREVIEW_HANDLE_SIZE * 0.6 * pixels_per_point;
    let fill = if active {
        theme.palette.accent.hover
    } else {
        theme.palette.surface.widget
    };
    painter.circle_filled(standoff_screen, r, fill);
    painter.circle_stroke(
        standoff_screen,
        r,
        Stroke::new(STROKE_WIDTH, theme.palette.accent.primary),
    );
}

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
