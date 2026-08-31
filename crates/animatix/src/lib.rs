#![warn(missing_docs)]

//! Core animation engine: timeline, renderer, and evaluation.

// Re-export syntax modules internally so animatix code can use `crate::ast` etc.
pub(crate) use animatix_syntax::{ast, diagnostics, easing, icon_glyphs, module};

// Runtime modules (stay in animatix)
pub mod composition;
/// Extension context for registering capabilities per build.
pub mod extension_context;
pub use extension_context::ExtensionRegistry;
/// Plugin loader for extension contexts.
pub mod extension_plugin;
/// Intermediate representation module.
pub mod ir;
/// Shared hierarchical stage tracing (PF-8); see `docs/performance_evaluation.md`.
pub mod perf;
/// Primitive shape and media types.
pub mod primitives;
/// Unified property descriptors shared by built-in and extension properties.
pub mod property_descriptor;
/// Rendering backend (Vello/WGPU-based).
pub mod renderer;
/// Timeline construction, evaluation, and animation engine.
pub mod timeline;
