#![warn(missing_docs)]

//! Core animation engine: timeline, renderer, and evaluation.

// Re-export syntax modules from animatix-syntax for backward compatibility
pub use animatix_syntax::ast;
pub use animatix_syntax::diagnostics;
pub use animatix_syntax::easing;
pub use animatix_syntax::icon_glyphs;
pub use animatix_syntax::module;
pub use animatix_syntax::parser;
pub use animatix_syntax::source_index;
pub use animatix_syntax::to_source;
pub use animatix_syntax::transition_registry;

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
