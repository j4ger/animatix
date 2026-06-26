//! `eparts` — Reusable egui widget + design-token library extracted from Animatix.
//!
//! The **tokens** module exposes generic design tokens: colors, spacing, typography, and motion.
//! The **widget** module exposes domain-agnostic egui widgets: button, row, layout, dialog,
//! context_menu, toast, anim, text, timeline, easing curve editor, and a diagnostics list
//! generic over the `DiagnosticEntry` trait.

pub mod tokens;
pub mod widget;

// Trait that decouples the diagnostics widget from animatix-domain types.
pub use widget::diagnostics::DiagnosticEntry;
pub use widget::spinner::Spinner;

// ── Shared widget traits & Size (A1 + A2) ──────────────────────────
pub use widget::traits::{Collapsible, Disableable, Selectable, Size, Sizable};

// ── Runtime theme (B1) ──────────────────────────────────────────────
pub use tokens::theme::{theme, theme_from_ctx, set_theme, AppThemeChoice, Theme};

// ── Tree, List, SearchableList (H1 + H2 + H3) ──────────────────────
pub use widget::tree::{Tree, TreeAction, TreeId, TreeItem, TreeResponse};
pub use widget::list::{List, ListAction, ListResponse, SearchableList, SearchableListResponse};

// ── Input widgets (C2 + C3) ─────────────────────────────────────────
pub use widget::input::{TextField, NumberField};
pub use widget::toggle::{Checkbox, Radio, Side, Switch};
