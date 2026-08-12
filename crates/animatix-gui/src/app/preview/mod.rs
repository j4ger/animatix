pub mod context;
pub mod drag_utils;
pub mod gesture;
pub mod gesture_router;
pub mod gestures;
pub mod grid;
pub mod overlay;
pub mod overlay_ops;
pub mod performance;
pub mod property_popup;
pub mod selection;
pub mod time_lens;

use std::collections::HashSet;

use animatix::timeline::{PlacementMode, SceneDimensions, Timeline, TrackAccessor};
use egui::{Pos2, Stroke, Vec2};

use super::DEFAULT_PREVIEW_SIZE;
use crate::app::design_tokens::spatial::STROKE_WIDTH;

// ─── Preview Transform ──────────────────────────────────────────────────────

/// Bundles all coordinate-space conversion state for the preview canvas.
///
/// Instead of threading `zoom`, `pan`, `preview_rect`, and `scene_dimensions`
/// through every helper function, callers build one `PreviewTransform` and
/// pass a reference.
#[derive(Clone, Copy, Debug)]
pub struct PreviewTransform {
    pub scene_dimensions: SceneDimensions,
    pub preview_rect: egui::Rect,
    pub zoom: f32,
    pub pan: Vec2,
}

impl PreviewTransform {
    pub fn new(
        scene_dimensions: SceneDimensions,
        preview_rect: egui::Rect,
        zoom: f32,
        pan: Vec2,
    ) -> Self {
        Self {
            scene_dimensions,
            preview_rect,
            zoom,
            pan,
        }
    }

    /// Compute the uniform scale factor that preserves scene aspect ratio.
    /// Uses "contain" logic: the entire scene fits inside the preview rect.
    pub fn scale(&self) -> (f64, f64) {
        let desired = self.preview_rect.size();
        let scene_w = self.scene_dimensions.width as f64;
        let scene_h = self.scene_dimensions.height as f64;

        // Pixels per scene pixel at zoom = 1.0 (screen / scene)
        let px_per_scene_x = desired.x.max(1.0) as f64 / scene_w;
        let px_per_scene_y = desired.y.max(1.0) as f64 / scene_h;
        let px_per_scene = px_per_scene_x.min(px_per_scene_y);

        // Scene pixels per screen pixel = inverse
        let base_scale = 1.0 / px_per_scene;
        let z = self.zoom.max(PREVIEW_MIN_ZOOM) as f64;
        let scale = base_scale / z;
        (scale, scale)
    }

    /// Return the display rect that the scene occupies when scaled uniformly
    /// to fit inside the preview rect (letterboxed if aspect ratios differ).
    pub fn display_rect(&self) -> egui::Rect {
        let desired = self.preview_rect.size();
        let scene_w = self.scene_dimensions.width as f64;
        let scene_h = self.scene_dimensions.height as f64;

        let px_per_scene_x = desired.x.max(1.0) as f64 / scene_w;
        let px_per_scene_y = desired.y.max(1.0) as f64 / scene_h;
        let px_per_scene = px_per_scene_x.min(px_per_scene_y);

        let z = self.zoom.max(PREVIEW_MIN_ZOOM) as f64;
        let display_w = (scene_w * px_per_scene * z).min(desired.x as f64);
        let display_h = (scene_h * px_per_scene * z).min(desired.y as f64);

        egui::Rect::from_center_size(
            self.preview_rect.center(),
            egui::vec2(display_w as f32, display_h as f32),
        )
    }

    /// Convert a screen position (e.g. mouse cursor) to scene coordinates.
    pub fn screen_to_scene(&self, screen: Pos2) -> kurbo::Point {
        let (scale_x, scale_y) = self.scale();
        let center_x = self.preview_rect.center().x as f64;
        let center_y = self.preview_rect.center().y as f64;
        kurbo::Point::new(
            self.pan.x as f64 + (screen.x as f64 - center_x) * scale_x,
            self.pan.y as f64 + (screen.y as f64 - center_y) * scale_y,
        )
    }

    /// Convert a scene coordinate to a screen position.
    pub fn scene_to_screen(&self, scene: kurbo::Point) -> Pos2 {
        let (scale_x, scale_y) = self.scale();
        let center_x = self.preview_rect.center().x as f64;
        let center_y = self.preview_rect.center().y as f64;
        Pos2::new(
            (center_x + (scene.x - self.pan.x as f64) / scale_x) as f32,
            (center_y + (scene.y - self.pan.y as f64) / scale_y) as f32,
        )
    }
}

// ─── Drag State ─────────────────────────────────────────────────────────────

/// Which spatial property a scale drag should mutate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeMode {
    /// Change the explicit `size` property (Vec2). Used for shapes, images,
    /// and containers where `size` directly controls geometry.
    Size,
    /// Change the transform `scale` property (F32, uniform). Used for actors
    /// with auto-measured bounds (text, plots) where dragging handles should
    /// scale the entire rendered block.
    Scale,
}

/// Active tool mode for the preview canvas. Determines the default interaction
/// when clicking on an actor body (handles still work in all modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolMode {
    /// Auto-detect interaction based on cursor position (default).
    #[default]
    Select,
    /// Click-drag to move actors.
    Move,
    /// Click-drag to scale actors.
    Scale,
    /// Click-drag to rotate actors.
    Rotate,
    /// Click-drag to edit polygon vertices.
    Vertex,
    /// Click-drag to move the pivot point.
    Pivot,
}

/// Tracks the current drag interaction on the preview canvas.
///
/// - `Move`, `Scale`, `Rotate` manipulate absolutely positioned actors.
/// - `Reorder` reorders layout-managed children within their parent container.
///
/// All spatial manipulation is computed in the actor's **object-local** coordinate system
/// (origin at actor centre, pre-rotation axes) and then transformed to world space.
#[derive(Debug, Clone)]
pub enum DragState {
    None,
    /// Dragging actor(s) to move them.
    Move {
        /// Primary actor (the one under the cursor at drag start).
        primary: String,
        /// All selected actors and their start positions.
        actors: Vec<(String, [f32; 2])>,
        /// Mouse position in scene space at drag start.
        start_scene: kurbo::Point,
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
        /// The anchor point in object‑local space.
        /// When a pivot is set this is the pivot; otherwise the corner opposite the handle.
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
        /// Angle from pivot to mouse at drag start (radians).
        start_angle: f32,
        /// Actor rotation at drag start (radians).
        start_rotation: f32,
        /// Pivot point in world space at drag start.
        pivot: [f32; 2],
    },
    /// Dragging a layout-managed child to reorder within its container.
    Reorder {
        actor: String,
        container: String,
        source_index: usize,
        target_index: usize,
        layout_type: animatix::timeline::LayoutType,
    },
    /// Dragging a polygon vertex to reshape it.
    EditVertices {
        actor: String,
        /// Index of the vertex being dragged.
        vertex: usize,
        /// All vertex positions in object-local space at drag start.
        start_points: Vec<[f32; 2]>,
        /// Mouse position in scene space at drag start.
        start_scene: kurbo::Point,
    },
    /// Dragging the pivot crosshair to reposition it.
    MovePivot {
        actor: String,
        /// Pivot offset in object-local space at drag start.
        start_offset: [f32; 2],
        /// Mouse position in scene space at drag start.
        start_scene: kurbo::Point,
    },
    /// Dragging a callout label handle — updates `label_at`.
    CalloutLabel {
        actor: String,
        /// `label_at` value at drag start.
        start_label_at: [f32; 2],
        /// Mouse position in scene space at drag start.
        start_scene: kurbo::Point,
    },
    /// Dragging a callout standoff handle — updates `standoff` scalar.
    CalloutStandoff {
        actor: String,
        /// Tip position (`to`) in scene space, used to compute new standoff distance.
        tip_scene: [f32; 2],
        /// `standoff` value at drag start.
        start_standoff: f32,
        /// Mouse position in scene space at drag start.
        start_scene: kurbo::Point,
    },
    /// Shift-detach initiated: bake and remove target on DragEnd.
    CalloutDetach {
        actor: String,
        /// Baked `from` (scene space).
        from: [f32; 2],
        /// Baked `to` (scene space).
        to: [f32; 2],
        /// Current `label_at`.
        label_at: [f32; 2],
    },
    /// Dragging a callout tip handle — updates `to` (manual) or `to_offset` (targeted).
    CalloutTip {
        actor: String,
        /// Whether the callout has a non-empty `target` (targeted mode).
        is_targeted: bool,
        /// `to` or `to_offset` value at drag start.
        start_value: [f32; 2],
        /// Mouse position in scene space at drag start.
        start_scene: kurbo::Point,
    },
    /// Dragging a motion path keyframe control point.
    MotionPath {
        actor: String,
        /// Time of the keyframe being dragged (ms).
        time_ms: u64,
        /// Position value at drag start.
        start_position: [f32; 2],
        /// Mouse position in scene space at drag start.
        start_scene: kurbo::Point,
    },
}

// ─── Actor Properties ───────────────────────────────────────────────────────

/// The essential spatial properties of an actor, extracted from the timeline.
#[derive(Debug, Clone, Copy)]
pub struct ActorProps {
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub rotation: f32,
    /// Pivot offset in object-local space (relative to actor center).
    pub pivot_offset: [f32; 2],
}

/// Compute the pivot point in world space.
pub(super) fn pivot_world(props: &ActorProps) -> [f32; 2] {
    let rotated = rotate_vec(props.pivot_offset, props.rotation);
    [
        props.position[0] + rotated[0],
        props.position[1] + rotated[1],
    ]
}

use crate::app::design_tokens::spatial::preview::{
    HANDLE_SIZE as PREVIEW_HANDLE_SIZE, MIN_ZOOM as PREVIEW_MIN_ZOOM,
    ROTATION_HIT_BUFFER as PREVIEW_ROTATION_HIT_BUFFER, ROTATION_OFFSET as PREVIEW_ROTATION_OFFSET,
    VERTEX_HIT_BUFFER as PREVIEW_VERTEX_HIT_BUFFER,
};

pub(super) fn is_layout_managed(actor: &str, timeline: &Timeline, time_ms: u64) -> bool {
    timeline
        .get_track(actor)
        .map(|t| {
            t.geometry.placement_mode.get(time_ms, PlacementMode::LayoutManaged)
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
    [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos]
}

/// Transform a local‑space point to world space.
pub(super) fn local_to_world(local: [f32; 2], position: [f32; 2], rotation: f32) -> kurbo::Point {
    let rotated = rotate_vec(local, rotation);
    kurbo::Point::new((position[0] + rotated[0]) as f64, (position[1] + rotated[1]) as f64)
}

/// Compute the 8 handle centre positions in **world (scene) space**.
pub(super) fn world_handle_positions(props: &ActorProps) -> [kurbo::Point; 8] {
    let local = local_handle_positions(props.size);
    std::array::from_fn(|i| {
        local_to_world([local[i].0, local[i].1], props.position, props.rotation)
    })
}

/// Compute the rotation handle centre in world space.
pub(super) fn rotation_handle_world(props: &ActorProps) -> kurbo::Point {
    let offset_local = [0.0_f32, -(props.size[1] / 2.0 + PREVIEW_ROTATION_OFFSET)];
    local_to_world(offset_local, props.position, props.rotation)
}

// ─── Scene ↔ Screen mapping ─────────────────────────────────────────────────

/// Convert scene coordinates to screen coordinates for the preview canvas.
pub(super) fn scene_to_screen(
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
pub(super) fn selection_screen_rect(
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
pub(super) fn hit_test_handle(
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
pub(super) fn hit_test_rotation_handle(
    screen_point: Pos2,
    rot_screen: Pos2,
    hit_radius: f32,
) -> bool {
    screen_point.distance(rot_screen) <= hit_radius + PREVIEW_ROTATION_HIT_BUFFER
}

/// Check if the screen point is near the pivot crosshair.
pub(super) fn hit_test_pivot(screen_point: Pos2, pivot_screen: Pos2, hit_radius: f32) -> bool {
    screen_point.distance(pivot_screen) <= hit_radius
}

/// Check if the screen point is near any polygon vertex.
/// Returns the vertex index if within hit radius.
pub(super) fn hit_test_vertex(
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
pub(super) fn draw_vertex_handles(
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
            theme.accent.primary
        } else {
            theme.text.primary
        };
        let stroke_color = if is_active {
            theme.status.warning
        } else {
            theme.accent.primary
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
pub(super) fn draw_callout_handles(
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
        theme.accent.hover
    } else {
        theme.text.primary
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
        theme.accent.hover
    } else {
        theme.text.primary
    };
    painter.circle_filled(label_screen, r, lbl_color);
    painter.circle_stroke(label_screen, r, Stroke::new(STROKE_WIDTH, theme.accent.primary));
}

/// Draw four side handles around a targeted callout's target bounds.
///
/// `active_place` is the currently-active `CalloutPlace` (highlights the active side).
pub(super) fn draw_callout_place_handles(
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
            theme.accent.hover
        } else {
            theme.surface.widget
        };
        let stroke_color = if active {
            theme.accent.hover
        } else {
            theme.accent.primary
        };
        painter.circle_filled(*screen, r, fill);
        painter.circle_stroke(*screen, r, Stroke::new(STROKE_WIDTH, stroke_color));
    }
}

/// Compute screen-space positions of the four side handles for a targeted callout's target bounds.
/// Order: [Top, Bottom, Left, Right].
pub(super) fn callout_place_handle_screens(
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
pub(super) fn draw_callout_standoff_handle(
    painter: &egui::Painter,
    theme: eparts::Theme,
    standoff_screen: Pos2,
    active: bool,
    pixels_per_point: f32,
) {
    let r = PREVIEW_HANDLE_SIZE * 0.6 * pixels_per_point;
    let fill = if active {
        theme.accent.hover
    } else {
        theme.surface.widget
    };
    painter.circle_filled(standoff_screen, r, fill);
    painter.circle_stroke(standoff_screen, r, Stroke::new(STROKE_WIDTH, theme.accent.primary));
}

// ─── Preview Helpers ────────────────────────────────────────────────────────────

pub(super) fn fit_preview(dimensions: SceneDimensions, available: Vec2) -> Vec2 {
    let aspect = if dimensions.width == 0 || dimensions.height == 0 {
        DEFAULT_PREVIEW_SIZE.width as f32 / DEFAULT_PREVIEW_SIZE.height as f32
    } else {
        dimensions.width as f32 / dimensions.height as f32
    };
    // Prioritize using all available height; compute width from aspect ratio.
    // This maximizes the preview surface area while preserving aspect ratio.
    Vec2::new(available.y * aspect, available.y)
}

#[cfg(test)]
pub(super) fn timeline_fraction(current_time_s: f64, duration_s: f64) -> f32 {
    (current_time_s / duration_s.max(0.1)).clamp(0.0, 1.0) as f32
}

#[cfg(test)]
pub(super) fn time_from_pointer_x(rect: egui::Rect, pointer_x: f32, duration_s: f64) -> f64 {
    let width = rect.width().max(1.0);
    let normalized = ((pointer_x - rect.left()) / width).clamp(0.0, 1.0) as f64;
    normalized * duration_s.max(0.1)
}

#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_to_world_zero_rotation() {
        let result = local_to_world([0.0, 0.0], [10.0, 20.0], 0.0);
        assert!((result.x - 10.0).abs() < 1e-6);
        assert!((result.y - 20.0).abs() < 1e-6);
    }

    #[test]
    fn test_local_to_world_90_degrees() {
        let result = local_to_world([1.0, 0.0], [0.0, 0.0], std::f32::consts::FRAC_PI_2);
        assert!((result.x - 0.0).abs() < 1e-6, "x={}", result.x);
        assert!((result.y - 1.0).abs() < 1e-6, "y={}", result.y);
    }

    #[test]
    fn test_local_to_world_180_degrees() {
        let result = local_to_world([1.0, 0.0], [0.0, 0.0], std::f32::consts::PI);
        assert!((result.x - (-1.0)).abs() < 1e-6, "x={}", result.x);
        assert!((result.y - 0.0).abs() < 1e-6, "y={}", result.y);
    }

    #[test]
    fn test_local_to_world_various_positions_zero_rotation() {
        let cases = [
            ([5.0, -3.0], [100.0, 200.0], [105.0, 197.0]),
            ([-50.0, 25.0], [10.0, 20.0], [-40.0, 45.0]),
            ([0.0, 0.0], [-100.0, 300.0], [-100.0, 300.0]),
            ([33.0, 77.0], [-20.0, -50.0], [13.0, 27.0]),
        ];
        for (local, position, expected) in &cases {
            let result = local_to_world(*local, *position, 0.0);
            assert!(
                (result.x - expected[0]).abs() < 1e-6,
                "local={:?} pos={:?}: expected x={}, got x={}",
                local,
                position,
                expected[0],
                result.x,
            );
            assert!(
                (result.y - expected[1]).abs() < 1e-6,
                "local={:?} pos={:?}: expected y={}, got y={}",
                local,
                position,
                expected[1],
                result.y,
            );
        }
    }

    #[test]
    fn test_handle_anchor_local_all_handles() {
        let size = [100.0, 50.0];
        // handle_anchor_local returns the opposite handle's local position.
        // Opposite mapping from local_handle_positions with size/2 = [50, 25]:
        //   0 (TL corner) → positions[2] = (50, 25)   [BR corner]
        //   1 (TR corner) → positions[3] = (-50, 25)  [BL corner]
        //   2 (BR corner) → positions[0] = (-50, -25) [TL corner]
        //   3 (BL corner) → positions[1] = (50, -25)  [TR corner]
        //   4 (top-mid)   → positions[6] = (0, 25)    [bottom-mid]
        //   5 (right-mid) → positions[7] = (-50, 0)   [left-mid]
        //   6 (bottom-mid)→ positions[4] = (0, -25)   [top-mid]
        //   7 (left-mid)  → positions[5] = (50, 0)    [right-mid]
        let expected: [[f32; 2]; 8] = [
            [50.0, 25.0],
            [-50.0, 25.0],
            [-50.0, -25.0],
            [50.0, -25.0],
            [0.0, 25.0],
            [-50.0, 0.0],
            [0.0, -25.0],
            [50.0, 0.0],
        ];
        for (i, expected_item) in expected.iter().enumerate() {
            let result = handle_anchor_local(i, size);
            assert!(
                (result[0] - expected_item[0]).abs() < 1e-6,
                "handle {}: expected x={}, got x={}",
                i,
                expected_item[0],
                result[0],
            );
            assert!(
                (result[1] - expected_item[1]).abs() < 1e-6,
                "handle {}: expected y={}, got y={}",
                i,
                expected_item[1],
                result[1],
            );
        }
    }

    #[test]
    fn test_handle_constrains_axis_corners_return_false() {
        for handle in 0..4 {
            assert!(
                !handle_constrains_axis(handle),
                "handle {} (corner) should not constrain axis",
                handle,
            );
        }
    }

    #[test]
    fn test_handle_constrains_axis_edges_return_true() {
        for handle in 4..8 {
            assert!(
                handle_constrains_axis(handle),
                "handle {} (edge) should constrain axis",
                handle,
            );
        }
    }

    #[test]
    fn test_timeline_tick_times_short() {
        // ≤2.0s → step 0.25s
        // ticks: 0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75  (8 in loop) + push 2.0 = 9
        let ticks = timeline_tick_times(2.0);
        assert_eq!(ticks.len(), 9);
        assert!((ticks[1] - 0.25).abs() < 1e-9);
        assert!((ticks[4] - 1.0).abs() < 1e-9);
        assert!((ticks[8] - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_timeline_tick_times_medium() {
        // Between 2.0 and 5.0 → step 0.5s
        // ticks: 0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0, 4.5 (10) + push 5.0 = 11
        let ticks = timeline_tick_times(5.0);
        assert_eq!(ticks.len(), 11);
        assert!((ticks[0] - 0.0).abs() < 1e-9);
        assert!((ticks[2] - 1.0).abs() < 1e-9);
        assert!((ticks[10] - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_timeline_tick_times_long() {
        // Between 5.0 and 15.0 → step 1.0s
        // ticks: 0..14 (15 ticks) + push 15.0 = 16
        let ticks = timeline_tick_times(15.0);
        assert_eq!(ticks.len(), 16);
        assert!((ticks[5] - 5.0).abs() < 1e-9);
        assert!((ticks[15] - 15.0).abs() < 1e-9);
    }

    #[test]
    fn test_timeline_tick_times_very_long() {
        // Between 15.0 and 45.0 → step 5.0s
        // ticks: 0,5,10,15,20,25,30,35,40 (9) + push 45.0 = 10
        let ticks = timeline_tick_times(45.0);
        assert_eq!(ticks.len(), 10);
        assert!((ticks[3] - 15.0).abs() < 1e-9);
        assert!((ticks[9] - 45.0).abs() < 1e-9);
    }

    #[test]
    fn test_timeline_tick_times_extreme() {
        // >45.0s → step 10.0s
        // ticks: 0,10,20,30,40,50,60,70,80,90 (10) + push 100.0 = 11
        let ticks = timeline_tick_times(100.0);
        assert_eq!(ticks.len(), 11);
        assert!((ticks[5] - 50.0).abs() < 1e-9);
        assert!((ticks[10] - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_timeline_tick_times_very_short() {
        // 0.1s → clamped to min 0.1, step 0.25
        let ticks = timeline_tick_times(0.1);
        assert_eq!(ticks.len(), 2, "Only t=0 and t=0.1");
        assert!((ticks[0] - 0.0).abs() < 1e-9);
        assert!((ticks[1] - 0.1).abs() < 1e-9);
    }

    #[test]
    fn test_timeline_tick_times_ends_with_duration() {
        let ticks = timeline_tick_times(3.7);
        assert!((ticks.last().unwrap() - 3.7).abs() < 1e-9, "last tick must equal duration_s");
    }

    #[test]
    fn test_timeline_tick_times_strictly_increasing() {
        let ticks = timeline_tick_times(42.0);
        for pair in ticks.windows(2) {
            assert!(
                pair[0] < pair[1],
                "ticks must be strictly increasing: {} >= {}",
                pair[0],
                pair[1]
            );
        }
    }
}
