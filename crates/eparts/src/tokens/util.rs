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
