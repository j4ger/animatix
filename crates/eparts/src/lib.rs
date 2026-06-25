//! `eparts` — Reusable egui widgets and design tokens extracted from Animatix.
//!
//! Phase 2 provides the generic half of the token system. Widget modules
//! (Phase 3+) will be added alongside.

pub mod tokens;
pub mod widget;

// Trait that decouples the diagnostics widget from animatix-domain types.
pub use widget::diagnostics::DiagnosticEntry;
