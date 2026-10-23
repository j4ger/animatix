use egui::Shadow;

#[cfg(feature = "theme-json")]
use super::serde_shadow;

/// Elevation shadow tokens for floating surfaces.
///
/// Three conceptual levels: `flat` (no shadow, in-panel chrome — no token needed),
/// `raised` (popover / menu / dropdown / toast — soft small shadow), and
/// `overlay` (dialog / modal — larger shadow on top of backdrop scrim).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "theme-json", derive(serde::Serialize, serde::Deserialize))]
pub struct Elevation {
    /// Soft small shadow for popover, menu, dropdown, toast.
    ///   Dark: offset [0, 2] / blur 4 / spread 0 / rgba(0,0,0,40)
    ///   Light: offset [0, 3] / blur 6 / spread 0 / rgba(0,0,0,50)
    #[cfg_attr(feature = "theme-json", serde(with = "serde_shadow"))]
    pub raised: Shadow,
    /// Larger shadow for dialog / modal, painted on top of backdrop scrim.
    ///   Dark: offset [0, 8] / blur 24 / spread 0 / rgba(0,0,0,80)
    ///   Light: offset [0, 12] / blur 32 / spread 0 / rgba(0,0,0,60)
    #[cfg_attr(feature = "theme-json", serde(with = "serde_shadow"))]
    pub overlay: Shadow,
}
