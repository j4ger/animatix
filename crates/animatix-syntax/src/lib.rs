#![warn(missing_docs)]

//! Parser and AST for the Animatix animation DSL.

pub mod ast;
/// Error and warning reporting types for the animation pipeline.
pub mod diagnostics;
/// Configurable source formatter for `.amx` files.
pub mod formatter;
/// Shared formatting logic used by both `ToSource` and `Formatter`.
pub mod format_core;
/// Animation easing curves and interpolation functions.
pub mod easing;
/// Icon glyph constants for UI primitives.
pub mod icon_glyphs;
pub mod module;
pub mod parser;
/// Re-export of chumsky for integration tests that need `chumsky::Parser`.
#[doc(hidden)]
pub use chumsky;
pub mod source_index;
/// Tree-sitter CST → Animatix AST converter for incremental parsing.
pub mod ts_convert;
pub mod to_source;
pub mod transition_registry;
/// Gradual type checker for component/action parameter validation.
pub mod typecheck;
