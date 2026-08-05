use egui::{Color32, Pos2, Shape, Ui, Widget};

use crate::theme;
use crate::widget::traits::{Sizable, Size};

// ── Spinner ────────────────────────────────────────────────────────

/// A small indeterminate spinner for loading states.
///
/// Draws a ~300° rotating arc driven by `ui.input(|i| i.time)`. Size and color
/// are theme-driven by default.
///
/// ## Examples
/// ```ignore
/// ui.add(Spinner::new());
/// ui.add(Spinner::new().set_size(24.0).with_color(Color32::GRAY));
/// ui.add(Spinner::new().with_size(Size::Lg));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Spinner {
    pixel_size: f32,
    color: Option<Color32>,
}

impl Spinner {
    /// Create a spinner with the default size (16 px).
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a custom pixel diameter directly.
    pub fn set_size(mut self, size: f32) -> Self {
        self.pixel_size = size;
        self
    }

    /// Override the stroke color (defaults to `theme(ui).accent.primary`).
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }
}

impl Default for Spinner {
    fn default() -> Self {
        Self {
            pixel_size: 16.0,
            color: None,
        }
    }
}

impl Sizable for Spinner {
    fn with_size(mut self, size: Size) -> Self {
        self.pixel_size = match size {
            Size::Xs => 12.0,
            Size::Sm => 14.0,
            Size::Md => 16.0,
            Size::Lg => 20.0,
            Size::Custom(v) => v,
        };
        self
    }
}

impl Widget for Spinner {
    fn ui(self, ui: &mut Ui) -> egui::Response {
        let size = self.pixel_size.max(4.0);
        let color = self.color.unwrap_or_else(|| theme(ui).accent.primary);

        // Drive continuous rotation from egui's global clock (f64 → f32).
        let time = ui.input(|i| i.time) as f32;
        let rotation = time * std::f32::consts::TAU * 0.75;

        // ~300° arc, drawn as a polyline of points.
        let arc_span = std::f32::consts::FRAC_PI_3 * 5.0;
        let start = rotation;
        let end = rotation + arc_span;
        let segments = 32;

        let center = ui.max_rect().center();
        let radius = size / 2.0;
        let mut points: Vec<Pos2> = Vec::with_capacity(segments + 1);
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let angle = start + t * (end - start);
            points.push(center + egui::Vec2::new(angle.cos(), angle.sin()) * radius);
        }

        let stroke_width = (size * 0.12).clamp(1.0, 4.0);
        ui.painter().add(Shape::line(points, egui::Stroke::new(stroke_width, color)));

        // Keep the animation running.
        ui.ctx().request_repaint();

        ui.allocate_rect(
            egui::Rect::from_center_size(center, egui::Vec2::splat(size)),
            egui::Sense::hover(),
        )
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_defaults() {
        let s = Spinner::new();
        assert!((s.pixel_size - 16.0).abs() < f32::EPSILON);
        assert!(s.color.is_none());
    }

    #[test]
    fn builder_set_size() {
        let s = Spinner::new().set_size(24.0);
        assert!((s.pixel_size - 24.0).abs() < f32::EPSILON);
    }

    #[test]
    fn builder_with_color() {
        let c = Color32::from_rgb(255, 128, 0);
        let s = Spinner::new().with_color(c);
        assert_eq!(s.color, Some(c));
    }

    #[test]
    fn builder_chaining() {
        let c = Color32::RED;
        let s = Spinner::new().set_size(32.0).with_color(c);
        assert!((s.pixel_size - 32.0).abs() < f32::EPSILON);
        assert_eq!(s.color, Some(c));
    }

    #[test]
    fn sizable_mapping() {
        let s = Spinner::new().with_size(Size::Xs);
        assert!((s.pixel_size - 12.0).abs() < f32::EPSILON);

        let s = s.with_size(Size::Sm);
        assert!((s.pixel_size - 14.0).abs() < f32::EPSILON);

        let s = s.with_size(Size::Md);
        assert!((s.pixel_size - 16.0).abs() < f32::EPSILON);

        let s = s.with_size(Size::Lg);
        assert!((s.pixel_size - 20.0).abs() < f32::EPSILON);

        let s = s.with_size(Size::Custom(42.0));
        assert!((s.pixel_size - 42.0).abs() < f32::EPSILON);
    }

    #[test]
    fn sizable_shorthand_xs_sm_lg() {
        let s = Spinner::new().xs();
        assert!((s.pixel_size - 12.0).abs() < f32::EPSILON);

        let s = Spinner::new().sm();
        assert!((s.pixel_size - 14.0).abs() < f32::EPSILON);

        let s = Spinner::new().lg();
        assert!((s.pixel_size - 20.0).abs() < f32::EPSILON);
    }
}
