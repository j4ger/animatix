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
pub struct Checkbox<'a> {
    value: &'a mut bool,
    label: Option<&'a str>,
    label_side: Side,
    tooltip: &'a str,
}

impl<'a> Checkbox<'a> {
    /// Create a checkbox bound to the given boolean.
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

impl<'a> egui::Widget for Checkbox<'a> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let t = theme(ui);
        let id = egui::Id::new(self.value as *const _);

        // Layout calculations
        let s = crate::spatial(ui);
        let box_size = Vec2::splat(s.row_xs);
        let spacing = s.space_3;
        let font = TextRole::Body.font_id();
        let label_galley = self
            .label
            .map(|l| ui.painter().layout_no_wrap(l.to_string(), font.clone(), t.text.primary));
        let label_size = label_galley.as_ref().map(|g| g.size()).unwrap_or(Vec2::ZERO);

        let total_size =
            Vec2::new(box_size.x + spacing + label_size.x, box_size.y.max(label_size.y));
        let (rect, response) = ui.allocate_exact_size(total_size, Sense::click());

        // Position checkbox and label
        let (checkbox_rect, label_rect) = match self.label_side {
            Side::Right => {
                let cb = egui::Rect::from_center_size(
                    rect.center() - Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    box_size,
                );
                let label_pos =
                    egui::pos2(cb.right() + spacing, rect.center().y - label_size.y / 2.0);
                (cb, egui::Rect::from_min_size(label_pos, label_size))
            },
            Side::Left => {
                let cb = egui::Rect::from_center_size(
                    rect.center() + Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    box_size,
                );
                let label_pos = egui::pos2(rect.min.x, rect.center().y - label_size.y / 2.0);
                (cb, egui::Rect::from_min_size(label_pos, label_size))
            },
        };

        // Animated checkmark crossfade
        let transition = Transition {
            duration: NORMAL,
            easing: STANDARD,
        };
        let check_t = animate_bool_eased(ui.ctx(), id.with("check"), *self.value, transition);

        // Draw checkbox box
        let box_bg = if *self.value {
            t.accent.primary
        } else {
            t.surface.widget
        };
        let box_border = if *self.value {
            t.accent.primary
        } else {
            t.border.default
        };

        ui.painter().rect_filled(checkbox_rect, RADIUS_M, box_bg);
        ui.painter().rect_stroke(
            checkbox_rect,
            RADIUS_M,
            Stroke::new(STROKE_WIDTH, box_border),
            StrokeKind::Inside,
        );

        // Draw animated checkmark
        if check_t > 0.01 {
            let alpha = (check_t * 255.0).round() as u8;
            let scale = 0.5 + 0.5 * check_t;
            let center = checkbox_rect.center();
            let arm = checkbox_rect.width() * 0.28;

            let check_color = Color32::from_rgba_unmultiplied(
                t.text.on_accent.r(),
                t.text.on_accent.g(),
                t.text.on_accent.b(),
                alpha,
            );

            let p1 = center + Vec2::new(-arm * 0.9 * scale, arm * 0.1 * scale);
            let p2 = center + Vec2::new(-arm * 0.1 * scale, arm * 0.8 * scale);
            let p3 = center + Vec2::new(arm * 0.9 * scale, -arm * 0.9 * scale);

            ui.painter()
                .line_segment([p1, p2], Stroke::new(STROKE_WIDTH_THICK, check_color));
            ui.painter()
                .line_segment([p2, p3], Stroke::new(STROKE_WIDTH_THICK, check_color));
        }

        // Draw label
        if let Some(galley) = label_galley {
            ui.painter().galley(label_rect.min, galley, t.text.primary);
        }

        // Toggle on click
        if response.clicked() {
            *self.value = !*self.value;
        }

        let accessible_label = non_empty_or(self.label, "Checkbox");

        // Tooltip + principle 3 cursor
        let response = if !self.tooltip.is_empty() {
            response.on_hover_cursor(egui::CursorIcon::Default).on_hover_text(self.tooltip)
        } else {
            response.on_hover_cursor(egui::CursorIcon::Default)
        };
        response.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::Checkbox,
                ui.is_enabled(),
                *self.value,
                accessible_label.clone(),
            )
        });
        response
    }
}

// ── Radio ───────────────────────────────────────────────────────────

/// A themed radio button with an animated inner dot.
///
/// State is app-owned: the caller passes `&mut T` and the selected value.
/// Selected when `*value == this_value`.
///
/// ## Examples
/// ```ignore
/// enum Choice { A, B, C }
/// let mut choice = Choice::A;
/// ui.add(Radio::new(&mut choice, Choice::B).label("Option B"));
/// ```
pub struct Radio<'a, T> {
    value: &'a mut T,
    this_value: T,
    label: Option<&'a str>,
    label_side: Side,
    tooltip: &'a str,
}

impl<'a, T: PartialEq + Clone + Hash> Radio<'a, T> {
    /// Create a radio button. It is selected when `*value == this_value`.
    pub fn new(value: &'a mut T, this_value: T) -> Self {
        Self {
            value,
            this_value,
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

impl<'a, T: PartialEq + Clone + Hash> egui::Widget for Radio<'a, T> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let t = theme(ui);
        let selected = *self.value == self.this_value;

        // Stable id derived from the value pointer + option identity
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (self.value as *const T as usize).hash(&mut hasher);
        self.this_value.hash(&mut hasher);
        let id = egui::Id::new(hasher.finish());

        // Layout
        let s = crate::spatial(ui);
        let outer_size = Vec2::splat(s.toggle.radio_size);
        let spacing = s.space_3;
        let font = TextRole::Body.font_id();
        let label_galley = self
            .label
            .map(|l| ui.painter().layout_no_wrap(l.to_string(), font.clone(), t.text.primary));
        let label_size = label_galley.as_ref().map(|g| g.size()).unwrap_or(Vec2::ZERO);

        let total_size =
            Vec2::new(outer_size.x + spacing + label_size.x, outer_size.y.max(label_size.y));
        let (rect, response) = ui.allocate_exact_size(total_size, Sense::click());

        let (outer_rect, label_rect) = match self.label_side {
            Side::Right => {
                let outer = egui::Rect::from_center_size(
                    rect.center() - Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    outer_size,
                );
                let label_pos =
                    egui::pos2(outer.right() + spacing, rect.center().y - label_size.y / 2.0);
                (outer, egui::Rect::from_min_size(label_pos, label_size))
            },
            Side::Left => {
                let outer = egui::Rect::from_center_size(
                    rect.center() + Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    outer_size,
                );
                let label_pos = egui::pos2(rect.min.x, rect.center().y - label_size.y / 2.0);
                (outer, egui::Rect::from_min_size(label_pos, label_size))
            },
        };

        // Animated dot progress
        let transition = Transition {
            duration: NORMAL,
            easing: STANDARD,
        };
        let dot_t = animate_bool_eased(ui.ctx(), id.with("dot"), selected, transition);

        // Draw outer circle
        let outer_color = if selected {
            t.accent.primary
        } else {
            t.border.default
        };
        ui.painter().circle_stroke(
            outer_rect.center(),
            outer_size.x / 2.0,
            Stroke::new(STROKE_WIDTH, outer_color),
        );

        // Draw animated inner dot
        if dot_t > 0.01 {
            let dot_radius = (outer_size.x / 2.0 - STROKE_WIDTH) * 0.6 * dot_t;
            let dot_color = Color32::from_rgba_unmultiplied(
                t.accent.primary.r(),
                t.accent.primary.g(),
                t.accent.primary.b(),
                (dot_t * 255.0).round() as u8,
            );
            ui.painter().circle_filled(outer_rect.center(), dot_radius, dot_color);
        }

        // Draw label
        if let Some(galley) = label_galley {
            ui.painter().galley(label_rect.min, galley, t.text.primary);
        }

        // Set value on click
        if response.clicked() {
            *self.value = self.this_value.clone();
        }

        let accessible_label = non_empty_or(self.label, "Radio");

        // Tooltip + principle 3 cursor
        let response = if !self.tooltip.is_empty() {
            response.on_hover_cursor(egui::CursorIcon::Default).on_hover_text(self.tooltip)
        } else {
            response.on_hover_cursor(egui::CursorIcon::Default)
        };
        response.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::RadioButton,
                ui.is_enabled(),
                selected,
                accessible_label.clone(),
            )
        });
        response
    }
}

// ── Switch ──────────────────────────────────────────────────────────

/// A themed switch (toggle) with animated thumb and track crossfade.
///
/// State is app-owned: the caller passes `&mut bool`.
///
/// ## Examples
/// ```ignore
/// let mut on = false;
/// ui.add(Switch::new(&mut on).label("Dark mode"));
/// ```
pub struct Switch<'a> {
    value: &'a mut bool,
    label: Option<&'a str>,
    label_side: Side,
    tooltip: &'a str,
}

impl<'a> Switch<'a> {
    /// Create a switch bound to the given boolean.
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

impl<'a> egui::Widget for Switch<'a> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let t = theme(ui);
        let id = egui::Id::new(self.value as *const _);

        // Dimensions
        let s = crate::spatial(ui);
        let track_height = s.toggle.switch_track_height;
        let track_width = s.toggle.switch_track_width;
        let thumb_radius = s.toggle.switch_thumb_radius;
        let spacing = s.space_3;

        let font = TextRole::Body.font_id();
        let label_galley = self
            .label
            .map(|l| ui.painter().layout_no_wrap(l.to_string(), font.clone(), t.text.primary));
        let label_size = label_galley.as_ref().map(|g| g.size()).unwrap_or(Vec2::ZERO);

        let total_size =
            Vec2::new(track_width + spacing + label_size.x, track_height.max(label_size.y));
        let (rect, response) = ui.allocate_exact_size(total_size, Sense::click());

        // Position track and label
        let (track_rect, label_rect) = match self.label_side {
            Side::Right => {
                let track = egui::Rect::from_center_size(
                    rect.center() - Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    Vec2::new(track_width, track_height),
                );
                let label_pos =
                    egui::pos2(track.right() + spacing, rect.center().y - label_size.y / 2.0);
                (track, egui::Rect::from_min_size(label_pos, label_size))
            },
            Side::Left => {
                let track = egui::Rect::from_center_size(
                    rect.center() + Vec2::new(label_size.x / 2.0 + spacing / 2.0, 0.0),
                    Vec2::new(track_width, track_height),
                );
                let label_pos = egui::pos2(rect.min.x, rect.center().y - label_size.y / 2.0);
                (track, egui::Rect::from_min_size(label_pos, label_size))
            },
        };

        let track_center_y = track_rect.center().y;
        let left_x = track_rect.left() + thumb_radius;
        let right_x = track_rect.right() - thumb_radius;

        let transition = Transition {
            duration: NORMAL,
            easing: STANDARD,
        };

        // Animated thumb position
        let thumb_x =
            animate_lerp(ui.ctx(), id.with("thumb"), left_x, right_x, *self.value, transition);

        // Animated track color crossfade
        let track_color = animate_lerp(
            ui.ctx(),
            id.with("track"),
            t.surface.widget,
            t.accent.primary,
            *self.value,
            transition,
        );

        // Draw track
        ui.painter().rect_filled(track_rect, track_height / 2.0, track_color);
        ui.painter().rect_stroke(
            track_rect,
            track_height / 2.0,
            Stroke::new(STROKE_WIDTH, t.border.default),
            StrokeKind::Inside,
        );

        // Draw thumb
        let thumb_center = Pos2::new(thumb_x, track_center_y);
        ui.painter().circle_filled(thumb_center, thumb_radius, t.text.on_accent);
        ui.painter().circle_stroke(
            thumb_center,
            thumb_radius,
            Stroke::new(STROKE_WIDTH, t.border.default),
        );

        // Draw label
        if let Some(galley) = label_galley {
            ui.painter().galley(label_rect.min, galley, t.text.primary);
        }

        // Toggle on click
        if response.clicked() {
            *self.value = !*self.value;
        }

        let accessible_label = non_empty_or(self.label, "Switch");

        // Tooltip + principle 3 cursor
        let response = if !self.tooltip.is_empty() {
            response.on_hover_cursor(egui::CursorIcon::Default).on_hover_text(self.tooltip)
        } else {
            response.on_hover_cursor(egui::CursorIcon::Default)
        };
        response.widget_info(|| {
            WidgetInfo::selected(
                WidgetType::Checkbox,
                ui.is_enabled(),
                *self.value,
                accessible_label.clone(),
            )
        });
        response
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Side ────────────────────────────────────────────────────────

    #[test]
    fn side_default_is_right() {
        assert_eq!(Side::default(), Side::Right);
    }

    // ── Checkbox builder ───────────────────────────────────────────

    #[test]
    fn checkbox_builder_defaults() {
        let mut val = false;
        let ptr = &mut val as *mut _;
        let cb = Checkbox::new(&mut val);
        assert_eq!(cb.value as *const _, ptr as *const _);
        assert!(cb.label.is_none());
        assert_eq!(cb.label_side, Side::Right);
        assert_eq!(cb.tooltip, "");
    }

    #[test]
    fn checkbox_builder_label() {
        let mut val = false;
        let cb = Checkbox::new(&mut val).label("Hello");
        assert_eq!(cb.label, Some("Hello"));
    }

    #[test]
    fn checkbox_builder_label_side() {
        let mut val = false;
        let cb = Checkbox::new(&mut val).label_side(Side::Left);
        assert_eq!(cb.label_side, Side::Left);
    }

    #[test]
    fn checkbox_builder_tooltip() {
        let mut val = false;
        let cb = Checkbox::new(&mut val).tooltip("tip");
        assert_eq!(cb.tooltip, "tip");
    }

    #[test]
    fn checkbox_builder_chaining() {
        let mut val = true;
        let cb = Checkbox::new(&mut val).label("L").label_side(Side::Left).tooltip("T");
        assert_eq!(cb.label, Some("L"));
        assert_eq!(cb.label_side, Side::Left);
        assert_eq!(cb.tooltip, "T");
    }

    // ── Radio builder ──────────────────────────────────────────────

    #[test]
    fn radio_builder_defaults() {
        let mut val = 0u32;
        let r = Radio::new(&mut val, 1);
        assert_eq!(r.this_value, 1);
        assert!(r.label.is_none());
        assert_eq!(r.label_side, Side::Right);
        assert_eq!(r.tooltip, "");
    }

    #[test]
    fn radio_builder_label() {
        let mut val = 0u32;
        let r = Radio::new(&mut val, 1).label("Opt");
        assert_eq!(r.label, Some("Opt"));
    }

    #[test]
    fn radio_builder_label_side() {
        let mut val = 0u32;
        let r = Radio::new(&mut val, 1).label_side(Side::Left);
        assert_eq!(r.label_side, Side::Left);
    }

    #[test]
    fn radio_builder_tooltip() {
        let mut val = 0u32;
        let r = Radio::new(&mut val, 1).tooltip("tip");
        assert_eq!(r.tooltip, "tip");
    }

    #[test]
    fn radio_builder_chaining() {
        let mut val = 0u32;
        let r = Radio::new(&mut val, 2).label("L").label_side(Side::Left).tooltip("T");
        assert_eq!(r.this_value, 2);
        assert_eq!(r.label, Some("L"));
        assert_eq!(r.label_side, Side::Left);
        assert_eq!(r.tooltip, "T");
    }

    #[test]
    fn radio_selection_logic() {
        let mut val = 1u32;
        let r = Radio::new(&mut val, 1);
        assert_eq!(*r.value, r.this_value);
    }

    // ── Switch builder ─────────────────────────────────────────────

    #[test]
    fn switch_builder_defaults() {
        let mut val = false;
        let s = Switch::new(&mut val);
        assert!(s.label.is_none());
        assert_eq!(s.label_side, Side::Right);
        assert_eq!(s.tooltip, "");
    }

    #[test]
    fn switch_builder_label() {
        let mut val = false;
        let s = Switch::new(&mut val).label("On");
        assert_eq!(s.label, Some("On"));
    }

    #[test]
    fn switch_builder_label_side() {
        let mut val = false;
        let s = Switch::new(&mut val).label_side(Side::Left);
        assert_eq!(s.label_side, Side::Left);
    }

    #[test]
    fn switch_builder_tooltip() {
        let mut val = false;
        let s = Switch::new(&mut val).tooltip("tip");
        assert_eq!(s.tooltip, "tip");
    }

    #[test]
    fn switch_builder_chaining() {
        let mut val = true;
        let s = Switch::new(&mut val).label("L").label_side(Side::Left).tooltip("T");
        assert_eq!(s.label, Some("L"));
        assert_eq!(s.label_side, Side::Left);
        assert_eq!(s.tooltip, "T");
    }
}
