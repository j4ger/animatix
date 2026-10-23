use egui::Context;

use super::model::Theme;

// ── Immediate-mode Memory accessors ───────────────────────────────────

/// Read the current `Theme` from the `Ui`'s `egui::Context`.
pub fn theme(ui: &egui::Ui) -> Theme {
    theme_from_ctx(ui.ctx())
}

/// Read the current `Theme` from an `egui::Context`.
pub fn theme_from_ctx(ctx: &Context) -> Theme {
    ctx.data(|d| d.get_temp::<Theme>(egui::Id::new("eparts_theme")))
        .unwrap_or_default()
}

/// Store a new `Theme` in the `egui::Context`'s `Memory`.
pub fn set_theme(ctx: &Context, theme: Theme) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("eparts_theme"), theme));
}

// ── App theme choice / system auto-theme policy (B11) ─────────────────

/// A user-facing theme preference. `Auto` follows the OS light/dark setting.
///
/// This is the reusable policy half of cross-platform auto-theming: the host
/// app detects the OS appearance (egui/winit natively on Windows/macOS, or the
/// `dark-light` crate on Linux) and calls [`AppThemeChoice::resolve`] to pick a
/// concrete [`Theme`]. The app keeps the choice (egui's resolved system theme is
/// `pub(crate)`), then `eparts::set_theme` + `ctx.set_visuals(theme.to_visuals(..))`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AppThemeChoice {
    /// Follow the OS light/dark setting (falls back to dark when unknown).
    #[default]
    Auto,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

impl AppThemeChoice {
    /// Resolve to a concrete [`Theme`]. `system_is_dark` is the detected OS
    /// appearance (`None` = unknown → dark fallback), only consulted for `Auto`.
    pub fn resolve(self, system_is_dark: Option<bool>) -> Theme {
        if self.is_dark(system_is_dark) {
            Theme::dark()
        } else {
            Theme::light()
        }
    }

    /// Whether the effective theme is dark, given the detected OS appearance.
    pub fn is_dark(self, system_is_dark: Option<bool>) -> bool {
        match self {
            AppThemeChoice::Light => false,
            AppThemeChoice::Dark => true,
            // Unknown system appearance falls back to dark (matches egui's default).
            AppThemeChoice::Auto => system_is_dark.unwrap_or(true),
        }
    }
}
