//! Keyboard shortcut badge widget.
//!
//! Renders a small, theme-styled rounded badge containing a keyboard shortcut
//! label (e.g. `"Ctrl+S"`, `"Cmd Shift+P"`).
//!
//! ## Usage
//! ```text
//! use eparts::widget::{Kbd, format_shortcut};
//! if ui.add(Kbd::new("Ctrl+S")).clicked() { … }
//! let text = format_shortcut(&shortcut, ui.ctx());
//! ```

use egui::{Context, CornerRadius, Response, Sense, Stroke, Widget};

use crate::tokens::spatial::RADIUS_S;
use crate::tokens::theme::theme;
use crate::tokens::typography::TextRole;

/// A badge-style widget that renders a keyboard shortcut label.
///
/// Colours and dimensions are entirely driven by the active [`crate::tokens::theme::Theme`]:
///   - **Fill**: `theme.overlay.badge_bg`
///   - **Text**: `theme.text.secondary`
///   - **Border**: `theme.border.default`
///   - **Corner radius**: `RADIUS_S`
///   - **Padding**: `SPACE_1` (inline and block)
///   - **Font**: [`TextRole::Caption`] — proportional 11 px.
#[derive(Clone, Debug, Default)]
pub struct Kbd {
    text: String,
}

impl Kbd {
    /// Create a new `Kbd` badge from any value convertible into a `String`.
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

impl Widget for Kbd {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let t = theme(ui);
        let s = crate::spatial(ui);
        let font_id = TextRole::Caption.font_id();

        let galley =
            ui.painter()
                .layout(self.text.clone(), font_id, t.text.secondary, ui.available_width());

        let pad = s.space_1;
        let size = galley.size() + egui::vec2(pad * 2.0, pad * 2.0);
        // Allocate from the layout cursor (`allocate_exact_size`) rather than
        // anchoring at `ui.min_rect().min`, which is the top-left of everything
        // already allocated in the Ui and would make the badge overlap prior
        // content in a vertical layout. The returned `Response` keeps hover sense.
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        let painter = ui.painter_at(rect);

        let corner = CornerRadius::same(RADIUS_S as u8);
        painter.rect_filled(rect, corner, t.overlay.badge_bg);
        // egui 0.34 `rect_stroke` takes a 4th `StrokeKind` argument.
        painter.rect_stroke(
            rect,
            corner,
            Stroke::new(1.0, t.border.default),
            egui::StrokeKind::Outside,
        );
        // egui 0.34 `galley` requires a fallback colour for glyphs missing from the font.
        painter.galley(rect.min + egui::vec2(pad, pad), galley, t.text.secondary);

        response
    }
}

/// Format a [`egui::KeyboardShortcut`] for display using the egui 0.34 API.
///
/// Delegates to [`egui::Context::format_shortcut`], which selects between
/// [`egui::ModifierNames::NAMES`] (e.g. `"Ctrl+Shift+S"`) and
/// [`egui::ModifierNames::SYMBOLS`] (e.g. `"⌃⇧F"`) depending on platform
/// and user preference.
///
/// ### egui 0.34 signatures used
/// ```ignore
/// // on egui::Context:
/// pub fn format_shortcut(&self, shortcut: &KeyboardShortcut) -> String
///
/// // on egui::KeyboardShortcut (called internally by Context::format_shortcut):
/// pub fn format(&self, names: &ModifierNames<'_>, is_mac: bool) -> String
/// ```
pub fn format_shortcut(shortcut: &egui::KeyboardShortcut, ctx: &Context) -> String {
    ctx.format_shortcut(shortcut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kbd_new_sets_text() {
        let badge = Kbd::new("Ctrl+S");
        // The text field is stored; we verify the struct carries it.
        // (Kbd has no public accessor by design — the text is consumed by `ui`.)
        // We verify by checking it does not panic and produces the expected string.
        let text = "Ctrl+S";
        assert_eq!(badge.text, text);
    }

    /// Regression: the badge used to be painted at `ui.min_rect().min`, which is
    /// the top-left of *all* previously allocated content. In a vertical layout
    /// that made it overlap the widget above it.
    #[test]
    fn kbd_allocates_below_prior_content() {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 120.0));
        let mut prior_bottom = 0.0;
        let mut kbd_rect = None;
        let _ = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| {
                let prior = ui.label("above");
                prior_bottom = prior.rect.bottom();
                kbd_rect = Some(ui.add(Kbd::new("Ctrl+S")).rect);
            },
        );
        let kbd_rect = kbd_rect.expect("kbd response");
        assert!(
            kbd_rect.top() >= prior_bottom - 0.5,
            "Kbd must be allocated below the prior widget (kbd.top={}, prior.bottom={})",
            kbd_rect.top(),
            prior_bottom
        );
    }

    #[test]
    fn format_shortcut_uses_names_on_non_mac() {
        // With NAMES, a COMMAND+S shortcut produces "Ctrl+S" on non-macOS.
        let shortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
        let ctx = egui::Context::default();
        let formatted = format_shortcut(&shortcut, &ctx);
        // egui 0.34 Context::format_shortcut uses NAMES on non-macOS by default.
        assert!(formatted.contains("Ctrl"));
        assert!(formatted.contains("S"));
    }
}
