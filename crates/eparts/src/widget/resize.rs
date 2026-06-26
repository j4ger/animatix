//! `ResizeHandle` — a thin draggable splitter widget.
//!
//! Painted as a 1 px line with a ±4 px padded hitbox for easy grabbing.
//! Returns the accumulated drag delta per frame so the caller can adjust
//! panel sizes directly (app-owned state, per §5 of the state contract).
//!
//! Highlight colour switches between `border::HOVER` (idle hover) and
//! `accent::PRIMARY` while the handle is being dragged.

use crate::tokens::spatial::{RADIUS_S, SPACE_S, STROKE_WIDTH};
use egui::{CornerRadius, CursorIcon, Id, Rect, Sense, Ui, Vec2};

// ── Public API ─────────────────────────────────────────────────────────────────

/// Axis of the resize handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ResizeAxis {
    /// Horizontal drag moves along the X axis (splits left/right panels).
    #[default]
    Horizontal,
    /// Vertical drag moves along the Y axis (splits top/bottom panels).
    Vertical,
}

impl ResizeAxis {
    /// Cursor icon shown when hovering or dragging.
    fn cursor(self) -> CursorIcon {
        match self {
            Self::Horizontal => CursorIcon::ResizeHorizontal,
            Self::Vertical => CursorIcon::ResizeVertical,
        }
    }

    /// Width of the visual line (px).
    fn visual_width(self) -> f32 {
        STROKE_WIDTH
    }

    /// Main axis drag direction: returns the delta component to expose.
    fn drag_component(self, delta: egui::Vec2) -> f32 {
        match self {
            Self::Horizontal => delta.x,
            Self::Vertical => delta.y,
        }
    }
}

/// A stateless resize-handle widget.
///
/// Returns the drag delta this frame (0.0 when idle). The caller accumulates
/// this value into the panel's stored size — the handle stores nothing
/// across frames.
///
/// ```ignore
/// let delta = ResizeHandle::new(ui.id().with("h_split"), ResizeAxis::Horizontal).show(ui);
/// left_width = (left_width + delta).clamp(100.0, ui.available_width() - 100.0);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct ResizeHandle {
    #[allow(dead_code)] // Stored for animation state keying if needed in future
    id: Id,
    axis: ResizeAxis,
    hit_pad: f32,
}

impl ResizeHandle {
    /// Create a new resize handle with the given id and axis.
    pub fn new(id: impl Into<Id>, axis: ResizeAxis) -> Self {
        Self { id: id.into(), axis, hit_pad: SPACE_S }
    }

    /// Set the extra padding (px) added to the visual width to enlarge the
    /// hitbox. Defaults to [`SPACE_S`] (4 px each side for a horizontal handle).
    pub fn with_hit_pad(mut self, pad: f32) -> Self {
        self.hit_pad = pad;
        self
    }

    /// Render the resize handle and return the drag delta this frame (0.0 when idle).
    pub fn show(&self, ui: &mut Ui) -> f32 {
        let t = crate::tokens::theme::theme(ui);
        let visual = self.axis.visual_width();
        let pad = self.hit_pad;

        // Allocate the full hitbox rect (visual line + padding both sides)
        let total_size = match self.axis {
            ResizeAxis::Horizontal => Vec2::new(visual + 2.0 * pad, ui.available_height()),
            ResizeAxis::Vertical => Vec2::new(ui.available_width(), visual + 2.0 * pad),
        };

        let (rect, response) = ui.allocate_exact_size(total_size, Sense::drag());

        // The visual line is centered inside the allocated rect.
        let visual_rect = match self.axis {
            ResizeAxis::Horizontal => Rect::from_min_size(
                rect.center() - Vec2::new(visual / 2.0, 0.0),
                Vec2::new(visual, rect.height()),
            ),
            ResizeAxis::Vertical => Rect::from_min_size(
                rect.center() - Vec2::new(0.0, visual / 2.0),
                Vec2::new(rect.width(), visual),
            ),
        };

        // ── Paint the visual line ──────────────────────────────────────────
        let is_hovered = response.hovered();
        let is_dragged = response.dragged();

        let (stroke_color, line_width) = if is_dragged {
            (t.accent.primary, STROKE_WIDTH + 0.5)
        } else if is_hovered {
            (t.border.strong, STROKE_WIDTH)
        } else {
            (t.border.default, STROKE_WIDTH)
        };

        let line_endpoints = match self.axis {
            ResizeAxis::Horizontal => [visual_rect.center_top(), visual_rect.center_bottom()],
            ResizeAxis::Vertical => [visual_rect.left_center(), visual_rect.right_center()],
        };

        ui.painter().line_segment(
            line_endpoints,
            egui::Stroke::new(line_width, stroke_color),
        );

        // ── Paint a faint background highlight while dragging ──────────────
        if is_dragged {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(RADIUS_S as u8),
                t.accent.faint,
            );
        }

        // ── Cursor ─────────────────────────────────────────────────────────
        if is_hovered || is_dragged {
            ui.ctx().set_cursor_icon(self.axis.cursor());
        }

        // ── Return delta on the main axis ──────────────────────────────────
        self.axis.drag_component(response.drag_delta())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resize_axis_cursor_icons() {
        assert_eq!(ResizeAxis::Horizontal.cursor(), CursorIcon::ResizeHorizontal);
        assert_eq!(ResizeAxis::Vertical.cursor(), CursorIcon::ResizeVertical);
    }

    #[test]
    fn resize_handle_new_defaults() {
        let h = ResizeHandle::new("test_id", ResizeAxis::Horizontal);
        assert_eq!(h.axis, ResizeAxis::Horizontal);
        assert_eq!(h.hit_pad, SPACE_S);
    }

    #[test]
    fn resize_handle_builder_chaining() {
        let h = ResizeHandle::new("test_id", ResizeAxis::Vertical).with_hit_pad(8.0);
        assert_eq!(h.hit_pad, 8.0);
    }

    #[test]
    fn resize_axis_drag_component_horizontal() {
        let d = ResizeAxis::Horizontal.drag_component(egui::vec2(42.0, -7.0));
        assert_eq!(d, 42.0);
    }

    #[test]
    fn resize_axis_drag_component_vertical() {
        let d = ResizeAxis::Vertical.drag_component(egui::vec2(42.0, -7.0));
        assert_eq!(d, -7.0);
    }
}
