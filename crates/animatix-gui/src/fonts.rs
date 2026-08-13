//! egui font registration shared by the main app and the review console.
//!
//! egui's default fonts do not include non-Latin glyphs, so Chinese text entered
//! in comment fields renders as replacement boxes. We discover system fallback
//! fonts by glyph coverage once at startup and append them to egui's font lists.

use std::sync::{Arc, OnceLock};

use animatix::renderer::text::FontContext;
use egui::{FontData, FontDefinitions, FontFamily};

/// Representative glyphs used to discover broad system fallback coverage.
const FALLBACK_PROBES: &[char] = &[
    '中', // Han
    '文', // Han
    'あ', // Hiragana
    '한', // Hangul
    'А',  // Cyrillic
    'Α',  // Greek
    'ا',  // Arabic
    'א',  // Hebrew
    'क',  // Devanagari
    'ก',  // Thai
];

/// Install egui's built-in fonts, Phosphor icons, and system fallback fonts.
pub(crate) fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    add_system_fallbacks(&mut fonts, system_fallbacks());
    ctx.set_fonts(fonts);
}

/// Shared system font context used by font discovery and the font-family picker.
pub(crate) fn system_font_context() -> &'static FontContext {
    static FONT_CONTEXT: OnceLock<FontContext> = OnceLock::new();
    FONT_CONTEXT.get_or_init(FontContext::new)
}

fn system_fallbacks() -> &'static [(Vec<u8>, u32)] {
    static FALLBACKS: OnceLock<Vec<(Vec<u8>, u32)>> = OnceLock::new();
    FALLBACKS.get_or_init(|| {
        let fonts = system_font_context().font_for_glyphs(FALLBACK_PROBES);
        if fonts.is_empty() {
            tracing::debug!("No system fallback fonts found for non-Latin glyphs");
        }
        fonts
    })
}

fn add_system_fallbacks(fonts: &mut FontDefinitions, fallbacks: &[(Vec<u8>, u32)]) {
    for (index, (bytes, face_index)) in fallbacks.iter().enumerate() {
        let name = format!("Animatix System Fallback {index}");
        let mut font_data = FontData::from_owned(bytes.clone());
        font_data.index = *face_index;
        fonts.font_data.insert(name.clone(), Arc::new(font_data));

        fonts.families.entry(FontFamily::Proportional).or_default().push(name.clone());
        fonts.families.entry(FontFamily::Monospace).or_default().push(name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_fallbacks_registered_for_proportional_and_monospace() {
        let mut fonts = FontDefinitions::default();
        add_system_fallbacks(&mut fonts, &[(vec![0; 16], 0)]);

        assert_eq!(fonts.font_data.len(), FontDefinitions::default().font_data.len() + 1);
        assert!(
            fonts.families[&FontFamily::Proportional]
                .contains(&"Animatix System Fallback 0".to_owned())
        );
        assert!(
            fonts.families[&FontFamily::Monospace]
                .contains(&"Animatix System Fallback 0".to_owned())
        );
    }

    #[test]
    fn missing_system_fallbacks_leave_defaults_untouched() {
        let mut fonts = FontDefinitions::default();
        add_system_fallbacks(&mut fonts, &[]);

        assert_eq!(fonts, FontDefinitions::default());
    }

    #[test]
    fn installed_fallbacks_cover_chinese_when_available() {
        if system_fallbacks().is_empty() {
            return; // CI/machines without a non-Latin font should not fail this test.
        }

        let ctx = egui::Context::default();
        install_fonts(&ctx);
        let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
        assert!(ctx.fonts_mut(|fonts| fonts.has_glyphs(&egui::FontId::proportional(14.0), "中文")));
        assert!(ctx.fonts_mut(|fonts| fonts.has_glyphs(&egui::FontId::monospace(14.0), "中文")));
    }

    #[test]
    fn shared_font_context_exposes_families() {
        use animatix::renderer::text::available_font_families;

        assert!(!available_font_families(system_font_context()).is_empty());
    }
}
