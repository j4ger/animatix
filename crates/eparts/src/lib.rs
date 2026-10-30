//! `eparts` — Reusable egui widget + design-token library extracted from Animatix.
//!
//! The **tokens** module exposes generic design tokens: colors, spacing, typography, and motion.
//! The **widget** module exposes domain-agnostic egui widgets: button, row, layout, dialog,
//! context_menu, toast, anim, text, timeline, easing curve editor, and a diagnostics list
//! generic over the `DiagnosticEntry` trait.

pub mod tokens;
pub mod widget;

// ── Motion preference (reduced-motion) ───────────────────────────────
pub use tokens::motion::{
    MotionPreference, motion_preference, motion_preference_from_ctx, resolve_duration,
    set_motion_preference,
};
pub use tokens::spatial::{
    Density, Spatial, density, density_from_ctx, set_density, spatial, spatial_from_ctx,
};
// ── Runtime theme (B1) ──────────────────────────────────────────────
pub use tokens::theme::{AppThemeChoice, Theme, set_theme, theme, theme_from_ctx};
#[cfg(feature = "theme-json")]
pub use tokens::theme_json::ThemeFile;
#[cfg(feature = "theme-json")]
pub use tokens::theme_registry::{ThemeRegistry, ThemeRegistryError};
#[cfg(feature = "theme-json")]
pub use tokens::theme_registry_watcher::{ThemeRegistryWatcher, ThemeRegistryWatcherEvent};
pub use widget::button::{Button, ButtonAlign};
pub use widget::color_picker::ColorPicker;
pub use widget::diagnostics::DiagnosticEntry;
// ── Feedback / status widgets ──────────────────────────────────────
pub use widget::feedback::{Alert, AlertLevel, Badge, ProgressBar, Tag};
// ── Input widgets (C2 + C3) ─────────────────────────────────────────
pub use widget::input::{NumberField, TextField};
pub use widget::kbd::{Kbd, format_shortcut};
pub use widget::popover::Popover;
pub use widget::row::Row;
pub use widget::select::Select;
pub use widget::slider::Slider;
pub use widget::spinner::Spinner;
pub use widget::tabs::TabBar;
pub use widget::toast::ToastQueue;
pub use widget::toggle::{Checkbox, Side, Switch};
pub use widget::tooltip::{Tooltip, text_tooltip};
// ── Shared widget traits & Size (A1 + A2) ──────────────────────────
pub use widget::traits::{Disableable, Selectable, Sizable, Size};

#[cfg(test)]
mod export_tests {
    // Locks the public root surface: this module only compiles if the names
    // below resolve at the documented paths. Add to it when new widgets become
    // part of the stable import surface.
    fn assert_type_exists<T>() {}

    #[test]
    fn root_exports_expected_widgets() {
        assert_type_exists::<crate::Button>();
        assert_type_exists::<crate::ButtonAlign>();
        assert_type_exists::<crate::Select<'static>>();
        assert_type_exists::<crate::Popover>();
        assert_type_exists::<crate::Kbd>();
        assert_type_exists::<crate::Tooltip>();
        assert_type_exists::<crate::Alert>();
        assert_type_exists::<crate::AlertLevel>();
        assert_type_exists::<crate::Badge>();
        assert_type_exists::<crate::ProgressBar>();
        assert_type_exists::<crate::Tag>();
        assert_type_exists::<crate::ToastQueue>();
        assert_type_exists::<crate::TabBar<'static>>();
        assert_type_exists::<crate::Row<'static>>();
        assert_type_exists::<crate::ColorPicker<'static>>();

        // Functions re-exported at the root.
        let _: fn(&egui::KeyboardShortcut, &egui::Context) -> String = crate::format_shortcut;
        let _: fn(&egui::Ui, egui::Id, &egui::Response, &str) = crate::text_tooltip;
    }
}
