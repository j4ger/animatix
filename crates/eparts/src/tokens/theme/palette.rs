use egui::Color32;

#[cfg(feature = "theme-json")]
use super::serde_color32;

// ── Nested slot structs ───────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Surface {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub base: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub panel: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub surface: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub widget: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub hover: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub active: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub floating_card_bg: Color32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Text {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub primary: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub secondary: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub muted: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub disabled: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub on_accent: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub faint: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub subtle: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub hover: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub dim: Color32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Accent {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub primary: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub cyan: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub primary_hover: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub primary_active: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub faint: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub ghost: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub subtle: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub hover: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub strong: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub selection: Color32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Status {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub success: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub warning: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub error: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub info: Color32,
    // Editor-specific semantic aliases retained for Animatix's diagnostics
    // and transport UI. Generic consumers can map these onto the status slots
    // above; future work should move them to the app semantic layer.
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub playing_text: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub diagnostic_error: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub diagnostic_warning: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub success_faint: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub success_ultra_faint: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub warning_subtle: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub error_faint: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub error_ultra_faint: Color32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Border {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub default: Color32,
    /// `strong` mirrors `semantic::border::HOVER` (the strongest neutral border).
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub strong: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub focus: Color32,
    /// Subtle divider line (semi-transparent stroke).
    #[cfg_attr(feature = "theme-json", serde(default, with = "serde_color32"))]
    pub subtle: Color32,
    /// 1px top edge highlight / rim bevel for elevated containers.
    #[cfg_attr(feature = "theme-json", serde(default, with = "serde_color32"))]
    pub rim_highlight: Color32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Overlay {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub backdrop: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub badge_bg: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub tooltip_bg: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub shadow_ambient: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub shadow_direct: Color32,
    /// Translucent floating HUD background (pill capsule over canvas).
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub floating_hud_bg: Color32,
}

/// Neutral grid / guide line colors (white-alpha; not brand-specific).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Lines {
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub grid: Color32,
    #[cfg_attr(feature = "theme-json", serde(with = "serde_color32"))]
    pub guide: Color32,
}

// ── Theme groups ──────────────────────────────────────────────────────

/// Palette-scoped tokens: surfaces, text, accents, status colors, borders,
/// overlays, and neutral lines.
///
/// Accessed as `theme.palette.surface.base`, `theme.palette.text.primary`, …
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Palette {
    pub surface: Surface,
    pub text: Text,
    pub accent: Accent,
    pub status: Status,
    pub border: Border,
    pub overlay: Overlay,
    /// Neutral grid / guide line colors.
    pub lines: Lines,
}
