//! C16 — `ColorPicker` widget.
//!
//! Composed from existing eparts primitives:
//! - [`Popover`] — themed floating panel + dismissal
//! - egui's built-in `color_picker_color32` — HSV square + channel sliders
//! - [`TextField`] — hex entry (`#RRGGBB` / `#RRGGBBAA`)
//! - Optional swatch grid (clickable preset colours)
//!
//! The value (`Color32`) is app-owned; the widget borrows it mutably each frame.
//!
//! ## Usage
//! ```ignore
//! let mut color = egui::Color32::from_rgb(100, 150, 200);
//! let resp = ColorPicker::new("my_picker", &mut color)
//!     .alpha(true)
//!     .swatches(&[egui::Color32::RED, egui::Color32::GREEN])
//!     .show(ui);
//! if resp.changed { /* color was modified */ }
//! ```

use egui::{Color32, CornerRadius, Id, Response, Stroke, Ui};

use crate::tokens::spatial::{RADIUS_M, STROKE_WIDTH};
use crate::tokens::theme::theme;
use crate::widget::popover::Popover;
use crate::widget::input::TextField;

// ── Hex helpers (pure, testable) ──────────────────────────────────────────────

/// Format a `Color32` as `#RRGGBB` (ignoring alpha, values unmultiplied).
pub fn color_to_hex_rgb(c: Color32) -> String {
    let [r, g, b, _] = c.to_srgba_unmultiplied();
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// Format a `Color32` as `#RRGGBBAA` (values unmultiplied).
pub fn color_to_hex_rgba(c: Color32) -> String {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
}

/// Parse a hex string (`#RRGGBB` or `#RRGGBBAA`).
/// Returns `None` if the string is malformed.
pub fn parse_hex_color(s: &str) -> Option<Color32> {
    let s = s.strip_prefix('#').unwrap_or(s);
    match s.len() {
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some(Color32::from_rgb(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            let a = u8::from_str_radix(&s[6..8], 16).ok()?;
            Some(Color32::from_rgba_unmultiplied(r, g, b, a))
        }
        _ => None,
    }
}

// ── Response ─────────────────────────────────────────────────────────────────

/// Response returned by [`ColorPicker::show`].
pub struct ColorPickerResponse {
    /// Whether the colour value changed this frame.
    pub changed: bool,
    /// The underlying trigger button response.
    pub trigger: Response,
}

// ── Widget ────────────────────────────────────────────────────────────────────

/// A themed colour picker: swatch trigger → Popover with HSV picker + hex + swatches.
///
/// The builder pattern follows eparts AGENTS.md §2.
pub struct ColorPicker<'a> {
    id: Id,
    color: &'a mut Color32,
    show_alpha: bool,
    swatches: Vec<Color32>,
}

impl<'a> ColorPicker<'a> {
    /// Create a new `ColorPicker` bound to `color`.
    pub fn new(id: impl Into<Id>, color: &'a mut Color32) -> Self {
        Self {
            id: id.into(),
            color,
            show_alpha: true,
            swatches: Vec::new(),
        }
    }

    /// Show or hide the alpha channel slider and hex alpha byte.
    /// Default: `true`.
    pub fn alpha(mut self, show_alpha: bool) -> Self {
        self.show_alpha = show_alpha;
        self
    }

    /// Set optional preset swatch colours rendered below the picker.
    pub fn swatches(mut self, swatches: &[Color32]) -> Self {
        self.swatches = swatches.to_vec();
        self
    }

    /// Render the trigger swatch button and (when open) the popover panel.
    pub fn show(self, ui: &mut Ui) -> ColorPickerResponse {
        let t = theme(ui);
        let Self { id, color, show_alpha, swatches } = self;

        // ── Trigger: small rounded swatch button ──────────────────────────
        let swatch_size = egui::vec2(32.0, 20.0);
        let (rect, trigger) = ui.allocate_exact_size(swatch_size, egui::Sense::click());

        if ui.is_rect_visible(rect) {
            let painter = ui.painter();
            let cr = CornerRadius::same(RADIUS_M as u8);
            // Checkerboard for transparent colours
            if color.a() < 255 {
                painter.rect_filled(
                    rect,
                    cr,
                    Color32::WHITE,
                );
                // Simple two-tone checkerboard approximation
                let half = egui::Rect::from_min_max(rect.min, egui::pos2(rect.center().x, rect.max.y));
                painter.rect_filled(half, cr, t.text.disabled);
            }
            painter.rect_filled(rect, cr, *color);
            let border_color = if trigger.hovered() {
                t.border.strong
            } else {
                t.border.default
            };
            painter.rect_stroke(rect, cr, Stroke::new(STROKE_WIDTH, border_color), egui::StrokeKind::Outside);
        }

        // ── Popover panel ─────────────────────────────────────────────────
        let hex_buf_key = id.with("__hex_buf");
        let popover = Popover::new(id).below();
        let mut changed = false;

        let alpha_mode = if show_alpha {
            egui::color_picker::Alpha::BlendOrAdditive
        } else {
            egui::color_picker::Alpha::Opaque
        };

        // Hex buffer stored in egui Memory (transient)
        let mut hex_buf: String = ui.ctx().data(|d| {
            d.get_temp::<String>(hex_buf_key).unwrap_or_else(|| {
                if show_alpha { color_to_hex_rgba(*color) } else { color_to_hex_rgb(*color) }
            })
        });

        popover.show(ui, &trigger, |ui| {
            // ── Built-in HSV picker (square + channel sliders) ────────────
            let before = *color;
            egui::color_picker::color_picker_color32(ui, color, alpha_mode);
            if *color != before {
                changed = true;
                hex_buf = if show_alpha { color_to_hex_rgba(*color) } else { color_to_hex_rgb(*color) };
            }

            ui.add_space(4.0);

            // ── Hex TextField ─────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label("Hex");
                let hex_resp = TextField::new(&mut hex_buf)
                    .placeholder(if show_alpha { "#RRGGBBAA" } else { "#RRGGBB" })
                    .validate(|s| {
                        let s = s.strip_prefix('#').unwrap_or(s);
                        matches!(s.len(), 6 | 8)
                            && s.chars().all(|c| c.is_ascii_hexdigit())
                    })
                    .show(ui);
                if hex_resp.response.lost_focus() || hex_resp.response.changed() {
                    if let Some(parsed) = parse_hex_color(&hex_buf) {
                        let new_color = if show_alpha {
                            parsed
                        } else {
                            Color32::from_rgb(parsed.r(), parsed.g(), parsed.b())
                        };
                        if new_color != *color {
                            *color = new_color;
                            changed = true;
                        }
                    }
                }
            });

            // ── Swatch palette ────────────────────────────────────────────
            if !swatches.is_empty() {
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for &swatch in &swatches {
                        let sw_size = egui::vec2(18.0, 18.0);
                        let (sr, sresp) = ui.allocate_exact_size(sw_size, egui::Sense::click());
                        if ui.is_rect_visible(sr) {
                            let cr = CornerRadius::same(3);
                            ui.painter().rect_filled(sr, cr, swatch);
                            let sc = if sresp.hovered() { t.border.strong } else { t.border.default };
                            ui.painter().rect_stroke(sr, cr, Stroke::new(STROKE_WIDTH, sc), egui::StrokeKind::Outside);
                        }
                        if sresp.clicked() {
                            let new_color = if show_alpha {
                                swatch
                            } else {
                                Color32::from_rgb(swatch.r(), swatch.g(), swatch.b())
                            };
                            *color = new_color;
                            hex_buf = if show_alpha { color_to_hex_rgba(new_color) } else { color_to_hex_rgb(new_color) };
                            changed = true;
                        }
                    }
                });
            }
        });

        // Persist hex buffer
        ui.ctx().data_mut(|d| d.insert_temp(hex_buf_key, hex_buf));

        ColorPickerResponse { changed, trigger }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Hex helpers ──────────────────────────────────────────────────────────

    #[test]
    fn hex_rgb_round_trip() {
        let c = Color32::from_rgb(0xDE, 0xAD, 0xBE);
        assert_eq!(parse_hex_color(&color_to_hex_rgb(c)), Some(Color32::from_rgb(0xDE, 0xAD, 0xBE)));
    }

    #[test]
    fn hex_rgba_round_trip() {
        let c = Color32::from_rgba_unmultiplied(0x11, 0x22, 0x33, 0x80);
        assert_eq!(parse_hex_color(&color_to_hex_rgba(c)), Some(c));
    }

    #[test]
    fn hex_format_rgb_no_hash_parse() {
        assert_eq!(parse_hex_color("AABBCC"), Some(Color32::from_rgb(0xAA, 0xBB, 0xCC)));
    }

    #[test]
    fn hex_format_lowercase_rejected() {
        // parse_hex_color accepts uppercase and lowercase (from_str_radix is case-insensitive)
        assert_eq!(parse_hex_color("#aabbcc"), Some(Color32::from_rgb(0xAA, 0xBB, 0xCC)));
    }

    #[test]
    fn hex_format_invalid_returns_none() {
        assert_eq!(parse_hex_color("#ZZZZZZ"), None);
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(parse_hex_color(""), None);
    }

    #[test]
    fn hex_rgb_format() {
        assert_eq!(color_to_hex_rgb(Color32::from_rgb(255, 0, 128)), "#FF0080");
    }

    #[test]
    fn hex_rgba_format() {
        assert_eq!(color_to_hex_rgba(Color32::from_rgba_unmultiplied(255, 0, 128, 64)), "#FF008040");
    }

    // ── Builder fields ───────────────────────────────────────────────────────

    #[test]
    fn builder_alpha_default_true() {
        let mut c = Color32::WHITE;
        let cp = ColorPicker::new("test", &mut c);
        assert!(cp.show_alpha);
    }

    #[test]
    fn builder_alpha_false() {
        let mut c = Color32::WHITE;
        let cp = ColorPicker::new("test", &mut c).alpha(false);
        assert!(!cp.show_alpha);
    }

    #[test]
    fn builder_swatches_set() {
        let mut c = Color32::WHITE;
        let palette = [Color32::RED, Color32::GREEN, Color32::BLUE];
        let cp = ColorPicker::new("test", &mut c).swatches(&palette);
        assert_eq!(cp.swatches, palette);
    }

    #[test]
    fn builder_swatches_default_empty() {
        let mut c = Color32::WHITE;
        let cp = ColorPicker::new("test", &mut c);
        assert!(cp.swatches.is_empty());
    }
}
