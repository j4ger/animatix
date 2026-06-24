pub mod context;
pub mod drag_utils;
pub mod gesture;
pub mod gesture_router;
pub mod gestures;
pub mod grid;
pub mod overlay;
pub mod performance;
pub mod property_popup;
pub mod selection;
pub mod time_lens;

use super::DEFAULT_PREVIEW_SIZE;
use crate::app::design_tokens::semantic::accent;
use crate::app::design_tokens::semantic::accent::hover as accent_hover;
use crate::app::design_tokens::semantic::overlay::{badge_bg, tooltip_bg};
use crate::app::design_tokens::semantic::status;
use crate::app::design_tokens::semantic::status::warning_subtle as amber_subtle;
use crate::app::design_tokens::semantic::surface;
use crate::app::design_tokens::semantic::text;
use crate::app::design_tokens::semantic::text::faint as text_faint;
use crate::app::design_tokens::spatial::STROKE_WIDTH;
use crate::app::design_tokens::typography::TextRole;
use animatix::timeline::{PlacementMode, SceneDimensions, Timeline, TrackAccessor};
use egui::{Color32, Pos2, Stroke, Vec2};
use std::collections::HashSet;

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

use crate::app::design_tokens::spatial::preview::CROSS_SIZE as PREVIEW_CROSS_SIZE;
use crate::app::design_tokens::spatial::preview::DASH_LEN as PREVIEW_DASH_LEN;
use crate::app::design_tokens::spatial::preview::GAP_LEN as PREVIEW_GAP_LEN;
use crate::app::design_tokens::spatial::preview::HANDLE_SIZE as PREVIEW_HANDLE_SIZE;
use crate::app::design_tokens::spatial::preview::MIN_ZOOM as PREVIEW_MIN_ZOOM;
use crate::app::design_tokens::spatial::preview::ROTATION_HIT_BUFFER as PREVIEW_ROTATION_HIT_BUFFER;
use crate::app::design_tokens::spatial::preview::ROTATION_OFFSET as PREVIEW_ROTATION_OFFSET;
use crate::app::design_tokens::spatial::preview::ROTATION_RADIUS as PREVIEW_ROTATION_RADIUS;
use crate::app::design_tokens::spatial::preview::VERTEX_HIT_BUFFER as PREVIEW_VERTEX_HIT_BUFFER;

const SELECTION_COLOR: Color32 = accent::PRIMARY;

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

/// Draw a small corner arc (quarter circle) for rotation affordance.
fn draw_corner_arc(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    start_angle: f32,
    end_angle: f32,
    stroke: Stroke,
) {
    let segments = 8;
    for i in 0..segments {
        let t0 = i as f32 / segments as f32;
        let t1 = (i + 1) as f32 / segments as f32;
        let a0 = start_angle + (end_angle - start_angle) * t0;
        let a1 = start_angle + (end_angle - start_angle) * t1;
        let p0 = Pos2::new(center.x + radius * a0.cos(), center.y + radius * a0.sin());
        let p1 = Pos2::new(center.x + radius * a1.cos(), center.y + radius * a1.sin());
        painter.line_segment([p0, p1], stroke);
    }
}

/// Draw the selection bounding box, 8 scale handles, and rotation handle.
///
/// When `props` is available the bounding box is correctly rotated;
/// otherwise falls back to the axis-aligned hit_regions rect.
/// `pixels_per_point` scales visual handle sizes for HiDPI displays.
pub(super) fn draw_selection_overlay(
    painter: &egui::Painter,
    props: Option<&ActorProps>,
    fallback_rect: Option<egui::Rect>,
    is_dragging: bool,
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    desired: Vec2,
    pixels_per_point: f32,
    zoom: f32,
    pan: Vec2,
) {
    let stroke = if is_dragging {
        Stroke::new(1.5, accent_hover())
    } else {
        Stroke::new(1.5, SELECTION_COLOR)
    };

    if let Some(p) = props {
        // ── Rotated overlay ──────────────────────────────────────────────
        let hw = p.size[0] / 2.0;
        let hh = p.size[1] / 2.0;

        // Compute the four corners of the rotated rect in world space
        let local_corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
        let world_corners: [kurbo::Point; 4] =
            std::array::from_fn(|i| local_to_world(local_corners[i], p.position, p.rotation));

        // Convert to screen space
        let screen_corners: [Pos2; 4] = std::array::from_fn(|i| {
            scene_to_screen(world_corners[i], preview_rect, scene_dimensions, desired, zoom, pan)
        });

        // Draw the four edges of the rotated bounding box
        for i in 0..4 {
            let next = (i + 1) % 4;
            painter.line_segment([screen_corners[i], screen_corners[next]], stroke);
        }

        // Dashed overlay during drag
        if is_dragging {
            let dash_stroke = Stroke::new(STROKE_WIDTH, text_faint());
            for i in 0..4 {
                let start = screen_corners[i];
                let end = screen_corners[(i + 1) % 4];
                let total = start.distance(end);
                let mut pos = 0.0;
                while pos < total {
                    let t0 = pos / total;
                    let t1 = ((pos + PREVIEW_DASH_LEN).min(total)) / total;
                    let p0 = Pos2::new(
                        start.x + (end.x - start.x) * t0,
                        start.y + (end.y - start.y) * t0,
                    );
                    let p1 = Pos2::new(
                        start.x + (end.x - start.x) * t1,
                        start.y + (end.y - start.y) * t1,
                    );
                    painter.line_segment([p0, p1], dash_stroke);
                    pos += PREVIEW_DASH_LEN + PREVIEW_GAP_LEN;
                }
            }
        }

        // ── Unified transform gizmo handles ────────────────────────────────
        let handle_world = world_handle_positions(p);
        let handle_screen: [Pos2; 8] = std::array::from_fn(|i| {
            scene_to_screen(handle_world[i], preview_rect, scene_dimensions, desired, zoom, pan)
        });

        // Corner handles (indices 0-3): filled circles with slight larger presence
        let corner_radius = PREVIEW_HANDLE_SIZE * 0.6 * pixels_per_point;
        for &pos in handle_screen[..4].iter() {
            painter.circle_filled(pos, corner_radius, text::PRIMARY);
            painter.circle_stroke(pos, corner_radius, Stroke::new(1.5, SELECTION_COLOR));
        }

        // Edge handles (indices 4-7): smaller filled squares, more subtle
        let edge_handle_px = PREVIEW_HANDLE_SIZE * 0.7 * pixels_per_point;
        for &pos in handle_screen[4..].iter() {
            let handle_rect =
                egui::Rect::from_center_size(pos, Vec2::new(edge_handle_px, edge_handle_px));
            painter.rect_filled(handle_rect, 1.0, text::PRIMARY);
            painter.rect_stroke(
                handle_rect,
                1.0,
                Stroke::new(STROKE_WIDTH, SELECTION_COLOR),
                egui::StrokeKind::Outside,
            );
        }

        // Rotation ring arcs at corners (only when not dragging)
        if !is_dragging {
            let arc_radius = PREVIEW_HANDLE_SIZE * 1.5 * pixels_per_point;
            let arc_stroke = Stroke::new(STROKE_WIDTH, SELECTION_COLOR.gamma_multiply(0.5));
            // Arc orientations (screen coordinates, y-down): 0°=right, 90°=down
            // TL (index 0): from 180° (left) up to 270° (up)    — outside of top-left
            // TR (index 1): from 270° (up)   to 0° (right)       — outside of top-right
            // BR (index 2): from 0° (right)  to 90° (down)       — outside of bottom-right
            // BL (index 3): from 90° (down)  to 180° (left)      — outside of bottom-left
            let arc_angles: [(f32, f32); 4] = [
                (std::f32::consts::PI, 3.0 * std::f32::consts::PI / 2.0),
                (3.0 * std::f32::consts::PI / 2.0, 2.0 * std::f32::consts::PI),
                (0.0, std::f32::consts::PI / 2.0),
                (std::f32::consts::PI / 2.0, std::f32::consts::PI),
            ];
            for i in 0..4 {
                let (start_angle, end_angle) = arc_angles[i];
                draw_corner_arc(
                    painter,
                    handle_screen[i],
                    arc_radius,
                    start_angle,
                    end_angle,
                    arc_stroke,
                );
            }
        }

        // Rotation handle: on the line from centre to above top-edge, offset by PREVIEW_ROTATION_OFFSET
        let rot_world = rotation_handle_world(p);
        let rot_screen =
            scene_to_screen(rot_world, preview_rect, scene_dimensions, desired, zoom, pan);

        // Line from top-centre to rotation handle
        let top_center_local = [0.0_f32, -hh];
        let top_center_world = local_to_world(top_center_local, p.position, p.rotation);
        let top_center_screen =
            scene_to_screen(top_center_world, preview_rect, scene_dimensions, desired, zoom, pan);
        painter.line_segment(
            [top_center_screen, rot_screen],
            Stroke::new(STROKE_WIDTH, SELECTION_COLOR),
        );
        let rot_radius = PREVIEW_ROTATION_RADIUS * pixels_per_point;
        painter.circle_filled(rot_screen, rot_radius, text::PRIMARY);
        painter.circle_stroke(rot_screen, rot_radius, Stroke::new(STROKE_WIDTH, SELECTION_COLOR));

        // Pivot marker (crosshair) — always drawn so it can be dragged
        {
            let pivot_world_pt = pivot_world(p);
            let pivot_screen = scene_to_screen(
                kurbo::Point::new(pivot_world_pt[0] as f64, pivot_world_pt[1] as f64),
                preview_rect,
                scene_dimensions,
                desired,
                zoom,
                pan,
            );
            let cross_size = PREVIEW_CROSS_SIZE * pixels_per_point;
            let cross_color = status::WARNING;
            painter.line_segment(
                [
                    Pos2::new(pivot_screen.x - cross_size, pivot_screen.y),
                    Pos2::new(pivot_screen.x + cross_size, pivot_screen.y),
                ],
                Stroke::new(1.5, cross_color),
            );
            painter.line_segment(
                [
                    Pos2::new(pivot_screen.x, pivot_screen.y - cross_size),
                    Pos2::new(pivot_screen.x, pivot_screen.y + cross_size),
                ],
                Stroke::new(1.5, cross_color),
            );
            painter.circle_stroke(
                pivot_screen,
                cross_size + 2.0 * pixels_per_point,
                Stroke::new(STROKE_WIDTH, cross_color),
            );
        }
    } else if let Some(fallback) = fallback_rect {
        // ── Axis‑aligned fallback ────────────────────────────────────────
        let sel_rect = fallback;

        painter.rect_stroke(sel_rect, 0.0, stroke, egui::StrokeKind::Outside);

        if is_dragging {
            let dash_stroke = Stroke::new(STROKE_WIDTH, text_faint());
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
                    let t1 = ((pos + PREVIEW_DASH_LEN).min(total)) / total;
                    let p0 = Pos2::new(
                        start.x + (end.x - start.x) * t0,
                        start.y + (end.y - start.y) * t0,
                    );
                    let p1 = Pos2::new(
                        start.x + (end.x - start.x) * t1,
                        start.y + (end.y - start.y) * t1,
                    );
                    painter.line_segment([p0, p1], dash_stroke);
                    pos += PREVIEW_DASH_LEN + PREVIEW_GAP_LEN;
                }
            }
        }

        // Axis-aligned handle positions (old-style)
        let handle_positions = scale_handle_positions(sel_rect);
        for pos in &handle_positions {
            let handle_rect = egui::Rect::from_center_size(
                *pos,
                Vec2::new(PREVIEW_HANDLE_SIZE, PREVIEW_HANDLE_SIZE),
            );
            painter.rect_filled(handle_rect, 1.0, text::PRIMARY);
            painter.rect_stroke(
                handle_rect,
                1.0,
                Stroke::new(STROKE_WIDTH, SELECTION_COLOR),
                egui::StrokeKind::Outside,
            );
        }

        // Rotation handle
        let top_center = Pos2::new(sel_rect.center().x, sel_rect.top());
        let rot_center = Pos2::new(top_center.x, top_center.y - PREVIEW_ROTATION_OFFSET);
        painter.line_segment([top_center, rot_center], Stroke::new(STROKE_WIDTH, SELECTION_COLOR));
        painter.circle_filled(rot_center, PREVIEW_ROTATION_RADIUS, text::PRIMARY);
        painter.circle_stroke(
            rot_center,
            PREVIEW_ROTATION_RADIUS,
            Stroke::new(STROKE_WIDTH, SELECTION_COLOR),
        );
    }
}

/// Draw a union bounding box and handles for multi-selection.
pub(super) fn draw_multi_selection_overlay(
    painter: &egui::Painter,
    screen_rects: &[egui::Rect],
    is_dragging: bool,
    pixels_per_point: f32,
) {
    if screen_rects.is_empty() {
        return;
    }

    // Compute union bounding box
    let mut min = screen_rects[0].min;
    let mut max = screen_rects[0].max;
    for rect in &screen_rects[1..] {
        min.x = min.x.min(rect.min.x);
        min.y = min.y.min(rect.min.y);
        max.x = max.x.max(rect.max.x);
        max.y = max.y.max(rect.max.y);
    }
    let union_rect = egui::Rect::from_min_max(min, max);

    let stroke = if is_dragging {
        Stroke::new(1.5, accent_hover())
    } else {
        Stroke::new(1.5, SELECTION_COLOR)
    };

    painter.rect_stroke(union_rect, 0.0, stroke, egui::StrokeKind::Outside);

    // Dashed overlay during drag
    if is_dragging {
        let dash_stroke = Stroke::new(STROKE_WIDTH, text_faint());
        let corners = [
            union_rect.left_top(),
            union_rect.right_top(),
            union_rect.right_bottom(),
            union_rect.left_bottom(),
        ];
        for i in 0..4 {
            let start = corners[i];
            let end = corners[(i + 1) % 4];
            let total = start.distance(end);
            let mut pos = 0.0;
            while pos < total {
                let t0 = pos / total;
                let t1 = ((pos + PREVIEW_DASH_LEN).min(total)) / total;
                let p0 =
                    Pos2::new(start.x + (end.x - start.x) * t0, start.y + (end.y - start.y) * t0);
                let p1 =
                    Pos2::new(start.x + (end.x - start.x) * t1, start.y + (end.y - start.y) * t1);
                painter.line_segment([p0, p1], dash_stroke);
                pos += PREVIEW_DASH_LEN + PREVIEW_GAP_LEN;
            }
        }
    }

    // Handles on the union bounding box
    let handle_positions = scale_handle_positions(union_rect);
    let handle_px = PREVIEW_HANDLE_SIZE * pixels_per_point;
    for pos in &handle_positions {
        let handle_rect = egui::Rect::from_center_size(*pos, Vec2::new(handle_px, handle_px));
        painter.rect_filled(handle_rect, 1.0, text::PRIMARY);
        painter.rect_stroke(
            handle_rect,
            1.0,
            Stroke::new(STROKE_WIDTH, SELECTION_COLOR),
            egui::StrokeKind::Outside,
        );
    }
}

/// Draw ghost outlines of an actor at a different point in time.
/// Used for onion-skin / ghost edit context.
pub(super) fn draw_ghost_overlay(
    painter: &egui::Painter,
    props: &ActorProps,
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    desired: Vec2,
    zoom: f32,
    pan: Vec2,
    color: Color32,
) {
    let hw = props.size[0] / 2.0;
    let hh = props.size[1] / 2.0;

    let local_corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
    let world_corners: [kurbo::Point; 4] =
        std::array::from_fn(|i| local_to_world(local_corners[i], props.position, props.rotation));

    let screen_corners: [Pos2; 4] = std::array::from_fn(|i| {
        scene_to_screen(world_corners[i], preview_rect, scene_dimensions, desired, zoom, pan)
    });

    let dash_stroke = Stroke::new(STROKE_WIDTH, color);

    for i in 0..4 {
        let start = screen_corners[i];
        let end = screen_corners[(i + 1) % 4];
        let total = start.distance(end);
        let mut pos = 0.0;
        while pos < total {
            let t0 = pos / total;
            let t1 = ((pos + PREVIEW_DASH_LEN).min(total)) / total;
            let p0 = Pos2::new(start.x + (end.x - start.x) * t0, start.y + (end.y - start.y) * t0);
            let p1 = Pos2::new(start.x + (end.x - start.x) * t1, start.y + (end.y - start.y) * t1);
            painter.line_segment([p0, p1], dash_stroke);
            pos += PREVIEW_DASH_LEN + PREVIEW_GAP_LEN;
        }
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
    zoom: f32,
    pan: Vec2,
) {
    let ghost_color = accent_hover();
    let hw = props.size[0] / 2.0;
    let hh = props.size[1] / 2.0;
    let local_corners: [[f32; 2]; 4] = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
    let world_corners: [kurbo::Point; 4] =
        std::array::from_fn(|i| local_to_world(local_corners[i], props.position, props.rotation));
    let screen_corners: [Pos2; 4] = std::array::from_fn(|i| {
        scene_to_screen(world_corners[i], preview_rect, scene_dimensions, desired, zoom, pan)
    });
    for i in 0..4 {
        let next = (i + 1) % 4;
        painter
            .line_segment([screen_corners[i], screen_corners[next]], Stroke::new(1.5, ghost_color));
    }

    let coords: Vec<f32> = sibling_positions
        .iter()
        .map(|(_, pos)| if is_row { pos[0] } else { pos[1] })
        .collect();
    let insertion_coord = if coords.is_empty() {
        if is_row {
            props.position[0]
        } else {
            props.position[1]
        }
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

    let accent = accent::PRIMARY;
    let insertion_screen = if is_row {
        let insertion_pt = scene_to_screen(
            kurbo::Point::new(insertion_coord as f64, 0.0),
            preview_rect,
            scene_dimensions,
            desired,
            zoom,
            pan,
        );
        painter.line_segment(
            [
                Pos2::new(insertion_pt.x, preview_rect.top()),
                Pos2::new(insertion_pt.x, preview_rect.bottom()),
            ],
            Stroke::new(2.5, accent),
        );
        Pos2::new(insertion_pt.x, preview_rect.top() + 16.0)
    } else {
        let insertion_pt = scene_to_screen(
            kurbo::Point::new(0.0, insertion_coord as f64),
            preview_rect,
            scene_dimensions,
            desired,
            zoom,
            pan,
        );
        painter.line_segment(
            [
                Pos2::new(preview_rect.left(), insertion_pt.y),
                Pos2::new(preview_rect.right(), insertion_pt.y),
            ],
            Stroke::new(2.5, accent),
        );
        Pos2::new(preview_rect.left() + 16.0, insertion_pt.y)
    };

    // Draw target index badge on the insertion line
    let badge_text = format!("→ {}", target_index + 1);
    crate::app::utils::draw_badge(
        painter,
        insertion_screen,
        &badge_text,
        badge_bg(),
        text::PRIMARY,
        Some(Stroke::new(STROKE_WIDTH, accent)),
    );

    // Draw subtle shift arrows on affected siblings
    let shift_color = amber_subtle();
    for (i, (_, pos)) in sibling_positions.iter().enumerate() {
        let screen_pos = scene_to_screen(
            kurbo::Point::new(pos[0] as f64, pos[1] as f64),
            preview_rect,
            scene_dimensions,
            desired,
            zoom,
            pan,
        );
        let arrow_size = 8.0;
        if i == target_index {
            // Sibling at target index will shift right/down
            let (dx, dy) = if is_row {
                (arrow_size, 0.0)
            } else {
                (0.0, arrow_size)
            };
            painter.arrow(screen_pos, Vec2::new(dx, dy), Stroke::new(1.5, shift_color));
        } else if target_index > 0 && i == target_index - 1 {
            // Sibling before target will shift left/up
            let (dx, dy) = if is_row {
                (-arrow_size, 0.0)
            } else {
                (0.0, -arrow_size)
            };
            painter.arrow(screen_pos, Vec2::new(dx, dy), Stroke::new(1.5, shift_color));
        }
    }

    let tooltip_pos = preview_rect.left_top() + Vec2::new(10.0, 10.0);
    let tooltip_text = format!("Reorder: move to position {}", target_index + 1);
    let galley = painter.layout_no_wrap(tooltip_text, TextRole::BodyS.font_id(), text::PRIMARY);
    let tooltip_rect = egui::Rect::from_min_size(tooltip_pos, galley.size() + Vec2::new(12.0, 8.0));
    painter.rect_filled(tooltip_rect, 4.0, tooltip_bg());
    painter.rect_stroke(
        tooltip_rect,
        4.0,
        Stroke::new(STROKE_WIDTH, accent),
        egui::StrokeKind::Outside,
    );
    painter.galley(tooltip_rect.min + Vec2::new(6.0, 4.0), galley, text::PRIMARY);
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
            accent::PRIMARY
        } else {
            text::PRIMARY
        };
        let stroke_color = if is_active {
            status::WARNING
        } else {
            SELECTION_COLOR
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

/// Compute the effective scene-space tip position for a callout actor.
///
/// Delegates to the shared core helper so GUI handle positions always match
/// the rendered arrow tip.
pub(super) fn callout_effective_to(
    track: &animatix::timeline::AnimationTrack,
    timeline: &animatix::timeline::Timeline,
    time_ms: u64,
    scene_dimensions: SceneDimensions,
) -> [f32; 2] {
    animatix::timeline::callout_geometry::derive_callout_geometry(track, time_ms, Some(timeline), scene_dimensions)
        .to
}

// ─── Callout Handles ───────────────────────────────────────────────────────

/// Draw the tip and label handles for a selected Callout actor.
///
/// - Tip handle: diamond at `to` (scene space)
/// - Label handle: circle at `to + label_at` (scene space)
pub(super) fn draw_callout_handles(
    painter: &egui::Painter,
    tip_screen: Pos2,
    label_screen: Pos2,
    active_tip: bool,
    active_label: bool,
    pixels_per_point: f32,
) {
    let r = PREVIEW_HANDLE_SIZE * 0.7 * pixels_per_point;
    // Tip: diamond
    let tip_color = if active_tip { accent_hover() } else { text::PRIMARY };
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
    let lbl_color = if active_label { accent_hover() } else { text::PRIMARY };
    painter.circle_filled(label_screen, r, lbl_color);
    painter.circle_stroke(label_screen, r, Stroke::new(STROKE_WIDTH, SELECTION_COLOR));
}

/// Compute screen-space positions of the callout tip and label handles.
/// Returns `(tip_screen, label_screen)` or `None` if the actor has no callout data.
pub(super) fn callout_handle_screens(
    actor: &str,
    timeline: &animatix::timeline::Timeline,
    time_ms: u64,
    preview_rect: egui::Rect,
    scene_dimensions: SceneDimensions,
    desired: Vec2,
    zoom: f32,
    pan: Vec2,
) -> Option<(Pos2, Pos2)> {
    use animatix::timeline::TrackAccessor;
    let track = timeline.get_track(actor)?;
    let to = callout_effective_to(track, timeline, time_ms, scene_dimensions);
    let label_at = track.geometry.label_at.get(time_ms, [0.0, 50.0]);
    let tip_world = kurbo::Point::new(to[0] as f64, to[1] as f64);
    let label_world = kurbo::Point::new((to[0] + label_at[0]) as f64, (to[1] + label_at[1]) as f64);
    let tip_screen = scene_to_screen(tip_world, preview_rect, scene_dimensions, desired, zoom, pan);
    let label_screen = scene_to_screen(label_world, preview_rect, scene_dimensions, desired, zoom, pan);
    Some((tip_screen, label_screen))
}

/// Draw four side handles around a targeted callout's target bounds.
///
/// `active_place` is the currently-active `CalloutPlace` (highlights the active side).
pub(super) fn draw_callout_place_handles(
    painter: &egui::Painter,
    place_screens: [Pos2; 4],
    active_place: Option<animatix::timeline::animation_track::CalloutPlace>,
    pixels_per_point: f32,
) {
    use animatix::timeline::animation_track::CalloutPlace;
    let places = [CalloutPlace::Top, CalloutPlace::Bottom, CalloutPlace::Left, CalloutPlace::Right];
    let r = PREVIEW_HANDLE_SIZE * 0.55 * pixels_per_point;
    for (i, screen) in place_screens.iter().enumerate() {
        let active = active_place.map(|p| p == places[i]).unwrap_or(false);
        let fill = if active { accent_hover() } else { surface::WIDGET };
        let stroke_color = if active { accent_hover() } else { SELECTION_COLOR };
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
    points.map(|p| scene_to_screen(kurbo::Point::new(p[0] as f64, p[1] as f64), preview_rect, scene_dimensions, desired, zoom, pan))
}

/// Draw the standoff drag handle on the callout tail (at `from` scene position).
pub(super) fn draw_callout_standoff_handle(
    painter: &egui::Painter,
    standoff_screen: Pos2,
    active: bool,
    pixels_per_point: f32,
) {
    let r = PREVIEW_HANDLE_SIZE * 0.6 * pixels_per_point;
    let fill = if active { accent_hover() } else { surface::WIDGET };
    painter.circle_filled(standoff_screen, r, fill);
    painter.circle_stroke(standoff_screen, r, Stroke::new(STROKE_WIDTH, SELECTION_COLOR));
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
