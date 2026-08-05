//! Small themed display/feedback widgets (roadmap G2–G6).
//!
//! All widgets are theme-driven, use the builder pattern, and implement
//! `egui::Widget` where sensible. Icons come from `egui_phosphor`.

use egui::{Align2, Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, Widget};

use crate::theme;
use crate::tokens::spatial::{RADIUS_M, RADIUS_S, STROKE_WIDTH, spatial};
use crate::tokens::typography::TextRole;

// ── Skeleton (G2) ──────────────────────────────────────────────────

/// A shimmer placeholder block used during loading/recompiles.
///
/// Paints a rounded rect filled with `surface.widget` and a subtle pulsing
/// highlight driven by `ui.input(|i| i.time)`. The widget requests a repaint
/// every frame while visible so the animation stays smooth.
///
/// ## Examples
/// ```ignore
/// ui.add(Skeleton::new(vec2(120.0, 16.0)));
/// ui.add(Skeleton::new(vec2(120.0, 16.0)).width(200.0));
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Skeleton {
    size: Vec2,
}

impl Default for Skeleton {
    fn default() -> Self {
        Self {
            size: Vec2::new(100.0, 16.0),
        }
    }
}

impl Skeleton {
    /// Create a skeleton with the given size.
    pub fn new(size: Vec2) -> Self {
        Self { size }
    }

    /// Override the width (keeping the current height).
    pub fn width(mut self, width: f32) -> Self {
        self.size.x = width;
        self
    }

    /// Override the height (keeping the current width).
    pub fn height(mut self, height: f32) -> Self {
        self.size.y = height;
        self
    }
}

impl Widget for Skeleton {
    fn ui(self, ui: &mut Ui) -> Response {
        let size = self.size.max(Vec2::splat(4.0));
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        let t = theme(ui);
        let radius = RADIUS_S as u8;

        // Base fill.
        ui.painter().rect_filled(rect, radius, t.surface.widget);

        // Subtle pulsing highlight using the accent color at low alpha.
        let time = ui.input(|i| i.time) as f32;
        let alpha = 0.3 + 0.7 * ((time * 2.5).sin() * 0.5 + 0.5);
        let shimmer = t.accent.primary.linear_multiply(alpha * 0.15);
        ui.painter().rect_filled(rect, radius, shimmer);

        // Keep the shimmer alive.
        ui.ctx().request_repaint();
        response
    }
}

// ── ProgressBar (G3) ───────────────────────────────────────────────

/// A determinate progress bar.
///
/// Paints a rounded track (`surface.widget`) with an `accent.primary` fill to
/// `fraction`. An optional label is centered; when `show_percentage` is true
/// the label is a percentage string.
///
/// ## Examples
/// ```ignore
/// ui.add(ProgressBar::new(0.42).show_percentage(true));
/// ui.add(ProgressBar::new(0.0).text("Loading…"));
/// ```
#[derive(Clone, Debug, Default)]
pub struct ProgressBar {
    fraction: f32,
    text: Option<String>,
    show_percentage: bool,
}

impl ProgressBar {
    /// Create a new progress bar. `fraction` is clamped to `0.0..=1.0`.
    pub fn new(fraction: f32) -> Self {
        Self {
            fraction: fraction.clamp(0.0, 1.0),
            ..Self::default()
        }
    }

    /// Set an optional text label displayed in the center.
    pub fn text(mut self, text: Option<String>) -> Self {
        self.text = text;
        self
    }

    /// Replace the label with a percentage string.
    pub fn show_percentage(mut self, yes: bool) -> Self {
        self.show_percentage = yes;
        self
    }
}

impl Widget for ProgressBar {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = theme(ui);
        let s = spatial(ui);
        let width = ui.available_width();
        let height = s.component.progress_bar_height;
        let size = Vec2::new(width, height);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        let radius = RADIUS_M as u8;

        // Track.
        ui.painter().rect_filled(rect, radius, t.surface.widget);

        // Fill.
        let fill_width = (rect.width() * self.fraction).clamp(0.0, rect.width());
        let fill_rect =
            Rect::from_min_size(rect.min, Vec2::new(fill_width, rect.height())).intersect(rect);
        ui.painter().rect_filled(fill_rect, radius, t.accent.primary);

        // Label.
        let label = if self.show_percentage {
            Some(format!("{:.0}%", self.fraction * 100.0))
        } else {
            self.text
        };
        if let Some(text) = label {
            let text_color = if self.fraction > 0.5 {
                t.text.on_accent
            } else {
                t.text.primary
            };
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                text,
                TextRole::BodyS.font_id(),
                text_color,
            );
        }

        response
    }
}

// ── Badge (G4) ─────────────────────────────────────────────────────

/// A tiny status/count badge.
///
/// Paints a pill-shaped rect with `overlay.badge_bg` background and the
/// supplied (or default accent) text color.
///
/// ## Examples
/// ```ignore
/// ui.add(Badge::new("3"));
/// ui.add(Badge::new("new").color(Color32::from_rgb(255, 100, 0)));
/// ```
#[derive(Clone, Debug, Default)]
pub struct Badge {
    text: String,
    color: Option<Color32>,
}

impl Badge {
    /// Create a new badge.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// Override the badge text/icon color. Defaults to `accent.primary`.
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }
}

impl Widget for Badge {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = theme(ui);
        let s = spatial(ui);
        let font = TextRole::Caption.font_id();
        let galley = ui.painter().layout_no_wrap(self.text.clone(), font, t.text.primary);
        let pad = s.space_2;
        let size = Vec2::new(galley.size().x + pad * 2.0, galley.size().y + pad);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        let radius = rect.height() / 2.0;

        ui.painter().rect_filled(rect, radius as u8, t.overlay.badge_bg);

        let text_color = self.color.unwrap_or(t.accent.primary);
        ui.painter().galley(rect.center() - galley.size() * 0.5, galley, text_color);

        response
    }
}

// ── Tag (G5) ───────────────────────────────────────────────────────

/// A labeled chip, optionally removable.
///
/// Paints a rounded chip with `surface.widget` background, `border.default`
/// stroke, and small `BodyS` text. When `removable` is true an trailing `✕`
/// icon is shown; clicking the chip reports `response.clicked()`.
///
/// ## Examples
/// ```ignore
/// ui.add(Tag::new("filter"));
/// ui.add(Tag::new("filter").removable(true));
/// ```
#[derive(Clone, Debug, Default)]
pub struct Tag {
    text: String,
    removable: bool,
    color: Option<Color32>,
}

impl Tag {
    /// Create a new tag.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// Show a remove icon. Returns `Response::clicked()` when the icon area is clicked.
    pub fn removable(mut self, yes: bool) -> Self {
        self.removable = yes;
        self
    }

    /// Override the chip text and border color. Defaults to `text.primary` / `border.default`.
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }
}

impl Widget for Tag {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = theme(ui);
        let s = spatial(ui);
        let font = TextRole::BodyS.font_id();
        let suffix = if self.removable { " ✕" } else { "" };
        let text = format!("{}{}", self.text, suffix);
        let galley = ui.painter().layout_no_wrap(text, font, t.text.primary);
        let pad = s.space_2;
        let size = Vec2::new(galley.size().x + pad * 2.0, galley.size().y + pad);
        let sense = if self.removable {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(size, sense);
        let radius = RADIUS_S as u8;
        let text_color = self.color.unwrap_or(t.text.primary);
        let border_color = self.color.unwrap_or(t.border.default);

        ui.painter().rect_filled(rect, radius, t.surface.widget);
        ui.painter().rect_stroke(
            rect,
            radius,
            Stroke::new(STROKE_WIDTH, border_color),
            StrokeKind::Inside,
        );
        ui.painter().galley(rect.center() - galley.size() * 0.5, galley, text_color);

        response
    }
}

// ── Alert (G6) ─────────────────────────────────────────────────────

/// Inline status banner level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AlertLevel {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

/// An inline status banner.
///
/// Paints a rounded rect with a faint tinted background (from `status.*_faint`
/// or `accent.faint` for info), a left accent bar, an icon, and an optional
/// title line.
///
/// ## Examples
/// ```ignore
/// ui.add(Alert::new("Saved.", AlertLevel::Success));
/// ui.add(Alert::new("Disk full", AlertLevel::Error).title(Some("Error")));
/// ```
#[derive(Clone, Debug, Default)]
pub struct Alert {
    text: String,
    level: AlertLevel,
    title: Option<String>,
}

impl Alert {
    /// Create a new alert.
    pub fn new(text: impl Into<String>, level: AlertLevel) -> Self {
        Self {
            text: text.into(),
            level,
            ..Self::default()
        }
    }

    /// Set an optional title rendered above the body text.
    pub fn title(mut self, title: Option<String>) -> Self {
        self.title = title;
        self
    }
}

impl Widget for Alert {
    fn ui(self, ui: &mut Ui) -> Response {
        let t = theme(ui);
        let s = spatial(ui);
        let (icon, color, bg) = match self.level {
            AlertLevel::Info => (egui_phosphor::regular::INFO, t.status.info, t.accent.faint),
            AlertLevel::Success => {
                (egui_phosphor::regular::CHECK, t.status.success, t.status.success_faint)
            },
            AlertLevel::Warning => {
                (egui_phosphor::regular::WARNING, t.status.warning, t.status.warning_subtle)
            },
            AlertLevel::Error => {
                (egui_phosphor::regular::X_CIRCLE, t.status.error, t.status.error_faint)
            },
        };

        let icon_font = TextRole::BodyS.font_id();
        let title_font = TextRole::Body.font_id();
        let body_font = TextRole::BodyS.font_id();

        let icon_galley = ui.painter().layout_no_wrap(icon.to_string(), icon_font.clone(), color);
        let title_galley = self
            .title
            .as_ref()
            .map(|s| ui.painter().layout_no_wrap(s.clone(), title_font.clone(), t.text.primary));
        let body_galley =
            ui.painter()
                .layout_no_wrap(self.text.clone(), body_font.clone(), t.text.primary);

        let spacing = s.space_2;
        let pad = s.space_3;
        let bar_w = 4.0;
        let icon_w = icon_galley.size().x;
        let mut width = pad * 2.0 + bar_w + spacing + icon_w + body_galley.size().x;
        let mut height = pad * 2.0 + icon_galley.size().y;

        if let Some(ref tg) = title_galley {
            width = pad * 2.0 + bar_w + spacing + icon_w + tg.size().x;
            height = tg.size().y + spacing + body_galley.size().y;
        }

        let size = Vec2::new(ui.available_width().max(width), height);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        let radius = RADIUS_M as u8;

        // Background.
        ui.painter().rect_filled(rect, radius, bg);

        // Left accent bar.
        let bar_rect = Rect::from_min_size(rect.min, Vec2::new(bar_w, rect.height()));
        ui.painter().rect_filled(bar_rect, radius, color);

        // Icon.
        let icon_x = rect.min.x + pad + icon_w / 2.0;
        let icon_y = if let Some(tg) = title_galley.as_ref() {
            rect.min.y + pad + tg.size().y / 2.0
        } else {
            rect.center().y
        };
        ui.painter()
            .text(Pos2::new(icon_x, icon_y), Align2::CENTER_CENTER, icon, icon_font, color);

        // Text.
        let text_x = rect.min.x + pad + bar_w + spacing + icon_w;
        if let Some(ref tg) = title_galley {
            let title_y = rect.min.y + pad;
            ui.painter().galley(Pos2::new(text_x, title_y), tg.clone(), t.text.primary);
            let body_y = title_y + tg.size().y + spacing;
            ui.painter().galley(Pos2::new(text_x, body_y), body_galley, t.text.primary);
        } else {
            let text_y = rect.center().y - body_galley.size().y / 2.0;
            ui.painter().galley(Pos2::new(text_x, text_y), body_galley, t.text.primary);
        }

        response
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Skeleton
    #[test]
    fn skeleton_defaults() {
        let s = Skeleton::default();
        assert_eq!(s.size, Vec2::new(100.0, 16.0));
    }

    #[test]
    fn skeleton_new_and_size() {
        let s = Skeleton::new(Vec2::new(80.0, 24.0));
        assert_eq!(s.size, Vec2::new(80.0, 24.0));
    }

    #[test]
    fn skeleton_width_height() {
        let s = Skeleton::new(Vec2::new(40.0, 10.0)).width(120.0).height(32.0);
        assert_eq!(s.size, Vec2::new(120.0, 32.0));
    }

    // ProgressBar
    #[test]
    fn progress_bar_defaults() {
        let p = ProgressBar::new(0.5);
        assert_eq!(p.fraction, 0.5);
        assert!(p.text.is_none());
        assert!(!p.show_percentage);
    }

    #[test]
    fn progress_bar_clamp_negative() {
        let p = ProgressBar::new(-0.5);
        assert_eq!(p.fraction, 0.0);
    }

    #[test]
    fn progress_bar_clamp_over_one() {
        let p = ProgressBar::new(1.5);
        assert_eq!(p.fraction, 1.0);
    }

    #[test]
    fn progress_bar_text() {
        let p = ProgressBar::new(0.3).text(Some("Loading".into()));
        assert_eq!(p.text, Some("Loading".into()));
    }

    #[test]
    fn progress_bar_show_percentage() {
        let p = ProgressBar::new(0.7).show_percentage(true);
        assert!(p.show_percentage);
    }

    // Badge
    #[test]
    fn badge_defaults() {
        let b = Badge::new("42");
        assert_eq!(b.text, "42");
        assert!(b.color.is_none());
    }

    #[test]
    fn badge_color() {
        let c = Color32::from_rgb(255, 80, 0);
        let b = Badge::new("new").color(c);
        assert_eq!(b.color, Some(c));
    }

    // Tag
    #[test]
    fn tag_defaults() {
        let t = Tag::new("rust");
        assert_eq!(t.text, "rust");
        assert!(!t.removable);
        assert!(t.color.is_none());
    }

    #[test]
    fn tag_removable_and_color() {
        let t = Tag::new("rust").removable(true).color(Color32::from_rgb(100, 200, 100));
        assert!(t.removable);
        assert_eq!(t.color, Some(Color32::from_rgb(100, 200, 100)));
    }

    // Alert
    #[test]
    fn alert_defaults() {
        let a = Alert::new("ok", AlertLevel::Info);
        assert_eq!(a.text, "ok");
        assert_eq!(a.level, AlertLevel::Info);
        assert!(a.title.is_none());
    }

    #[test]
    fn alert_title() {
        let a = Alert::new("boom", AlertLevel::Error).title(Some("Error".into()));
        assert_eq!(a.title, Some("Error".into()));
        assert_eq!(a.level, AlertLevel::Error);
    }

    #[test]
    fn alert_levels() {
        let levels = [
            AlertLevel::Info,
            AlertLevel::Success,
            AlertLevel::Warning,
            AlertLevel::Error,
        ];
        for lvl in levels {
            let a = Alert::new("x", lvl);
            assert_eq!(a.level, lvl);
        }
    }
}
