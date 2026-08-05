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
pub use widget::diagnostics::DiagnosticEntry;
// ── Input widgets (C2 + C3) ─────────────────────────────────────────
pub use widget::input::{NumberField, TextField};
pub use widget::list::{List, ListAction, ListResponse, SearchableList, SearchableListResponse};
pub use widget::spinner::Spinner;
pub use widget::toggle::{Checkbox, Radio, Side, Switch};
// ── Shared widget traits & Size (A1 + A2) ──────────────────────────
pub use widget::traits::{Collapsible, Disableable, Selectable, Sizable, Size};
// ── Tree, List, SearchableList (H1 + H2 + H3) ──────────────────────
pub use widget::tree::{Tree, TreeAction, TreeId, TreeItem, TreeResponse};
