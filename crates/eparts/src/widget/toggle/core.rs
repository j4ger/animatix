//! The shared `Toggle` engine: field layout, builder, interaction and the
//! control-specific paint dispatch. `mod.rs` re-exports the public surface.

use std::hash::Hash;

use egui::{Color32, Response, Role, Sense, Vec2, WidgetInfo};

use crate::tokens::theme;
use crate::tokens::typography::TextRole;
use crate::widget::traits::finish_response;

use super::{checkbox, switch};

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

/// Build the animation-state base id shared by `Checkbox`/`Switch`.
///
/// The salt is the address of the caller-owned value, but it is namespaced
/// under the parent [`egui::Ui`]'s id: two controls can otherwise collide when
/// their backing values happen to share a recycled stack address, and state
/// would leak across panels.
pub(super) fn animation_id(ui: &egui::Ui, value: impl Hash + std::fmt::Debug) -> egui::Id {
    ui.scope_id().with(value)
}

/// Pick the checkbox/radio/switch slot for the current interaction state.
///
/// Disabled wins over hover; hover applies only to the *unchecked* control
/// (a checked control already reads as active, matching the previous
/// `accent.primary` look).
pub(super) fn toggle_slot(
    t: &theme::Theme,
    value: bool,
    enabled: bool,
    hovered: bool,
) -> theme::Slot {
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
pub(super) fn toggle_mark_color(t: &theme::Theme, enabled: bool) -> Color32 {
    if enabled {
        t.components.toggle.mark
    } else {
        t.components.toggle.disabled.fg
    }
}

/// A boolean toggle control — the shared engine behind [`super::Checkbox`] and [`super::Switch`].
///
/// The `SWITCH` flag selects the painted control: `false` draws a check box and
/// `true` draws a track with a sliding thumb. Prefer the [`super::Checkbox`] / [`super::Switch`]
/// aliases; this type is public only so those aliases can name it.
///
/// State is app-owned: the caller passes `&mut bool`.
pub struct Toggle<'a, const SWITCH: bool> {
    pub(super) value: &'a mut bool,
    pub(super) label: Option<&'a str>,
    pub(super) label_side: Side,
    pub(super) tooltip: &'a str,
}

impl<'a, const SWITCH: bool> Toggle<'a, SWITCH> {
    /// Create a toggle bound to the given boolean.
    pub fn new(value: &'a mut bool) -> Self {
        Self {
            value,
            label: None,
            label_side: Side::Right,
            tooltip: "",
        }
    }

    /// Set the label text.
    pub fn label(mut self, label: &'a str) -> Self {
        self.label = Some(label);
        self
    }

    /// Set which side the label appears on.
    pub fn label_side(mut self, side: Side) -> Self {
        self.label_side = side;
        self
    }

    /// Set a tooltip shown on hover.
    pub fn tooltip(mut self, tip: &'a str) -> Self {
        self.tooltip = tip;
        self
    }
}

impl<'a, const SWITCH: bool> egui::Widget for Toggle<'a, SWITCH> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let t = theme(ui);
        // Namespace the animation id under the parent `Ui` so two toggles in
        // different panels (or frames) can't collide on a recycled address.
        let id = animation_id(ui, self.value as *const bool);

        let s = crate::spatial(ui);
        let control_size = if SWITCH {
            Vec2::new(s.toggle.switch_track_width, s.toggle.switch_track_height)
        } else {
            Vec2::splat(s.row_xs)
        };
        let spacing = s.space_3;
        let font = TextRole::Body.font_id();
        let label_galley = self.label.map(|l| {
            ui.painter().layout_no_wrap(l.to_string(), font.clone(), t.palette.text.primary)
        });
        let label_size = label_galley.as_ref().map(|g| g.size()).unwrap_or(Vec2::ZERO);

        let total_size =
            Vec2::new(control_size.x + spacing + label_size.x, control_size.y.max(label_size.y));
        let (rect, response) = ui.allocate_exact_size(total_size, Sense::click());

        // Position the control and label on the requested side.
        let (control_rect, label_rect) = match self.label_side {
            Side::Right => {
                let control = egui::Rect::from_center_size(
                    rect.center() - Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    control_size,
                );
                let label_pos =
                    egui::pos2(control.right() + spacing, rect.center().y - label_size.y / 2.0);
                (control, egui::Rect::from_min_size(label_pos, label_size))
            },
            Side::Left => {
                let control = egui::Rect::from_center_size(
                    rect.center() + Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    control_size,
                );
                let label_pos = egui::pos2(rect.min.x, rect.center().y - label_size.y / 2.0);
                (control, egui::Rect::from_min_size(label_pos, label_size))
            },
        };

        let enabled = ui.is_enabled();
        let hovered = response.hovered();
        if SWITCH {
            switch::paint(ui, id, &t, &s, control_rect, *self.value, enabled, hovered);
        } else {
            checkbox::paint(ui, id, &t, control_rect, *self.value, enabled, hovered);
        }

        // Draw label
        if let Some(galley) = label_galley {
            ui.painter().galley(label_rect.min, galley, t.palette.text.primary);
        }

        // Toggle on click
        if response.clicked() {
            *self.value = !*self.value;
        }

        let accessible_label = non_empty_or(self.label, if SWITCH { "Switch" } else { "Checkbox" });

        // Tooltip + principle 3 cursor
        let response = finish_response(ui, response, self.tooltip);
        response.widget_info(|| {
            WidgetInfo::selected(
                Role::CheckBox,
                ui.is_enabled(),
                *self.value,
                accessible_label.clone(),
            )
        });
        response
    }
}
