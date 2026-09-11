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
        t.toggle.disabled
    } else if value {
        t.toggle.checked
    } else if hovered {
        t.toggle.hover
    } else {
        t.toggle.unchecked
    }
}

/// Mark colour for a checked toggle control: the dedicated `mark` slot when
/// enabled, dimmed to the disabled slot's foreground when not.
fn toggle_mark_color(t: &theme::Theme, enabled: bool) -> Color32 {
    if enabled {
        t.toggle.mark
    } else {
        t.toggle.disabled.fg
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
        // Namespace the animation id under the parent `Ui` so two checkboxes in
        // different panels (or frames) can't collide on a recycled address.
        let id = animation_id(ui, self.value as *const bool);

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

        // Read the control's state slot from `theme.toggle`. Hover and enabled
        // state come from the response; a disabled control overrides everything.
        let enabled = ui.is_enabled();
        let slot = toggle_slot(&t, *self.value, enabled, response.hovered());

        // Draw checkbox box
        ui.painter().rect_filled(checkbox_rect, RADIUS_M, slot.bg);
        ui.painter().rect_stroke(
            checkbox_rect,
            RADIUS_M,
            Stroke::new(STROKE_WIDTH, slot.border),
            StrokeKind::Inside,
        );

        // Draw animated checkmark
        if check_t > 0.01 {
            let alpha = (check_t * 255.0).round() as u8;
            let scale = 0.5 + 0.5 * check_t;
            let center = checkbox_rect.center();
            let arm = checkbox_rect.width() * 0.28;

            let mark = toggle_mark_color(&t, enabled);
            let check_color = Color32::from_rgba_unmultiplied(mark.r(), mark.g(), mark.b(), alpha);

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
        let response = finish_response(ui, response, self.tooltip);
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

        // Stable id derived from the value pointer + option identity, namespaced
        // under the parent `Ui` so it can't collide across panels.
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (self.value as *const T as usize).hash(&mut hasher);
        self.this_value.hash(&mut hasher);
        let id = animation_id(ui, hasher.finish());

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

        // Radio has no fill: the state slot's `border` carries the ring colour.
        let enabled = ui.is_enabled();
        let slot = toggle_slot(&t, selected, enabled, response.hovered());
        ui.painter().circle_stroke(
            outer_rect.center(),
            outer_size.x / 2.0,
            Stroke::new(STROKE_WIDTH, slot.border),
        );

        // Draw animated inner dot
        if dot_t > 0.01 {
            let dot_radius = (outer_size.x / 2.0 - STROKE_WIDTH) * 0.6 * dot_t;
            let mark = toggle_mark_color(&t, enabled);
            let dot_color = Color32::from_rgba_unmultiplied(
                mark.r(),
                mark.g(),
                mark.b(),
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
        let response = finish_response(ui, response, self.tooltip);
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
        // Namespace the animation id under the parent `Ui` (see `Checkbox`).
        let id = animation_id(ui, self.value as *const bool);

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

        // Track crossfades between the off/on slot fills. Hover brightens the off
        // fill; disabled collapses both endpoints onto the disabled fill.
        let enabled = ui.is_enabled();
        let slot = toggle_slot(&t, *self.value, enabled, response.hovered());
        let off_bg = if !enabled {
            t.toggle.disabled.bg
        } else if response.hovered() {
            t.toggle.hover.bg
        } else {
            t.toggle.unchecked.bg
        };
        let on_bg = if enabled {
            t.toggle.checked.bg
        } else {
            t.toggle.disabled.bg
        };

        // Animated track color crossfade
        let track_color =
            animate_lerp(ui.ctx(), id.with("track"), off_bg, on_bg, *self.value, transition);

        // Draw track
        ui.painter().rect_filled(track_rect, track_height / 2.0, track_color);
        ui.painter().rect_stroke(
            track_rect,
            track_height / 2.0,
            Stroke::new(STROKE_WIDTH, slot.border),
            StrokeKind::Inside,
        );

        // Draw thumb
        let thumb_color = if enabled {
            t.toggle.thumb
        } else {
            t.toggle.disabled.fg
        };
        let thumb_center = Pos2::new(thumb_x, track_center_y);
        ui.painter().circle_filled(thumb_center, thumb_radius, thumb_color);
        ui.painter().circle_stroke(
            thumb_center,
            thumb_radius,
            Stroke::new(STROKE_WIDTH, slot.border),
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
        let response = finish_response(ui, response, self.tooltip);
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

    // ── Animation id namespacing ───────────────────────────────────

    /// Collect `animation_id` results for the same value pointer under
    /// different parent `Ui` ids.
    fn ids_for_same_ptr() -> (egui::Id, egui::Id, egui::Id, egui::Id) {
        let ctx = egui::Context::default();
        let value = 0u32;
        let mut under_a = egui::Id::NULL;
        let mut under_b = egui::Id::NULL;
        let mut top_level = egui::Id::NULL;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            top_level = animation_id(ui, &value as *const u32);
            ui.push_id("panel_a", |ui| {
                under_a = animation_id(ui, &value as *const u32);
            });
            ui.push_id("panel_b", |ui| {
                under_b = animation_id(ui, &value as *const u32);
            });
        });
        // The pre-fix construction: keyed on the raw address alone.
        let legacy = egui::Id::new(&value as *const u32);
        (under_a, under_b, top_level, legacy)
    }

    #[test]
    fn animation_id_is_namespaced_by_parent_ui() {
        let (under_a, under_b, top_level, legacy) = ids_for_same_ptr();
        assert_ne!(
            under_a, under_b,
            "the same value rendered under different parent Uis must not share animation state"
        );
        assert_ne!(under_a, top_level);
        assert_ne!(under_b, top_level);
        assert_ne!(under_a, legacy);
        assert_ne!(under_b, legacy);
    }

    #[test]
    fn toggle_widgets_render_with_namespaced_animation_ids() {
        let ctx = egui::Context::default();
        let mut flag = false;
        let mut choice = 0u32;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.push_id("ns_panel", |ui| {
                ui.add(Checkbox::new(&mut flag).label("Check"));
                ui.add(Switch::new(&mut flag).label("Switch"));
                ui.add(Radio::new(&mut choice, 1).label("Radio"));
            });
        });
    }

    /// Representative slot-resolution test: the toggle control's colour is
    /// chosen from `theme.toggle.*` and the dark/light seeds differ as expected.
    #[test]
    fn toggle_slot_resolves_per_state_in_both_themes() {
        let dark = theme::Theme::dark();
        let light = theme::Theme::light();

        // State selection is theme-independent: disabled > checked > hover > unchecked.
        for t in [&dark, &light] {
            assert_eq!(toggle_slot(t, true, false, true), t.toggle.disabled);
            assert_eq!(toggle_slot(t, true, true, false), t.toggle.checked);
            assert_eq!(toggle_slot(t, false, true, true), t.toggle.hover);
            assert_eq!(toggle_slot(t, false, true, false), t.toggle.unchecked);
        }

        // The checked fill comes from the accent slot, not a widget literal, and
        // the light unchecked fill differs from the dark one.
        assert_eq!(dark.toggle.checked.bg, crate::tokens::semantic::accent::PRIMARY);
        assert_eq!(light.toggle.checked.bg, crate::tokens::semantic::accent::PRIMARY);
        assert_ne!(dark.toggle.unchecked.bg, light.toggle.unchecked.bg);
        assert_ne!(dark.toggle.mark, light.toggle.disabled.fg);
    }

    /// The toggle widgets render through the slots in both themes.
    #[test]
    fn toggle_widgets_render_in_both_themes() {
        let ctx = egui::Context::default();
        for t in [theme::Theme::dark(), theme::Theme::light()] {
            crate::tokens::theme::set_theme(&ctx, t);
            let mut flag = false;
            let mut choice = 0u32;
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.add(Checkbox::new(&mut flag).label("Check"));
                ui.add(Switch::new(&mut flag).label("Switch"));
                ui.add(Radio::new(&mut choice, 1).label("Radio"));
                ui.add_enabled(false, Checkbox::new(&mut flag).label("Disabled"));
                ui.add_enabled(false, Switch::new(&mut flag).label("Disabled"));
            });
        }
    }
}
