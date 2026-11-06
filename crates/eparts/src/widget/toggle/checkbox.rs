use egui::{Color32, Stroke, StrokeKind, Vec2};

use crate::tokens::motion::{NORMAL, STANDARD, Transition};
use crate::tokens::spatial::{RADIUS_M, STROKE_WIDTH, STROKE_WIDTH_THICK};
use crate::tokens::theme;
use crate::widget::anim::animate_bool_eased;

use super::core::{toggle_mark_color, toggle_slot};

/// Paint a checkbox control: the box, its border, and the animated checkmark.
pub(super) fn paint(
    ui: &egui::Ui,
    id: egui::Id,
    t: &theme::Theme,
    rect: egui::Rect,
    value: bool,
    enabled: bool,
    hovered: bool,
) {
    // Animated checkmark crossfade.
    let transition = Transition {
        duration: NORMAL,
        easing: STANDARD,
    };
    let check_t = animate_bool_eased(ui.ctx(), id.with("check"), value, transition);

    // Read the control's state slot from `theme.toggle`. Hover and enabled
    // state come from the caller; a disabled control overrides everything.
    let slot = toggle_slot(t, value, enabled, hovered);

    // Draw checkbox box
    ui.painter().rect_filled(rect, RADIUS_M, slot.bg);
    ui.painter().rect_stroke(
        rect,
        RADIUS_M,
        Stroke::new(STROKE_WIDTH, slot.border),
        StrokeKind::Inside,
    );

    // Draw animated checkmark
    if check_t > 0.01 {
        let alpha = (check_t * 255.0).round() as u8;
        let scale = 0.5 + 0.5 * check_t;
        let center = rect.center();
        let arm = rect.width() * 0.28;

        let mark = toggle_mark_color(t, enabled);
        let check_color = Color32::from_rgba_unmultiplied(mark.r(), mark.g(), mark.b(), alpha);

        let p1 = center + Vec2::new(-arm * 0.9 * scale, arm * 0.1 * scale);
        let p2 = center + Vec2::new(-arm * 0.1 * scale, arm * 0.8 * scale);
        let p3 = center + Vec2::new(arm * 0.9 * scale, -arm * 0.9 * scale);

        ui.painter()
            .line_segment([p1, p2], Stroke::new(STROKE_WIDTH_THICK, check_color));
        ui.painter()
            .line_segment([p2, p3], Stroke::new(STROKE_WIDTH_THICK, check_color));
    }
}
