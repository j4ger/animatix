//! Themed Checkbox, Radio, and Switch widgets with animated transitions.
//!
//! All three widgets are theme-driven, app-owned state, and use `animate_bool_eased`
//! / `animate_lerp` for crossfades and thumb motion.

use std::hash::{Hash, Hasher};

use egui::{Color32, Pos2, Response, Sense, Stroke, StrokeKind, Vec2, WidgetInfo, WidgetType};

use crate::tokens::motion::{NORMAL, STANDARD, Transition};
use crate::tokens::spatial::{RADIUS_M, STROKE_WIDTH, STROKE_WIDTH_THICK};
use crate::tokens::theme;
use crate::tokens::typography::TextRole;
use crate::widget::anim::{animate_bool_eased, animate_lerp};
use crate::widget::tooltip::text_tooltip;

// ── Side ────────────────────────────────────────────────────────────

/// Which side the label appears on relative to the control.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    /// Label on the right (default).
    #[default]
    Right,
    /// Label on the left.
    Left,
}

fn non_empty_or(label: Option<&str>, fallback: &str) -> String {
    match label {
        Some(l) if !l.trim().is_empty() => l.to_string(),
        _ => fallback.to_owned(),
    }
}

/// Build the animation-state base id shared by `Checkbox`/`Switch`/`Radio`.
///
/// The salt is the address of the caller-owned value, but it is namespaced
/// under the parent [`egui::Ui`]'s id: two controls can otherwise collide when
/// their backing values happen to share a recycled stack address, and state
/// would leak across panels.
fn animation_id(ui: &egui::Ui, value: impl Hash) -> egui::Id {
    ui.id().with(value)
}

/// Apply the default arrow cursor and render tooltips with the grace-period
/// eparts `Tooltip` instead of egui's immediate hover text.
fn finish_response(ui: &mut egui::Ui, response: Response, tooltip: &str) -> Response {
    let response = response.on_hover_cursor(egui::CursorIcon::Default);
    if !tooltip.is_empty() {
        text_tooltip(ui, response.id.with("tooltip"), &response, tooltip);
    }
    response
}

/// Pick the checkbox/radio/switch slot for the current interaction state.
///
/// Disabled wins over hover; hover applies only to the *unchecked* control
/// (a checked control already reads as active, matching the previous
/// `accent.primary` look).
fn toggle_slot(t: &theme::Theme, value: bool, enabled: bool, hovered: bool) -> theme::Slot {
    if !enabled {
        t.components.toggle.disabled
    } else if value {
        t.components.toggle.checked
    } else if hovered {
        t.components.toggle.hover
    } else {
        t.components.toggle.unchecked
    }
}

/// Mark colour for a checked toggle control: the dedicated `mark` slot when
/// enabled, dimmed to the disabled slot's foreground when not.
fn toggle_mark_color(t: &theme::Theme, enabled: bool) -> Color32 {
    if enabled {
        t.components.toggle.mark
    } else {
        t.components.toggle.disabled.fg
    }
}

// ── Checkbox ────────────────────────────────────────────────────────

/// A themed checkbox with an animated checkmark crossfade.
///
/// State is app-owned: the caller passes `&mut bool`. The checkmark fades in/out
/// smoothly (~200 ms, ease-in-out) rather than snapping.
///
/// ## Examples
/// ```ignore
/// let mut enabled = false;
/// ui.add(Checkbox::new(&mut enabled).label("Enable feature"));
/// ```
mod checkbox;
mod radio;
mod switch;
#[cfg(test)]
mod tests;

pub use checkbox::Checkbox;
pub use radio::Radio;
pub use switch::Switch;
