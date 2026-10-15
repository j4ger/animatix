//! Typography roles and compatibility font-size constants.
//!
//! `TextRole` centralises the 8-level type scale and is the recommended
//! API for new code. Legacy `FONT_SIZE_*` constants are re-exported by the
//! compatibility facade for use during migration.

use egui::{FontFamily, FontId};

/// 11-level type scale with weight and tabular digit specialisations.
///
/// Each variant maps to a size and a font family (proportional, medium, semibold, or monospace).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextRole {
    Display,
    Heading,
    Subheading,
    Title,
    Body,
    BodyMedium,
    BodyS,
    Caption,
    Mono,
    MonoDigits,
    Micro,
}

impl TextRole {
    /// Returns the `egui::FontId` for this role.
    pub fn font_id(&self) -> FontId {
        match self {
            Self::Display => FontId::new(26.0, FontFamily::Name("GeistSemiBold".into())),
            Self::Heading => FontId::new(20.0, FontFamily::Name("GeistSemiBold".into())),
            Self::Subheading => FontId::new(16.0, FontFamily::Name("GeistMedium".into())),
            Self::Title => FontId::new(14.0, FontFamily::Name("GeistMedium".into())),
            Self::Body => FontId::proportional(13.0),
            Self::BodyMedium => FontId::new(13.0, FontFamily::Name("GeistMedium".into())),
            Self::BodyS => FontId::proportional(12.0),
            Self::Caption => FontId::new(11.0, FontFamily::Name("GeistMedium".into())),
            Self::Mono => FontId::monospace(12.0),
            Self::MonoDigits => FontId::new(12.0, FontFamily::Name("JetBrainsMonoMedium".into())),
            Self::Micro => FontId::proportional(10.0),
        }
    }

    /// Returns the font size in pixels.
    pub const fn size(&self) -> f32 {
        match self {
            Self::Display => 26.0,
            Self::Heading => 20.0,
            Self::Subheading => 16.0,
            Self::Title => 14.0,
            Self::Body => 13.0,
            Self::BodyMedium => 13.0,
            Self::BodyS => 12.0,
            Self::Caption => 11.0,
            Self::Mono => 12.0,
            Self::MonoDigits => 12.0,
            Self::Micro => 10.0,
        }
    }
}

// ── Legacy font-size constants (for compatibility facade) ──

pub const FONT_SIZE_XS: f32 = TextRole::Micro.size();
pub const FONT_SIZE_S: f32 = TextRole::BodyS.size();
pub const FONT_SIZE_M: f32 = TextRole::Body.size();
pub const FONT_SIZE_L: f32 = TextRole::Title.size();
pub const FONT_SIZE_XL: f32 = TextRole::Heading.size();

/// Register fallback font aliases for named font families on an [`egui::FontDefinitions`].
///
/// When running in a host app (like `animatix-gui`), custom TTFs (Geist, JetBrains Mono)
/// are installed for these names. In headless environments or tests where custom fonts
/// are not loaded, this ensures `FontFamily::Name` falls back gracefully to `Proportional`
/// or `Monospace` without panicking.
pub fn install_font_definitions(fonts: &mut egui::FontDefinitions) {
    let prop_fonts = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mono_fonts = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();

    fonts
        .families
        .entry(FontFamily::Name("GeistMedium".into()))
        .or_insert_with(|| prop_fonts.clone());
    fonts
        .families
        .entry(FontFamily::Name("GeistSemiBold".into()))
        .or_insert_with(|| prop_fonts);
    fonts
        .families
        .entry(FontFamily::Name("JetBrainsMonoMedium".into()))
        .or_insert_with(|| mono_fonts);
}

/// Convenience to ensure standard font families are registered on a context.
pub fn register_font_families(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    install_font_definitions(&mut fonts);
    ctx.set_fonts(fonts);
}
