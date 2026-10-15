//! egui font registration for the desktop application.
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

const GEIST_REGULAR: &[u8] = include_bytes!("../assets/fonts/Geist-Regular.ttf");
const GEIST_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Geist-Medium.ttf");
const GEIST_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Geist-SemiBold.ttf");
const JB_MONO_REGULAR: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf");
const JB_MONO_MEDIUM: &[u8] = include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf");

/// Install Geist Sans, JetBrains Mono, Phosphor icons, and system fallback fonts.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    // 1. Install Geist Sans and JetBrains Mono font faces
    fonts
        .font_data
        .insert("GeistRegular".to_owned(), Arc::new(FontData::from_static(GEIST_REGULAR)));
    fonts
        .font_data
        .insert("GeistMedium".to_owned(), Arc::new(FontData::from_static(GEIST_MEDIUM)));
    fonts
        .font_data
        .insert("GeistSemiBold".to_owned(), Arc::new(FontData::from_static(GEIST_SEMIBOLD)));
    fonts.font_data.insert(
        "JetBrainsMonoRegular".to_owned(),
        Arc::new(FontData::from_static(JB_MONO_REGULAR)),
    );
    fonts.font_data.insert(
        "JetBrainsMonoMedium".to_owned(),
        Arc::new(FontData::from_static(JB_MONO_MEDIUM)),
    );

    // 2. Put Geist at the front of Proportional and JetBrains Mono at the front of Monospace
    let prop_family = fonts.families.entry(FontFamily::Proportional).or_default();
    prop_family.insert(0, "GeistRegular".to_owned());

    let mono_family = fonts.families.entry(FontFamily::Monospace).or_default();
    mono_family.insert(0, "JetBrainsMonoRegular".to_owned());

    // 3. Register Phosphor icons and system fallback glyphs
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    add_system_fallbacks(&mut fonts, system_fallbacks());

    // 4. Inherit fallbacks for named families
    let prop_fallbacks: Vec<String> = fonts
        .families
        .get(&FontFamily::Proportional)
        .map(|f| f.iter().skip(1).cloned().collect())
        .unwrap_or_default();
    let mono_fallbacks: Vec<String> = fonts
        .families
        .get(&FontFamily::Monospace)
        .map(|f| f.iter().skip(1).cloned().collect())
        .unwrap_or_default();

    let medium = fonts.families.entry(FontFamily::Name("GeistMedium".into())).or_default();
    medium.push("GeistMedium".to_owned());
    medium.extend(prop_fallbacks.clone());

    let semibold = fonts.families.entry(FontFamily::Name("GeistSemiBold".into())).or_default();
    semibold.push("GeistSemiBold".to_owned());
    semibold.extend(prop_fallbacks);

    let mono_medium = fonts
        .families
        .entry(FontFamily::Name("JetBrainsMonoMedium".into()))
        .or_default();
    mono_medium.push("JetBrainsMonoMedium".to_owned());
    mono_medium.extend(mono_fallbacks);

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
        if system_font_context().font_for_glyphs(&['中', '文']).is_empty() {
            return; // CI/machines without a Chinese font should not fail this test.
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
