//! G7 — Themed `Label` widget.
//!
//! A simple themed text label honoring a [`TextRole`] with an optional required
//! marker (red asterisk). Implements [`egui::Widget`] so it can be used anywhere
//! egui widgets are accepted.
//!
//! Used by `Form` fields for consistent label styling.

use egui::{Color32, Response, Widget};

use crate::tokens::theme::theme;
use crate::tokens::typography::TextRole;

/// A themed label widget honoring a `TextRole` and an optional required marker.
///
/// ## Examples
/// ```
/// # use eparts::widget::Label;
/// # use eparts::tokens::typography::TextRole;
/// # use egui::Color32;
///
/// // Default body label
/// Label::new("Hello");
///
/// // Required field label (red asterisk)
/// Label::new("Password").role(TextRole::BodyS).required(true);
///
/// // Custom color override
/// Label::new("Warning").role(TextRole::Caption).color(Color32::YELLOW);
/// ```
#[derive(Clone, Debug)]
pub struct Label {
    text: egui::WidgetText,
    role: TextRole,
    required: bool,
    color: Option<Color32>,
}

impl Label {
    /// Create a new label with the given text.
    pub fn new(text: impl Into<egui::WidgetText>) -> Self {
        Self {
            text: text.into(),
            role: TextRole::Body,
            required: false,
            color: None,
        }
    }

    /// Set the [`TextRole`] (font family + size) for this label.
    pub fn role(mut self, role: TextRole) -> Self {
        self.role = role;
        self
    }

    /// Mark this label as a required field. Appends a red asterisk using
    /// `Theme::status.error`.
    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    /// Override the text color. When `None`, falls back to `Theme::text.primary`.
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }
}

impl Widget for Label {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let t = theme(ui);
        let base_color = self.color.unwrap_or(t.text.primary);
        let font_id = self.role.font_id();
        let base_text = self.text.text();

        // Base text galley
        let base_galley =
            ui.painter().layout_no_wrap(base_text.to_string(), font_id.clone(), base_color);

        // Optional required asterisk
        let asterisk_galley = if self.required {
            Some(ui.painter().layout_no_wrap(" *".to_string(), font_id, t.status.error))
        } else {
            None
        };

        let base_text_width = base_galley.size().x;
        let asterisk_width = asterisk_galley.as_ref().map_or(0.0, |g| g.size().x);
        let total_width = base_text_width + asterisk_width;
        let total_height =
            base_galley.size().y.max(asterisk_galley.as_ref().map_or(0.0, |g| g.size().y));

        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(total_width, total_height), egui::Sense::hover());

        // Paint base text
        ui.painter().galley(rect.min, base_galley.clone(), base_color);

        // Paint asterisk immediately after base text
        if let Some(asterisk) = asterisk_galley {
            ui.painter().galley(
                egui::pos2(rect.min.x + base_text_width, rect.min.y),
                asterisk,
                t.status.error,
            );
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use egui::Color32;

    use super::*;

    #[test]
    fn builder_defaults() {
        let l = Label::new("hello");
        assert_eq!(l.role, TextRole::Body);
        assert!(!l.required);
        assert!(l.color.is_none());
    }

    #[test]
    fn builder_role() {
        let l = Label::new("hello").role(TextRole::Title);
        assert_eq!(l.role, TextRole::Title);
    }

    #[test]
    fn builder_required() {
        let l = Label::new("hello").required(true);
        assert!(l.required);
    }

    #[test]
    fn builder_color() {
        let l = Label::new("hello").color(Color32::RED);
        assert_eq!(l.color, Some(Color32::RED));
    }

    #[test]
    fn required_false_has_no_asterisk() {
        let l = Label::new("hello").required(false);
        assert!(!l.required);
    }

    #[test]
    fn chaining_works() {
        let l = Label::new("test").role(TextRole::Caption).required(true).color(Color32::GREEN);
        assert_eq!(l.role, TextRole::Caption);
        assert!(l.required);
        assert_eq!(l.color, Some(Color32::GREEN));
    }
}
