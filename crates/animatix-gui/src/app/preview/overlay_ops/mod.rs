//! Screen-space overlay operations for the preview canvas.
//!
//! Preview overlays are generated as a list of [`PreviewOverlayOp`] and executed
//! by a single painter pass. This decouples overlay geometry/state from egui
//! painting so behavior can be unit-tested without a GPU or a live UI frame.
//!
//! Geometry is authored in scene coordinates. The executor receives a
//! [`PreviewTransform`] and maps scene geometry to the canvas. Screen-space
//! decorations that depend on egui layout (tooltip boxes, mouse-cycle badges,
//! HUD text) use the explicit [`ScreenOverlayOp`] variants instead of mixing
//! coordinate spaces inside one op.

use animatix::timeline::Timeline;
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};

use super::{
    ActorProps, PreviewTransform, local_to_world, pivot_world, rotation_handle_world,
    world_handle_positions,
};
use crate::app::design_tokens::spatial::STROKE_WIDTH;

/// Colors used by the layout debug overlay.
struct LayoutDebugStyle {
    container_color: Color32,
    slot_color: Color32,
    size_color: Color32,
    spacing_color: Color32,
}

impl LayoutDebugStyle {
    fn from_theme(theme: &eparts::Theme) -> Self {
        Self {
            container_color: theme.palette.accent.primary,
            slot_color: Color32::from_rgba_premultiplied(
                theme.palette.status.warning.r(),
                theme.palette.status.warning.g(),
                theme.palette.status.warning.b(),
                150,
            ),
            size_color: theme.palette.status.warning,
            spacing_color: Color32::from_rgba_premultiplied(200, 80, 80, 60),
        }
    }
}

const RADIUS_M: f32 = 4.0;

/// A stroke used by overlay operations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayStroke {
    pub width: f32,
    pub color: Color32,
}

impl OverlayStroke {
    pub fn new(width: f32, color: Color32) -> Self {
        Self { width, color }
    }
}

impl From<Stroke> for OverlayStroke {
    fn from(stroke: Stroke) -> Self {
        Self::new(stroke.width, stroke.color)
    }
}

impl From<&OverlayStroke> for Stroke {
    fn from(stroke: &OverlayStroke) -> Self {
        Stroke::new(stroke.width, stroke.color)
    }
}

/// A screen-space overlay decoration that depends on egui layout or pointer
/// position. Screen ops are only used where scene-space geometry would not
/// preserve the intended visual size or anchor.
#[derive(Clone, Debug)]
pub enum ScreenOverlayOp {
    RectStroke {
        rect: Rect,
        corner_radius: f32,
        stroke: OverlayStroke,
        stroke_kind: StrokeKind,
    },
    RectFill {
        rect: Rect,
        corner_radius: f32,
        color: Color32,
    },
    Text {
        pos: Pos2,
        anchor: Align2,
        text: String,
        font: FontId,
        color: Color32,
    },
}

/// A drawable preview overlay primitive.
///
/// Scene-space geometry is authored in scene coordinates. Screen decorations
/// are wrapped in [`PreviewOverlayOp::Screen`] and painted directly.
#[derive(Clone, Debug)]
pub enum PreviewOverlayOp {
    Line {
        from: kurbo::Point,
        to: kurbo::Point,
        stroke: OverlayStroke,
    },
    DashedLine {
        from: kurbo::Point,
        to: kurbo::Point,
        dash_len: f32,
        gap_len: f32,
        stroke: OverlayStroke,
    },
    RectStroke {
        rect: kurbo::Rect,
        corner_radius: f32,
        stroke: OverlayStroke,
        stroke_kind: StrokeKind,
    },
    RectFill {
        rect: kurbo::Rect,
        corner_radius: f32,
        color: Color32,
    },
    CircleFill {
        center: kurbo::Point,
        radius: f32,
        color: Color32,
    },
    CircleStroke {
        center: kurbo::Point,
        radius: f32,
        stroke: OverlayStroke,
    },
    Arc {
        center: kurbo::Point,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        stroke: OverlayStroke,
    },
    Text {
        pos: kurbo::Point,
        anchor: Align2,
        text: String,
        font: FontId,
        color: Color32,
    },
    Arrow {
        from: kurbo::Point,
        to: kurbo::Point,
        stroke: OverlayStroke,
    },
    Badge {
        pos: kurbo::Point,
        text: String,
        bg: Color32,
        fg: Color32,
        border_stroke: OverlayStroke,
    },
    Screen(ScreenOverlayOp),
}

/// Convert scene units per screen unit into scene-space overlay sizes.
fn px_to_scene(tx: &PreviewTransform, px: f32) -> f32 {
    (px as f64 * tx.scale().0) as f32
}

/// Convert a scene-space point to screen space through the preview transform.
fn scene_to_screen(tx: &PreviewTransform, point: kurbo::Point) -> Pos2 {
    tx.scene_to_screen(point)
}

/// Convert a scene-space rectangle to screen space.
fn scene_rect_to_screen(tx: &PreviewTransform, rect: &kurbo::Rect) -> Rect {
    let min = scene_to_screen(tx, kurbo::Point::new(rect.x0, rect.y0));
    let max = scene_to_screen(tx, kurbo::Point::new(rect.x1, rect.y1));
    Rect::from_min_max(min, max)
}

fn scene_radius_to_screen(tx: &PreviewTransform, radius: f32) -> f32 {
    (radius as f64 / tx.scale().0) as f32
}

/// Convert a screen-space rectangle back to scene space.
fn screen_rect_to_scene(tx: &PreviewTransform, rect: Rect) -> kurbo::Rect {
    let min = tx.screen_to_scene(rect.min);
    let max = tx.screen_to_scene(rect.max);
    kurbo::Rect::new(min.x.min(max.x), min.y.min(max.y), min.x.max(max.x), min.y.max(max.y))
}

/// The scene-space viewport currently visible in the preview rect.
fn scene_viewport(tx: &PreviewTransform) -> kurbo::Rect {
    screen_rect_to_scene(tx, tx.preview_rect)
}

/// Execute overlay operations onto the canvas painter.
mod execute;
mod guides;
mod interaction;
mod layout;
mod motion;
mod selection;

pub use execute::{execute_overlay_ops, scale_handle_positions};
pub use guides::{actor_label_ops, grid_ops, snap_guide_ops};
pub use interaction::{ghost_overlay_ops, hover_highlight_ops, reorder_overlay_ops};
pub use layout::{layout_debug_ops, scene_bounds_ops};
pub use motion::{cycle_indicator_ops, motion_path_ops};

pub use selection::{multi_selection_overlay_ops, selection_overlay_ops};
