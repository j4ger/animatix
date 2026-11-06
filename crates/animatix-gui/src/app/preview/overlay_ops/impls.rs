use egui::{Color32, Pos2, Rect, Stroke};

use super::{LayoutDebugStyle, OverlayStroke, PreviewTransform};

impl LayoutDebugStyle {
    pub(super) fn from_theme(theme: &eparts::Theme) -> Self {
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

/// Convert scene units per screen unit into scene-space overlay sizes.
pub(super) fn px_to_scene(tx: &PreviewTransform, px: f32) -> f32 {
    (px as f64 * tx.scale().0) as f32
}

/// Convert a scene-space point to screen space through the preview transform.
pub(super) fn scene_to_screen(tx: &PreviewTransform, point: kurbo::Point) -> Pos2 {
    tx.scene_to_screen(point)
}

/// Convert a scene-space rectangle to screen space.
pub(super) fn scene_rect_to_screen(tx: &PreviewTransform, rect: &kurbo::Rect) -> Rect {
    let min = scene_to_screen(tx, kurbo::Point::new(rect.x0, rect.y0));
    let max = scene_to_screen(tx, kurbo::Point::new(rect.x1, rect.y1));
    Rect::from_min_max(min, max)
}

pub(super) fn scene_radius_to_screen(tx: &PreviewTransform, radius: f32) -> f32 {
    (radius as f64 / tx.scale().0) as f32
}

/// Convert a screen-space rectangle back to scene space.
pub(super) fn screen_rect_to_scene(tx: &PreviewTransform, rect: Rect) -> kurbo::Rect {
    let min = tx.screen_to_scene(rect.min);
    let max = tx.screen_to_scene(rect.max);
    kurbo::Rect::new(min.x.min(max.x), min.y.min(max.y), min.x.max(max.x), min.y.max(max.y))
}

/// The scene-space viewport currently visible in the preview rect.
pub(super) fn scene_viewport(tx: &PreviewTransform) -> kurbo::Rect {
    screen_rect_to_scene(tx, tx.preview_rect)
}
