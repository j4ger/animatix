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
