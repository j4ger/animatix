//! C5 — Themed `Slider` widget (single-thumb, optional logarithmic + step).
//!
//! A thin builder wrapper around `egui::Slider` that exposes eparts-style
//! ergonomics. The track/thumb colors are driven by `egui::Visuals`, which
//! the host GUI already syncs from `eparts::Theme`, so no extra paint calls
//! are required here.

use egui::{Response, Widget};

use crate::tokens::theme::theme;

/// A themed single-thumb slider.
///
/// ## Examples
/// ```ignore
/// # let value = &mut 0.5f64;
/// Slider::new(value, 0.0..=1.0)
///     .step(0.1)
///     .logarithmic(false)
///     .suffix("x")
///     .show_value(true);
/// ```
#[derive(Debug)]
pub struct Slider<'a> {
    value: &'a mut f64,
    range: std::ops::RangeInclusive<f64>,
    step: Option<f64>,
    logarithmic: bool,
    suffix: String,
    show_value: bool,
}

impl<'a> Slider<'a> {
    /// Create a new slider bound to `value` within `range`.
    pub fn new(value: &'a mut f64, range: std::ops::RangeInclusive<f64>) -> Self {
        Self {
            value,
            range,
            step: None,
            logarithmic: false,
            suffix: String::new(),
            show_value: false,
        }
    }

    /// Set the step increment. When `None` the slider is continuous.
    pub fn step(mut self, step: f64) -> Self {
        self.step = Some(step);
        self
    }

    /// Enable/disable logarithmic spacing.
    pub fn logarithmic(mut self, logarithmic: bool) -> Self {
        self.logarithmic = logarithmic;
        self
    }

    /// Set a suffix string rendered next to the value.
    pub fn suffix(mut self, suffix: impl Into<String>) -> Self {
        self.suffix = suffix.into();
        self
    }

    /// Show the current value inline.
    pub fn show_value(mut self, show_value: bool) -> Self {
        self.show_value = show_value;
        self
    }
}

impl Widget for Slider<'_> {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        // Read theme to keep the visual sync hot-path active.
        let _t = theme(ui);

        let mut slider = egui::Slider::new(self.value, self.range)
            .logarithmic(self.logarithmic)
            .suffix(self.suffix)
            .show_value(self.show_value);

        if let Some(step) = self.step {
            slider = slider.step_by(step);
        }

        slider.ui(ui)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_defaults() {
        let mut v = 0.0f64;
        let s = Slider::new(&mut v, 0.0..=1.0);
        assert!(s.step.is_none());
        assert!(!s.logarithmic);
        assert!(s.suffix.is_empty());
        assert!(!s.show_value);
    }

    #[test]
    fn builder_step() {
        let mut v = 0.0f64;
        let s = Slider::new(&mut v, 0.0..=1.0).step(0.25);
        assert_eq!(s.step, Some(0.25));
    }

    #[test]
    fn builder_logarithmic() {
        let mut v = 0.0f64;
        let s = Slider::new(&mut v, 0.0..=1.0).logarithmic(true);
        assert!(s.logarithmic);
    }

    #[test]
    fn builder_suffix() {
        let mut v = 0.0f64;
        let s = Slider::new(&mut v, 0.0..=1.0).suffix("px");
        assert_eq!(s.suffix, "px");
    }

    #[test]
    fn builder_show_value() {
        let mut v = 0.0f64;
        let s = Slider::new(&mut v, 0.0..=1.0).show_value(true);
        assert!(s.show_value);
    }

    #[test]
    fn chaining_works() {
        let mut v = 0.0f64;
        let s = Slider::new(&mut v, 0.0..=1.0)
            .step(0.1)
            .logarithmic(true)
            .suffix("x")
            .show_value(true);
        assert_eq!(s.step, Some(0.1));
        assert!(s.logarithmic);
        assert_eq!(s.suffix, "x");
        assert!(s.show_value);
    }
}
