use egui::{Pos2, Stroke, StrokeKind};

use crate::tokens::motion::{NORMAL, STANDARD, Transition};
use crate::tokens::spatial::{STROKE_WIDTH, Spatial};
use crate::tokens::theme;
use crate::widget::anim::animate_lerp;

use super::core::toggle_slot;

/// Paint a switch control: the crossfading track and the sliding thumb.
pub(super) fn paint(
    ui: &egui::Ui,
    id: egui::Id,
    t: &theme::Theme,
    s: &Spatial,
    rect: egui::Rect,
    value: bool,
    enabled: bool,
    hovered: bool,
) {
    let thumb_radius = s.toggle.switch_thumb_radius;
    let track_center_y = rect.center().y;
    let left_x = rect.left() + thumb_radius;
    let right_x = rect.right() - thumb_radius;

    let transition = Transition {
        duration: NORMAL,
        easing: STANDARD,
    };

    // Animated thumb position
    let thumb_x = animate_lerp(ui.ctx(), id.with("thumb"), left_x, right_x, value, transition);

    // Track crossfades between the off/on slot fills. Hover brightens the off
    // fill; disabled collapses both endpoints onto the disabled fill.
    let slot = toggle_slot(t, value, enabled, hovered);
    let off_bg = if !enabled {
        t.components.toggle.disabled.bg
    } else if hovered {
        t.components.toggle.hover.bg
    } else {
        t.components.toggle.unchecked.bg
    };
    let on_bg = if enabled {
        t.components.toggle.checked.bg
    } else {
        t.components.toggle.disabled.bg
    };

    // Animated track color crossfade
    let track_color = animate_lerp(ui.ctx(), id.with("track"), off_bg, on_bg, value, transition);

    // Draw track
    ui.painter().rect_filled(rect, rect.height() / 2.0, track_color);
    ui.painter().rect_stroke(
        rect,
        rect.height() / 2.0,
        Stroke::new(STROKE_WIDTH, slot.border),
        StrokeKind::Inside,
    );

    // Draw thumb
    let thumb_color = if enabled {
        t.components.toggle.thumb
    } else {
        t.components.toggle.disabled.fg
    };
    let thumb_center = Pos2::new(thumb_x, track_center_y);
    ui.painter().circle_filled(thumb_center, thumb_radius, thumb_color);
    ui.painter()
        .circle_stroke(thumb_center, thumb_radius, Stroke::new(STROKE_WIDTH, slot.border));
}
