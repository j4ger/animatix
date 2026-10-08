//! `eparts` — Reusable egui widget + design-token library extracted from Animatix.
//!
//! The **tokens** module exposes generic design tokens: colors, spacing, typography, and motion.
//! The **widget** module exposes domain-agnostic egui widgets: button, row, layout, dialog,
//! context_menu, toast, anim, text, timeline, easing curve editor, and a diagnostics list
//! generic over the `DiagnosticEntry` trait.

pub mod tokens;
pub mod widget;

// Trait that decouples the diagnostics widget from animatix-domain types.
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
pub use tokens::theme_json::{ThemeFile, theme_schema_json};
#[cfg(feature = "theme-json")]
pub use tokens::theme_registry::{ThemeRegistry, ThemeRegistryError};
#[cfg(feature = "theme-json")]
pub use tokens::theme_registry_watcher::{ThemeRegistryWatcher, ThemeRegistryWatcherEvent};
#[cfg(feature = "theme-json")]
pub use tokens::theme_watcher::{ThemeWatcher, ThemeWatcherEvent};
pub use widget::button::Button;
pub use widget::color_picker::ColorPicker;
pub use widget::diagnostics::DiagnosticEntry;
// ── Feedback / status widgets ──────────────────────────────────────
pub use widget::feedback::{Alert, AlertLevel, Badge, ProgressBar, Tag};
// ── Input widgets (C2 + C3) ─────────────────────────────────────────
pub use widget::input::{NumberField, TextField};
pub use widget::kbd::{Kbd, format_shortcut};
pub use widget::list::{List, ListAction, ListResponse, SearchableList, SearchableListResponse};
pub use widget::popover::Popover;
pub use widget::row::Row;
pub use widget::select::Select;
pub use widget::spinner::Spinner;
pub use widget::tabs::TabBar;
pub use widget::toast::ToastQueue;
pub use widget::toggle::{Checkbox, Radio, Side, Switch};
pub use widget::tooltip::{Tooltip, text_tooltip};
// ── Shared widget traits & Size (A1 + A2) ──────────────────────────
pub use widget::traits::{Collapsible, Disableable, Selectable, Sizable, Size};
// ── Tree, List, SearchableList (H1 + H2 + H3) ──────────────────────
pub use widget::tree::{Tree, TreeAction, TreeId, TreeItem, TreeResponse};

// ── Prelude ────────────────────────────────────────────────────────
//
// Canonical import surface for downstream apps: `use eparts::prelude::*;`.
// Re-exports the common design tokens plus the most-used widgets. This is
// purely additive — every name here (and the root glob below) is the *same*
// item already exported above or via `eparts::widget`, so nothing is shadowed.
pub mod prelude {
    // Tokens (design system)
    pub use crate::tokens::motion::{
        MotionPreference, motion_preference, motion_preference_from_ctx, resolve_duration,
        set_motion_preference,
    };
    pub use crate::tokens::spatial::{
        Density, Spatial, density, density_from_ctx, set_density, spatial, spatial_from_ctx,
    };
    pub use crate::tokens::theme::{AppThemeChoice, Theme, set_theme, theme, theme_from_ctx};
    pub use crate::tokens::typography::TextRole;
    // Widgets
    pub use crate::widget::button::Button;
    pub use crate::widget::color_picker::ColorPicker;
    pub use crate::widget::feedback::{Alert, AlertLevel, Badge, ProgressBar, Tag};
    pub use crate::widget::input::{NumberField, TextField};
    pub use crate::widget::kbd::{Kbd, format_shortcut};
    pub use crate::widget::list::{
        List, ListAction, ListResponse, SearchableList, SearchableListResponse,
    };
    pub use crate::widget::popover::{Popover, PopoverDirection, PopoverResponse};
    pub use crate::widget::row::Row;
    pub use crate::widget::select::Select;
    pub use crate::widget::spinner::Spinner;
    pub use crate::widget::tabs::TabBar;
    pub use crate::widget::toast::ToastQueue;
    pub use crate::widget::toggle::{Checkbox, Radio, Side, Switch};
    pub use crate::widget::tooltip::{Tooltip, text_tooltip};
    pub use crate::widget::traits::{Collapsible, Disableable, Selectable, Sizable, Size};
    pub use crate::widget::tree::{Tree, TreeAction, TreeId, TreeItem, TreeResponse};
}

/// Glob re-export of [`prelude`] for `use eparts::*;` convenience.
pub use prelude::*;

#[cfg(test)]
mod export_tests {
    // Locks the public root + prelude surface: this module only compiles if the
    // names below resolve at the documented paths. Add to it when new widgets
    // become part of the stable import surface.
    fn assert_type_exists<T>() {}

    #[test]
    fn root_exports_expected_widgets() {
        assert_type_exists::<crate::Button>();
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
        assert_type_exists::<crate::Tree<'static>>();
        assert_type_exists::<crate::List<'static>>();

        // Functions re-exported at the root.
        let _: fn(&egui::KeyboardShortcut, &egui::Context) -> String = crate::format_shortcut;
        let _: fn(&egui::Ui, egui::Id, &egui::Response, &str) = crate::text_tooltip;
    }

    #[test]
    fn prelude_exports_tokens_and_widgets() {
        use crate::prelude::*;
        assert_type_exists::<Theme>();
        assert_type_exists::<AppThemeChoice>();
        assert_type_exists::<TextRole>();
        assert_type_exists::<Spinner>();
        assert_type_exists::<Checkbox>();
        assert_type_exists::<Switch>();
        assert_type_exists::<Button>();
        assert_type_exists::<Size>();
        assert_type_exists::<ListAction>();
        assert_type_exists::<TreeAction>();
        assert_type_exists::<TreeId>();
        assert_type_exists::<TreeItem>();
        assert_type_exists::<TreeResponse>();
    }
}
