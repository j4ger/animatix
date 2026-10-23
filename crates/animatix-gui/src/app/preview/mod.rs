use crate::app::design_tokens::spatial::preview::MIN_ZOOM as PREVIEW_MIN_ZOOM;
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

/// Per-actor start state captured when a group transform begins.
#[derive(Debug, Clone)]
pub struct GroupTransformActor {
    pub label: String,
    pub position: [f32; 2],
    pub size: [f32; 2],
    pub rotation: f32,
    /// `size` (geometry) or `scale` (whole-block) — see [`ResizeMode`].
    pub resize_mode: ResizeMode,
    /// Transform scale at drag start (used when `resize_mode == Scale`).
    pub scale: f32,
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
    /// Dragging a group scale handle across a multi-selection.
    GroupScale {
        actors: Vec<GroupTransformActor>,
        /// Union bounds of the selection in scene space at drag start.
        union: kurbo::Rect,
        /// Handle index 0-7 on the union box.
        handle: usize,
        /// Preserve aspect ratio (Shift held).
        uniform: bool,
    },
    /// Rotating a multi-selection around its union centre.
    GroupRotate {
        actors: Vec<GroupTransformActor>,
        /// Rotation centre in scene space.
        center: [f32; 2],
        /// Angle from centre to pointer at drag start (radians).
        start_angle: f32,
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

mod handles;
#[cfg(test)]
mod tests;

pub(crate) use handles::*;
