//! Easing Curve Editor
//!
//! Interactive cubic-bezier easing editor. Shows a small preview of the curve
//! with draggable control points P1 and P2.

use crate::app::design_tokens::*;
use egui::{Pos2, Rect, Sense, Stroke, Vec2};

/// State for the easing curve editor widget.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EasingCurveState {
    pub p1x: f32,
    pub p1y: f32,
    pub p2x: f32,
    pub p2y: f32,
}

impl Default for EasingCurveState {
    fn default() -> Self {
        Self {
            p1x: 0.42,
            p1y: 0.0,
            p2x: 0.58,
            p2y: 1.0,
        }
    }
}

impl EasingCurveState {
    pub fn from_array(cp: [f32; 4]) -> Self {
        Self {
            p1x: cp[0],
            p1y: cp[1],
            p2x: cp[2],
            p2y: cp[3],
        }
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.p1x, self.p1y, self.p2x, self.p2y]
    }
}

/// Render an interactive easing curve editor.
///
/// Returns `Some(new_state)` if the user dragged a control point.
pub fn easing_curve_editor(ui: &mut egui::Ui, state: EasingCurveState) -> Option<EasingCurveState> {
    let desired_size = Vec2::new(ui.available_width(), 100.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
    let painter = ui.painter_at(rect);

    // Plot area with padding
    let plot_rect = rect.shrink2(Vec2::new(SPACE_M, SPACE_S));

    // Map normalized (0..1) to plot_rect
    let map = |x: f32, y: f32| -> Pos2 {
        Pos2::new(
            egui::lerp(plot_rect.left()..=plot_rect.right(), x.clamp(0.0, 1.0)),
            egui::lerp(plot_rect.bottom()..=plot_rect.top(), y.clamp(0.0, 1.0)),
        )
    };

    // Background
    painter.rect_filled(rect, RADIUS_M, BG_BASE);
    painter.rect_stroke(rect, RADIUS_M, Stroke::new(STROKE_WIDTH, BORDER), egui::StrokeKind::Outside);

    // Grid
    for i in 0..=4 {
        let t = i as f32 / 4.0;
        let x = egui::lerp(plot_rect.left()..=plot_rect.right(), t);
        let y = egui::lerp(plot_rect.top()..=plot_rect.bottom(), t);
        painter.line_segment(
            [Pos2::new(x, plot_rect.top()), Pos2::new(x, plot_rect.bottom())],
            Stroke::new(STROKE_WIDTH, grid_line()),
        );
        painter.line_segment(
            [Pos2::new(plot_rect.left(), y), Pos2::new(plot_rect.right(), y)],
            Stroke::new(STROKE_WIDTH, grid_line()),
        );
    }

    // Diagonal reference line (linear)
    painter.line_segment(
        [map(0.0, 0.0), map(1.0, 1.0)],
        Stroke::new(1.0, TEXT_DISABLED),
    );

    // Draw curve
    let cp = state.to_array();
    let segments = 40;
    let mut prev = map(0.0, 0.0);
    for s in 1..=segments {
        let t = s as f32 / segments as f32;
        let x = cubic_bezier_x(t, cp);
        let y = cubic_bezier_y(t, cp);
        let curr = map(x, y);
        painter.line_segment([prev, curr], Stroke::new(2.5, ACCENT_BLUE));
        prev = curr;
    }

    // Control points
    let p1 = map(state.p1x, state.p1y);
    let p2 = map(state.p2x, state.p2y);
    let p0 = map(0.0, 0.0);
    let p3 = map(1.0, 1.0);

    // Control lines (dashed-ish via alpha)
    painter.line_segment([p0, p1], Stroke::new(1.0, TEXT_DISABLED.gamma_multiply(0.5)));
    painter.line_segment([p2, p3], Stroke::new(1.0, TEXT_DISABLED.gamma_multiply(0.5)));

    // Endpoints
    painter.circle_filled(p0, 3.0, TEXT_SECONDARY);
    painter.circle_filled(p3, 3.0, TEXT_SECONDARY);

    // Draggable handles
    let handle_radius = 6.0;
    let mut new_state = state;
    let mut changed = false;

    // P1 handle
    let p1_id = ui.id().with("easing_p1");
    let p1_response = ui.interact(Rect::from_center_size(p1, Vec2::splat(handle_radius * 2.0)), p1_id, Sense::drag());
    if p1_response.dragged() {
        let d = p1_response.drag_delta();
        let dx = d.x / plot_rect.width();
        let dy = -d.y / plot_rect.height();
        new_state.p1x = (new_state.p1x + dx).clamp(0.0, 1.0);
        new_state.p1y = (new_state.p1y + dy).clamp(0.0, 1.0);
        changed = true;
    }
    let p1_color = if p1_response.dragged() { AMBER } else { ACCENT_BLUE };
    painter.circle_filled(p1, handle_radius, p1_color);
    painter.circle_stroke(p1, handle_radius + 1.5, Stroke::new(1.5, TEXT_PRIMARY));

    // P2 handle
    let p2_id = ui.id().with("easing_p2");
    let p2_response = ui.interact(Rect::from_center_size(p2, Vec2::splat(handle_radius * 2.0)), p2_id, Sense::drag());
    if p2_response.dragged() {
        let d = p2_response.drag_delta();
        let dx = d.x / plot_rect.width();
        let dy = -d.y / plot_rect.height();
        new_state.p2x = (new_state.p2x + dx).clamp(0.0, 1.0);
        new_state.p2y = (new_state.p2y + dy).clamp(0.0, 1.0);
        changed = true;
    }
    let p2_color = if p2_response.dragged() { AMBER } else { ACCENT_BLUE };
    painter.circle_filled(p2, handle_radius, p2_color);
    painter.circle_stroke(p2, handle_radius + 1.5, Stroke::new(1.5, TEXT_PRIMARY));

    // Hover cursor
    if p1_response.hovered() || p2_response.hovered() || response.hovered() {
        ui.ctx().output_mut(|o| o.cursor_icon = egui::CursorIcon::Crosshair);
    }

    if changed {
        Some(new_state)
    } else {
        None
    }
}

fn cubic_bezier_x(t: f32, cp: [f32; 4]) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let omt = 1.0 - t;
    3.0 * omt * omt * t * cp[0] + 3.0 * omt * t * t * cp[2] + t * t * t
}

fn cubic_bezier_y(t: f32, cp: [f32; 4]) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let omt = 1.0 - t;
    3.0 * omt * omt * t * cp[1] + 3.0 * omt * t * t * cp[3] + t * t * t
}
