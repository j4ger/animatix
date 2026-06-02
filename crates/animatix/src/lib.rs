#![warn(missing_docs)]

//! Core animation engine: timeline, renderer, and evaluation.

// Re-export syntax modules internally so animatix code can use `crate::ast` etc.
pub(crate) use animatix_syntax::ast;
pub(crate) use animatix_syntax::diagnostics;
pub(crate) use animatix_syntax::easing;
pub(crate) use animatix_syntax::icon_glyphs;
pub(crate) use animatix_syntax::module;

// Runtime modules (stay in animatix)
pub mod composition;
/// Intermediate representation module.
pub mod ir;
/// Primitive shape and media types.
pub mod primitives;
/// Rendering backend (Vello/WGPU-based).
pub mod renderer;
/// Timeline construction, evaluation, and animation engine.
pub mod timeline;
/// Modifier bytecode VM.
pub mod vm;
