//! C1 — `Form` + `Field` layout widgets.
//!
//! `Form` is a theme-aware layout container that renders labeled input rows in a
//! vertical stack. Each row consists of a fixed-width label column (right-aligned)
//! and a flexible input column.
//!
//! No state is retained between frames; `Form` holds only layout configuration
//! and renders immediately via the `Field` builder yielded by `show`.

use egui::{Align, Layout, Vec2};

use crate::tokens::spatial::SPACE_M;
use crate::tokens::theme::theme;
use crate::tokens::typography::TextRole;
use crate::widget::label::Label;

/// A form layout container for labeled input rows.
///
/// Uses manual columns (fixed-width label + flexible input) per row. The label
/// column width is set by [`Self::label_width`].
///
/// ## Examples
/// ```ignore
/// # use eparts::widget::{Form, Field};
/// # use eparts::tokens::typography::TextRole;
/// Form::new("settings_form")
///     .label_width(120.0)
///     .show(ui, |f: &mut Field| {
///         f.field("Name", |ui| { /* input widget */ });
///         f.required_field("Password", |ui| { /* input widget */ });
///         f.field_opt("Email", true, |ui| { /* input widget */ });
///     });
/// ```
#[derive(Clone, Debug)]
pub struct Form {
    _id: egui::Id,
    num_columns: usize,
    label_width: f32,
}

impl Form {
    /// Create a new form with the given source id.
    pub fn new(id: impl Into<egui::Id>) -> Self {
        Self {
            _id: id.into(),
            num_columns: 2,
            label_width: 100.0,
        }
    }

    /// Set the number of columns (label + input = 2 by default).
    ///
    /// Currently only the label column width is enforced; extra columns are
    /// reserved for future `col_span` support.
    pub fn num_columns(mut self, n: usize) -> Self {
        self.num_columns = n;
        self
    }

    /// Set the fixed width of the label column in pixels.
    pub fn label_width(mut self, w: f32) -> Self {
        self.label_width = w;
        self
    }

    /// Show the form, yielding a [`Field`] builder for each row.
    pub fn show(self, ui: &mut egui::Ui, f: impl FnOnce(&mut Field)) {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = SPACE_M;
            let mut field = Field {
                ui,
                label_width: self.label_width,
            };
            f(&mut field);
        });
    }
}

/// Row builder for a [`Form`]. Each method renders one labeled input row.
///
/// ## Examples
/// ```ignore
/// # use eparts::widget::{Form, Field};
/// # let ui = &mut egui::Ui::dummy();
/// Form::new("f").show(ui, |f: &mut Field| {
///     f.field("Name", |ui| {});
///     f.required_field("Password", |ui| {});
///     f.field_opt("Email", false, |ui| {});
/// });
/// ```
pub struct Field<'a> {
    ui: &'a mut egui::Ui,
    label_width: f32,
}

impl<'a> Field<'a> {
    /// Render a standard labeled input row.
    pub fn field(&mut self, label: impl Into<egui::WidgetText>, add_contents: impl FnOnce(&mut egui::Ui)) {
        self.render_row(label, false, add_contents);
    }

    /// Render a required labeled input row. The label gains a red asterisk.
    pub fn required_field(&mut self, label: impl Into<egui::WidgetText>, add_contents: impl FnOnce(&mut egui::Ui)) {
        self.render_row(label, true, add_contents);
    }

    /// Render a labeled input row only when `visible` is `true`.
    pub fn field_opt(&mut self, label: impl Into<egui::WidgetText>, visible: bool, add_contents: impl FnOnce(&mut egui::Ui)) {
        if visible {
            self.render_row(label, false, add_contents);
        }
    }

    fn render_row(
        &mut self,
        label: impl Into<egui::WidgetText>,
        required: bool,
        add_contents: impl FnOnce(&mut egui::Ui),
    ) {
        let t = theme(self.ui);
        self.ui.horizontal(|ui| {
            // Fixed-width label column, right-aligned, using theme secondary text
            ui.allocate_ui_with_layout(
                Vec2::new(self.label_width, 0.0),
                Layout::right_to_left(Align::Center),
                |ui| {
                    ui.add(
                        Label::new(label)
                            .role(TextRole::BodyS)
                            .color(t.text.secondary)
                            .required(required),
                    );
                }
            );
            ui.add_space(SPACE_M);
            add_contents(ui);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_defaults() {
        let f = Form::new("test");
        assert_eq!(f.num_columns, 2);
        assert_eq!(f.label_width, 100.0);
    }

    #[test]
    fn builder_label_width() {
        let f = Form::new("test").label_width(150.0);
        assert_eq!(f.label_width, 150.0);
    }

    #[test]
    fn builder_num_columns() {
        let f = Form::new("test").num_columns(3);
        assert_eq!(f.num_columns, 3);
    }
}
