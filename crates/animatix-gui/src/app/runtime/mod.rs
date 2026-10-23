use eframe::egui;
#[cfg(feature = "theme-json")]
use eparts::{ThemeRegistry, ThemeRegistryWatcher, ThemeRegistryWatcherEvent};

use super::*;
use crate::app::audio::AudioEngine;
use crate::app::commands::{
    ActorCommand, DocumentCommand, KeyframeCommand, PlaybackCommand, SceneCommand, ShellAction,
    ViewAction, ViewCommand,
};
use crate::app::design_tokens::spatial::spatial_from_ctx;
use crate::app::persistence::{
    clear_app_state, load_app_state, load_workspace_persistence, persistence_path, save_app_state,
};

struct AnimatixApp {
    shell: GuiShell,
    preview_surface: PreviewSurface,
    preview_texture_id: Option<egui::TextureId>,
    /// Audio playback engine for preview audio synced with the timeline.
    audio_engine: Option<AudioEngine>,
    /// Cached OS dark/light detection for the `Auto` theme (None = unknown).
    system_is_dark: Option<bool>,
    /// Last time the OS theme was probed (periodic re-detect for Auto mode).
    last_theme_probe: Option<std::time::Instant>,
    /// The theme choice applied on the previous frame, to avoid redundant work.
    applied_theme_signature: Option<(eparts::AppThemeChoice, Option<bool>)>,
    /// The motion preference applied on the previous frame, to avoid redundant work.
    applied_motion_preference: Option<bool>,
    /// The density applied on the previous frame, to avoid redundant work.
    applied_density: Option<eparts::Density>,
    /// Loaded JSON theme registry when `theme-json` is enabled.
    #[cfg(feature = "theme-json")]
    theme_registry: Option<ThemeRegistry>,
    /// Live watcher for the configured JSON theme directory.
    #[cfg(feature = "theme-json")]
    theme_registry_watcher: Option<ThemeRegistryWatcher>,
    /// Last theme registry error, surfaced in Settings.
    #[cfg(feature = "theme-json")]
    theme_error: Option<String>,
    /// Opt-in JSONL perf telemetry sink (`--perf-log`, PF-9).
    perf_log: Option<crate::app::perf_log::PerfLogSink>,
    /// Bounded whole-window screenshot session (`ANIMATIX_SCREENSHOT`), used for
    /// visual layout review. Only compiled with `dev-screenshots`.
    #[cfg(feature = "dev-screenshots")]
    screenshot: Option<ScreenshotSession>,
}

/// Probe the OS light/dark appearance via the `dark-light` crate.
///
/// Cross-platform: XDG Desktop Portal on Linux, native APIs on Windows/macOS.
/// Returns `None` when the OS does not report a preference.
pub(crate) fn detect_system_dark() -> Option<bool> {
    match dark_light::detect() {
        Ok(dark_light::Mode::Dark) => Some(true),
        Ok(dark_light::Mode::Light) => Some(false),
        _ => None,
    }
}

mod app_impl;
mod eframe_impl;
mod helpers;
mod run;
mod screenshot;

pub use run::run_gui;
