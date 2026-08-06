//! Utility functions — temporary migration helpers.
//!
//! Kept as functions until all callers can use pre-computed semantic
//! constants or local animation helpers.

use egui::Color32;

/// Linearly interpolate between two colors.
pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
        (a.a() as f32 + (b.a() as f32 - a.a() as f32) * t) as u8,
    )
}

/// Multiply a color's alpha by a factor.
pub fn multiply_alpha(c: Color32, factor: f32) -> Color32 {
    let factor = factor.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (c.r() as f32 * factor) as u8,
        (c.g() as f32 * factor) as u8,
        (c.b() as f32 * factor) as u8,
        (c.a() as f32 * factor) as u8,
    )
}

/// WCAG AA contrast threshold for normal body text (4.5:1).
pub const WCAG_AA_TEXT: f64 = 4.5;

/// WCAG AA contrast threshold for large text and active UI components (3:1).
pub const WCAG_AA_UI: f64 = 3.0;

/// Compute WCAG 2.x relative luminance for an sRGB `Color32`.
///
/// Transparent colors are composited onto white so semi-transparent tokens can
/// still be compared without callers manually resolving the underlying surface.
pub fn relative_luminance(color: Color32) -> f64 {
    let channel = |channel: u8| {
        let srgb = f64::from(channel) / 255.0;
        if srgb <= 0.04045 {
            srgb / 12.92
        } else {
            ((srgb + 0.055) / 1.055).powf(2.4)
        }
    };

    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

/// Compute the WCAG 2.x contrast ratio between two colors.
///
/// Semi-transparent foreground colors are first composited onto `background`,
/// which matches how egui paints them over the surrounding surface.
pub fn contrast_ratio(foreground: Color32, background: Color32) -> f64 {
    let alpha = f64::from(foreground.a()) / 255.0;
    let blend = |channel: u8, bg_channel: u8| {
        (f64::from(channel) * alpha + f64::from(bg_channel) * (1.0 - alpha)).round() as u8
    };
    let fg = Color32::from_rgb(
        blend(foreground.r(), background.r()),
        blend(foreground.g(), background.g()),
        blend(foreground.b(), background.b()),
    );

    let lighter = relative_luminance(fg).max(relative_luminance(background));
    let darker = relative_luminance(fg).min(relative_luminance(background));
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_ratio_known_values() {
        assert!((contrast_ratio(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(Color32::WHITE, Color32::BLACK) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(Color32::BLACK, Color32::BLACK) - 1.0).abs() < 0.01);
    }

    #[test]
    fn contrast_ratio_composites_transparency() {
        assert!((contrast_ratio(Color32::TRANSPARENT, Color32::WHITE) - 1.0).abs() < 0.01);

        let half_black = Color32::from_rgba_unmultiplied(0, 0, 0, 128);
        let composited_gray = Color32::from_rgb(127, 127, 127);
        let blended = contrast_ratio(half_black, Color32::WHITE);
        let direct = contrast_ratio(composited_gray, Color32::WHITE);
        assert!((blended - direct).abs() < 0.01);
    }
}
