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
use egui::{Align2, Color32, FontId, Pos2, Rect, StrokeKind, Vec2};

use super::PreviewTransform;
use crate::app::design_tokens::spatial::STROKE_WIDTH;

/// Colors used by the layout debug overlay.
struct LayoutDebugStyle {
    container_color: Color32,
    slot_color: Color32,
    size_color: Color32,
    spacing_color: Color32,
}

const RADIUS_M: f32 = 4.0;

/// A stroke used by overlay operations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayStroke {
    pub width: f32,
    pub color: Color32,
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
    Text {
        pos: kurbo::Point,
        anchor: Align2,
        text: String,
        font: FontId,
        color: Color32,
    },
    Screen(ScreenOverlayOp),
}

/// Execute overlay operations onto the canvas painter.
mod execute;
mod guides;
mod impls;
mod interaction;
mod layout;
mod motion;

use impls::*;

pub use execute::execute_overlay_ops;
pub use guides::{actor_label_ops, grid_ops, snap_guide_ops};
pub use interaction::hover_highlight_ops;
pub use layout::{layout_debug_ops, scene_bounds_ops};
pub use motion::{cycle_indicator_ops, motion_path_ops};
