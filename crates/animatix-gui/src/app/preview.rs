use super::DEFAULT_PREVIEW_SIZE;
use animatix::timeline::{PlacementMode, SceneDimensions, Timeline, TrackAccessor};
use egui::{Color32, FontId, Pos2, Stroke, Vec2};

// ─── Drag State ─────────────────────────────────────────────────────────────

/// Which spatial property a scale drag should mutate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ResizeMode {
    /// Change the explicit `size` property (Vec2). Used for shapes, images,
    /// and containers where `size` directly controls geometry.
    Size,
    /// Change the transform `scale` property (F32, uniform). Used for actors
    /// with auto-measured bounds (text, plots) where dragging handles should
    /// scale the entire rendered block.
    Scale,
}

/// Tracks the current drag interaction on the preview canvas.
///
/// - `Move`, `Scale`, `Rotate` manipulate absolutely positioned actors.
/// - `Reorder` reorders layout-managed children within their parent container.
///
/// All spatial manipulation is computed in the actor's **object-local** coordinate system
/// (origin at actor center, pre-rotation axes) and then transformed to world space.
#[derive(Debug, Clone)]
pub(super) enum DragState {
    None,
    /// Dragging the actor body to move it.
    Move {
        actor: String,
        /// Mouse position in scene space at drag start.
        start_scene: kurbo::Point,
        /// Actor centre position [x, y] at drag start.
        start_position: [f32; 2],
    },
    /// Dragging a scale handle (8 corners + edge midpoints).
    Scale {
        actor: String,
        /// Handle index 0‑7 (see `scale_handle_indices`).
        handle: usize,
        /// Mouse position in scene space at drag start.
        start_scene: kurbo::Point,
        /// Actor centre position at drag start.
        start_position: [f32; 2],
        /// Full [width, height] at drag start.
        start_size: [f32; 2],
        /// Rotation in radians at drag start.
        start_rotation: f32,
        /// The anchor point in object‑local space (the corner / edge-midpoint
        /// opposite the dragged handle). This stays fixed in world space.
        anchor_local: [f32; 2],
        /// Whether this is a single‑axis scale (edge‑mid handle).
        constrain_axis: bool,
        /// Whether to preserve aspect ratio (Shift held).
        uniform_ratio: bool,
        /// Which property to mutate (`size` vs `scale`).
        resize_mode: ResizeMode,
        /// Transform scale at drag start (only used when `resize_mode == Scale`).
        start_scale: f32,
    },
    /// Dragging the rotation handle.
    Rotate {
        actor: String,
        /// Angle from centre to mouse at drag start (radians).
        start_angle: f32,
        /// Actor rotation at drag start (radians).
        start_rotation: f32,
        /// Actor centre in world space at drag start.
        center: [f32; 2],
    },
    /// Dragging a layout-managed child to reorder within its container.
    Reorder {
        actor: String,
        container: String,
        source_index: usize,
        target_index: usize,
        start_mouse: kurbo::Point,
        layout_type: animatix::timeline::LayoutType,
    },
}

// ─── Actor Properties ───────────────────────────────────────────────────────

/// The essential spatial properties of an actor, extracted from the timeline.
#[derive(Debug, Clone, Copy)]
pub(super) struct ActorProps {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub rotation: f32,
}

pub(super) const ROTATION_OFFSET: f32 = 20.0;
pub(super) const ROTATION_RADIUS: f32 = 4.0;
const HANDLE_SIZE: f32 = 6.0;
const HANDLE_HIT_RADIUS: f32 = 10.0;
const SELECTION_COLOR: Color32 = Color32::from_rgb(84, 110, 255);

pub(super) fn is_layout_managed(actor: &str, timeline: &Timeline, time_ms: u64) -> bool {
    timeline
        .get_track(actor)
        .map(|t| t.placement_mode.get(time_ms, PlacementMode::LayoutManaged) == PlacementMode::LayoutManaged)
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
pub(super) fn handle_anchor_local(handle: usize, size: [f32; 2]) -> [f32; 2] {
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
pub(super) fn handle_constrains_axis(handle: usize) -> bool {
    handle >= 4 // edge midpoints
}

// ─── Coordinate Transforms ──────────────────────────────────────────────────

/// Rotate a 2D vector by `angle` radians.
fn rotate_vec(v: [f32; 2], angle: f32) -> [f32; 2] {
    let cos = angle.cos();
    let sin = angle.sin();
    [
        v[0] * cos - v[1] * sin,
        v[0] * sin + v[1] * cos,
    ]
}

/// Transform a local‑space point to world space.
fn local_to_world(local: [f32; 2], position: [f32; 2], rotation: f32) -> kurbo::Point {
    let rotated = rotate_vec(local, rotation);
    kurbo::Point::new(
        (position[0] + rotated[0]) as f64,
        (position[1] + rotated[1]) as f64,
    )
}

/// Compute the 8 handle centre positions in **world (scene) space**.
pub(super) fn world_handle_positions(props: &ActorProps) -> [kurbo::Point; 8] {
    let local = local_handle_positions(props.size);
    std::array::from_fn(|i| local_to_world([local[i].0, local[i].1], props.position, props.rotation))
}

/// Compute the rotation handle centre in world space.
pub(super) fn rotation_handle_world(props: &ActorProps) -> kurbo::Point {
    let offset_local = [0.0_f32, -(props.size[1] / 2.0 + ROTATION_OFFSET)];
    local_to_world(offset_local, props.position, props.rotation)
}

// ─── Scene ↔ Screen mapping ─────────────────────────────────────────────────

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

// ─── Selection Bounds (fallback when props unavailable) ─────────────────────

/// Compute the screen-space bounding box for the selected actor using hit_regions.
/// This is a fallback for actors without explicit position/size/rotation tracks.
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

/// Draw the selection bounding box, 8 scale handles, and rotation handle.
///
/// When `props` is available the bounding box is correctly rotated;
/// otherwise falls back to the axis-aligned hit_regions rect.
pub(super) fn draw_selection_overlay(
    painter: &egui::Painter,
    props: Option<&ActorProps>,
    fallback_rect: Option<egui::Rect>,
    is_dragging: bool,
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    desired: Vec2,
) {
    let stroke = if is_dragging {
        Stroke::new(
            1.5,
            Color32::from_rgba_unmultiplied(84, 110, 255, 140),
        )
    } else {
        Stroke::new(1.5, SELECTION_COLOR)
    };

    if let Some(p) = props {
        // ── Rotated overlay ──────────────────────────────────────────────
        let hw = p.size[0] / 2.0;
        let hh = p.size[1] / 2.0;

        // Compute the four corners of the rotated rect in world space
        let local_corners: [[f32; 2]; 4] = [
            [-hw, -hh],
            [hw, -hh],
            [hw, hh],
            [-hw, hh],
        ];
        let world_corners: [kurbo::Point; 4] = std::array::from_fn(|i| {
            local_to_world(local_corners[i], p.position, p.rotation)
        });

        // Convert to screen space
        let screen_corners: [Pos2; 4] = std::array::from_fn(|i| {
            scene_to_screen(world_corners[i], preview_rect, scene_dimensions, desired)
        });

        // Draw the four edges of the rotated bounding box
        for i in 0..4 {
            let next = (i + 1) % 4;
            painter.line_segment([screen_corners[i], screen_corners[next]], stroke);
        }

        // Dashed overlay during drag
        if is_dragging {
            let dash_len = 6.0;
            let gap_len = 4.0;
            let dash_color = Color32::from_rgba_unmultiplied(255, 255, 255, 80);
            let dash_stroke = Stroke::new(1.0, dash_color);
            for i in 0..4 {
                let start = screen_corners[i];
                let end = screen_corners[(i + 1) % 4];
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

        // Scale handles (rotated)
        let handle_world = world_handle_positions(p);
        let handle_screen: [Pos2; 8] = std::array::from_fn(|i| {
            scene_to_screen(handle_world[i], preview_rect, scene_dimensions, desired)
        });
        for pos in &handle_screen {
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

        // Rotation handle: on the line from centre to above top-edge, offset by ROTATION_OFFSET
        let rot_world = rotation_handle_world(p);
        let rot_screen = scene_to_screen(rot_world, preview_rect, scene_dimensions, desired);

        // Line from top-centre to rotation handle
        let top_center_local = [0.0_f32, -hh];
        let top_center_world = local_to_world(top_center_local, p.position, p.rotation);
        let top_center_screen =
            scene_to_screen(top_center_world, preview_rect, scene_dimensions, desired);
        painter.line_segment(
            [top_center_screen, rot_screen],
            Stroke::new(1.0, SELECTION_COLOR),
        );
        painter.circle_filled(rot_screen, ROTATION_RADIUS, Color32::WHITE);
        painter.circle_stroke(
            rot_screen,
            ROTATION_RADIUS,
            Stroke::new(1.0, SELECTION_COLOR),
        );
    } else if let Some(fallback) = fallback_rect {
        // ── Axis‑aligned fallback ────────────────────────────────────────
        let sel_rect = fallback;

        painter.rect_stroke(sel_rect, 0.0, stroke, egui::StrokeKind::Outside);

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

        // Axis-aligned handle positions (old-style)
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

        // Rotation handle
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
}

pub(super) fn draw_reorder_overlay(
    painter: &egui::Painter,
    props: &ActorProps,
    target_index: usize,
    sibling_positions: &[(String, [f32; 2])],
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    desired: egui::Vec2,
    is_row: bool,
) {
    let ghost_color = Color32::from_rgba_unmultiplied(84, 110, 255, 153);
    let hw = props.size[0] / 2.0;
    let hh = props.size[1] / 2.0;
    let local_corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
    let world_corners: [kurbo::Point; 4] = std::array::from_fn(|i| {
        local_to_world(local_corners[i], props.position, props.rotation)
    });
    let screen_corners: [Pos2; 4] = std::array::from_fn(|i| {
        scene_to_screen(world_corners[i], preview_rect, scene_dimensions, desired)
    });
    for i in 0..4 {
        let next = (i + 1) % 4;
        painter.line_segment([screen_corners[i], screen_corners[next]], Stroke::new(1.5, ghost_color));
    }

    let coords: Vec<f32> = sibling_positions
        .iter()
        .map(|(_, pos)| if is_row { pos[0] } else { pos[1] })
        .collect();
    let insertion_coord = if coords.is_empty() {
        if is_row { props.position[0] } else { props.position[1] }
    } else if target_index == 0 {
        if coords.len() == 1 {
            coords[0]
        } else {
            coords[0] - (coords[1] - coords[0]) * 0.5
        }
    } else if target_index >= coords.len() {
        if coords.len() == 1 {
            coords[0]
        } else {
            let last = coords[coords.len() - 1];
            let prev = coords[coords.len() - 2];
            last + (last - prev) * 0.5
        }
    } else {
        (coords[target_index - 1] + coords[target_index]) * 0.5
    };

    let accent = Color32::from_rgb(84, 110, 255);
    let scale_x = desired.x as f64 / scene_dimensions.width as f64;
    let scale_y = desired.y as f64 / scene_dimensions.height as f64;
    if is_row {
        let x = (preview_rect.min.x as f64 + insertion_coord as f64 * scale_x) as f32;
        painter.line_segment(
            [Pos2::new(x, preview_rect.top()), Pos2::new(x, preview_rect.bottom())],
            Stroke::new(2.5, accent),
        );
    } else {
        let y = (preview_rect.min.y as f64 + insertion_coord as f64 * scale_y) as f32;
        painter.line_segment(
            [Pos2::new(preview_rect.left(), y), Pos2::new(preview_rect.right(), y)],
            Stroke::new(2.5, accent),
        );
    }

    let tooltip_pos = preview_rect.left_top() + Vec2::new(10.0, 10.0);
    let tooltip_text = "Reorder: drag to new position";
    let galley = painter.layout_no_wrap(tooltip_text.to_owned(), FontId::proportional(12.0), Color32::WHITE);
    let tooltip_rect = egui::Rect::from_min_size(tooltip_pos, galley.size() + Vec2::new(12.0, 8.0));
    painter.rect_filled(tooltip_rect, 4.0, Color32::from_rgba_unmultiplied(24, 28, 36, 235));
    painter.rect_stroke(
        tooltip_rect,
        4.0,
        Stroke::new(1.0, accent),
        egui::StrokeKind::Outside,
    );
    painter.galley(tooltip_rect.min + Vec2::new(6.0, 4.0), galley, Color32::WHITE);
}

/// Returns the 8 scale handle centre positions for an axis-aligned rect (legacy fallback).
fn scale_handle_positions(sel_rect: egui::Rect) -> [Pos2; 8] {
    [
        sel_rect.left_top(),
        sel_rect.right_top(),
        sel_rect.right_bottom(),
        sel_rect.left_bottom(),
        Pos2::new(sel_rect.center().x, sel_rect.top()),
        Pos2::new(sel_rect.right(), sel_rect.center().y),
        Pos2::new(sel_rect.center().x, sel_rect.bottom()),
        Pos2::new(sel_rect.left(), sel_rect.center().y),
    ]
}

// ─── Hit testing ────────────────────────────────────────────────────────────

/// Check which handle (0‑7) the given screen point is near.
/// Returns None if no handle is within hit radius.
pub(super) fn hit_test_handle(
    screen_point: Pos2,
    handle_screen_positions: &[Pos2; 8],
) -> Option<usize> {
    for (i, pos) in handle_screen_positions.iter().enumerate() {
        if screen_point.distance(*pos) <= HANDLE_HIT_RADIUS {
            return Some(i);
        }
    }
    None
}

/// Check if the screen point is near the rotation handle.
pub(super) fn hit_test_rotation_handle(
    screen_point: Pos2,
    rot_screen: Pos2,
) -> bool {
    screen_point.distance(rot_screen) <= HANDLE_HIT_RADIUS + 4.0
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
