//! Themed text and number input widgets (C2 + C3).
//!
//! ## Usage
//! ```ignore
//! let mut text = String::new();
//! let resp = TextField::new(&mut text)
//!     .placeholder("Search…")
//!     .cleanable(true)
//!     .show(ui);
//!
//! let mut value: f64 = 0.0;
//! let resp = NumberField::new(&mut value)
//!     .range(0.0..=100.0)
//!     .suffix(" px")
//!     .show(ui);
//! ```

use egui::{CornerRadius, DragValue, Margin, Response, Stroke, TextEdit, Ui};

use crate::tokens::spatial::{RADIUS_M, STROKE_WIDTH};
use crate::tokens::theme;

// ── TextField (C2) ─────────────────────────────────────────────────

type ValidateFn<'a> = Box<dyn Fn(&str) -> bool + 'a>;

/// A themed single-line text input.
///
/// Renders inside a frame that tracks `theme.input.{normal, hover, focus, invalid, disabled}`
/// slots. Supports optional prefix/suffix labels, a clear button, password masking, and
/// a validation predicate that flips the border to the `invalid` slot when it returns `false`.
pub struct TextField<'a> {
    buf: &'a mut String,
    placeholder: Option<String>,
    prefix: Option<String>,
    suffix: Option<String>,
    cleanable: bool,
    password: bool,
    desired_width: Option<f32>,
    validate: Option<ValidateFn<'a>>,
    enabled: bool,
}

impl<'a> TextField<'a> {
    pub fn new(buf: &'a mut String) -> Self {
        Self {
            buf,
            placeholder: None,
            prefix: None,
            suffix: None,
            cleanable: false,
            password: false,
            desired_width: None,
            validate: None,
            enabled: true,
        }
    }

    pub fn placeholder(mut self, text: &str) -> Self {
        self.placeholder = Some(text.to_owned());
        self
    }

    pub fn prefix(mut self, text: &str) -> Self {
        self.prefix = Some(text.to_owned());
        self
    }

    pub fn suffix(mut self, text: &str) -> Self {
        self.suffix = Some(text.to_owned());
        self
    }

    pub fn cleanable(mut self, yes: bool) -> Self {
        self.cleanable = yes;
        self
    }

    pub fn password(mut self, yes: bool) -> Self {
        self.password = yes;
        self
    }

    pub fn desired_width(mut self, w: f32) -> Self {
        self.desired_width = Some(w);
        self
    }

    /// Attach a validation predicate. When `f(text)` returns `false` the field
    /// renders with the `theme.input.invalid` border slot.
    pub fn validate(mut self, f: impl Fn(&str) -> bool + 'a) -> Self {
        self.validate = Some(Box::new(f));
        self
    }

    pub fn enabled(mut self, yes: bool) -> Self {
        self.enabled = yes;
        self
    }

    /// Render the field and return the inner [`egui::Response`].
    pub fn show(self, ui: &mut Ui) -> TextFieldResponse {
        let t = theme(ui);
        let s = crate::spatial(ui);

        // Pre-compute borrow-safe values before giving buf to TextEdit.
        let is_invalid = self.validate.as_ref().is_some_and(|f| !f(self.buf));
        let buf_nonempty = !self.buf.is_empty();
        let show_clear = self.cleanable && buf_nonempty;

        let desired_width = self.desired_width.unwrap_or_else(|| ui.available_width());
        let row_height = ui.text_style_height(&egui::TextStyle::Body) + s.space_3 * 2.0;
        let inner_margin = s.space_3;
        let radius = CornerRadius::same(RADIUS_M as u8);

        let (outer_rect, outer_resp) = ui.allocate_exact_size(
            egui::vec2(desired_width, row_height),
            egui::Sense::hover(),
        );

        if !ui.is_rect_visible(outer_rect) {
            return TextFieldResponse { response: outer_resp, changed: false };
        }

        // Initial slot for text color (focus resolved after rendering).
        let fg = if !self.enabled { t.input.disabled.fg } else { t.input.normal.fg };

        // Reserve background + border shape slots BEFORE rendering the text
        // content so they paint behind it. egui appends shapes in call order,
        // so painting the (opaque) fill after the TextEdit would cover the text.
        let painter = ui.painter_at(outer_rect);
        let bg_idx = painter.add(egui::Shape::Noop);
        let border_idx = painter.add(egui::Shape::Noop);
        let mut child_ui = ui.new_child(
            egui::UiBuilder::new().max_rect(outer_rect.shrink(inner_margin)),
        );

        let mut cleared = false;
        let mut had_focus = false;

        child_ui.add_enabled_ui(self.enabled, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(row_height - inner_margin * 2.0);
                ui.spacing_mut().item_spacing.x = s.space_2;

                if let Some(ref pre) = self.prefix {
                    ui.label(egui::RichText::new(pre).color(fg));
                }

                let mut te = TextEdit::singleline(self.buf)
                    .frame(egui::Frame::NONE)
                    .text_color(fg)
                    .margin(Margin::ZERO)
                    .desired_width(f32::INFINITY);

                if self.password { te = te.password(true); }
                if let Some(ref ph) = self.placeholder { te = te.hint_text(ph.as_str()); }

                let te_resp = ui.add(te);
                had_focus = te_resp.has_focus();

                if let Some(ref suf) = self.suffix {
                    ui.label(egui::RichText::new(suf).color(t.text.muted));
                }

                if show_clear {
                    let btn = egui::Button::new(
                        egui::RichText::new("✕").size(10.0).color(t.text.muted),
                    ).frame(false);
                    if ui.add(btn).clicked() {
                        cleared = true;
                    }
                }
            });
        });

        if cleared { self.buf.clear(); }

        let is_hovered = outer_resp.hovered();
        let active_slot = if !self.enabled {
            t.input.disabled
        } else if is_invalid {
            t.input.invalid
        } else if had_focus {
            t.input.focus
        } else if is_hovered {
            t.input.hover
        } else {
            t.input.normal
        };

        painter.set(
            bg_idx,
            egui::epaint::RectShape::filled(outer_rect, radius, active_slot.bg),
        );
        painter.set(
            border_idx,
            egui::epaint::RectShape::stroke(
                outer_rect,
                radius,
                Stroke::new(STROKE_WIDTH, active_slot.border),
                egui::StrokeKind::Inside,
            ),
        );

        TextFieldResponse { response: outer_resp, changed: cleared }
    }
}

/// Return value from [`TextField::show`].
pub struct TextFieldResponse {
    /// The egui response for the outer frame allocation.
    pub response: Response,
    /// Whether the buffer was changed this frame (clear button was clicked).
    pub changed: bool,
}

// ── NumberField (C3) ───────────────────────────────────────────────

/// A themed numeric input backed by [`egui::DragValue`].
///
/// Supports drag-to-change, typing, clamping via `.range()`, step size, and a
/// suffix label (e.g. `" px"`, `" s"`). The frame matches `theme.input.*` slots,
/// keeping visual consistency with [`TextField`].
pub struct NumberField<'a> {
    value: &'a mut f64,
    range: Option<std::ops::RangeInclusive<f64>>,
    step: Option<f64>,
    speed: Option<f32>,
    suffix: Option<String>,
    desired_width: Option<f32>,
    enabled: bool,
}

impl<'a> NumberField<'a> {
    pub fn new(value: &'a mut f64) -> Self {
        Self {
            value,
            range: None,
            step: None,
            speed: None,
            suffix: None,
            desired_width: None,
            enabled: true,
        }
    }

    pub fn range(mut self, r: std::ops::RangeInclusive<f64>) -> Self {
        self.range = Some(r);
        self
    }

    pub fn step(mut self, s: f64) -> Self {
        self.step = Some(s);
        self
    }

    pub fn speed(mut self, s: f32) -> Self {
        self.speed = Some(s);
        self
    }

    pub fn suffix(mut self, s: &str) -> Self {
        self.suffix = Some(s.to_owned());
        self
    }

    pub fn desired_width(mut self, w: f32) -> Self {
        self.desired_width = Some(w);
        self
    }

    pub fn enabled(mut self, yes: bool) -> Self {
        self.enabled = yes;
        self
    }

    /// Render the field and return the inner [`egui::Response`].
    pub fn show(self, ui: &mut Ui) -> Response {
        let t = theme(ui);
        let s = crate::spatial(ui);
        let desired_width = self.desired_width.unwrap_or(80.0);
        let row_height = ui.text_style_height(&egui::TextStyle::Body) + s.space_3 * 2.0;

        let (outer_rect, outer_resp) =
            ui.allocate_exact_size(egui::vec2(desired_width, row_height), egui::Sense::hover());

        if !ui.is_rect_visible(outer_rect) {
            return outer_resp;
        }

        let inner_margin = s.space_3;
        let radius = CornerRadius::same(RADIUS_M as u8);

        // Reserve background + border shape slots BEFORE adding the DragValue
        // so they paint behind the number. egui appends shapes in call order;
        // painting the opaque fill afterwards would cover the value text.
        let painter = ui.painter_at(outer_rect);
        let bg_idx = painter.add(egui::Shape::Noop);
        let border_idx = painter.add(egui::Shape::Noop);

        let mut child_ui = ui.new_child(
            egui::UiBuilder::new().max_rect(outer_rect.shrink(inner_margin)),
        );

        let mut dv = DragValue::new(self.value).speed(self.speed.unwrap_or(0.5) as f64);

        if let Some(r) = self.range {
            dv = dv.range(r);
        }
        if let Some(s) = self.step {
            dv = dv.speed(s);
        }
        if let Some(ref suf) = self.suffix {
            dv = dv.suffix(suf.clone());
        }

        let inner_resp = child_ui.add_sized(
            egui::vec2(desired_width - inner_margin * 2.0, row_height - inner_margin * 2.0),
            dv,
        );

        let is_focused = inner_resp.has_focus();
        let is_hovered = outer_resp.hovered() || inner_resp.hovered();

        let slot = if !self.enabled {
            t.input.disabled
        } else if is_focused {
            t.input.focus
        } else if is_hovered {
            t.input.hover
        } else {
            t.input.normal
        };

        painter.set(
            bg_idx,
            egui::epaint::RectShape::filled(outer_rect, radius, slot.bg),
        );
        painter.set(
            border_idx,
            egui::epaint::RectShape::stroke(
                outer_rect,
                radius,
                Stroke::new(STROKE_WIDTH, slot.border),
                egui::StrokeKind::Inside,
            ),
        );

        inner_resp
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_field_builder_placeholder() {
        let mut s = String::new();
        let tf = TextField::new(&mut s).placeholder("hint");
        assert_eq!(tf.placeholder.as_deref(), Some("hint"));
    }

    #[test]
    fn text_field_builder_cleanable() {
        let mut s = String::new();
        let tf = TextField::new(&mut s).cleanable(true);
        assert!(tf.cleanable);
    }

    #[test]
    fn text_field_builder_password() {
        let mut s = String::new();
        let tf = TextField::new(&mut s).password(true);
        assert!(tf.password);
    }

    #[test]
    fn text_field_builder_prefix_suffix() {
        let mut s = String::new();
        let tf = TextField::new(&mut s).prefix("@").suffix(".com");
        assert_eq!(tf.prefix.as_deref(), Some("@"));
        assert_eq!(tf.suffix.as_deref(), Some(".com"));
    }

    #[test]
    fn number_field_builder_range() {
        let mut v = 0.0_f64;
        let nf = NumberField::new(&mut v).range(0.0..=100.0);
        let r = nf.range.unwrap();
        assert_eq!(*r.start(), 0.0);
        assert_eq!(*r.end(), 100.0);
    }

    #[test]
    fn number_field_builder_step() {
        let mut v = 0.0_f64;
        let nf = NumberField::new(&mut v).step(0.5);
        assert_eq!(nf.step, Some(0.5));
    }

    #[test]
    fn number_field_builder_suffix() {
        let mut v = 0.0_f64;
        let nf = NumberField::new(&mut v).suffix(" px");
        assert_eq!(nf.suffix.as_deref(), Some(" px"));
    }

    #[test]
    fn text_field_validate_predicate_stored() {
        let mut s = String::from("hello");
        let tf = TextField::new(&mut s).validate(|t| !t.is_empty());
        assert!(tf.validate.is_some());
    }
}
