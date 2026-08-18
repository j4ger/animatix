#![warn(missing_docs)]

//! Parser and AST for the Animatix animation DSL.

pub mod ast;
/// Single registry of built-in names, types, and documentation.
pub mod builtins;
/// Error and warning reporting types for the animation pipeline.
pub mod diagnostics;
/// Animation easing curves and interpolation functions.
pub mod easing;
/// Shared formatting logic used by both `ToSource` and `Formatter`.
pub mod format_core;
/// Configurable source formatter for `.amx` files.
pub mod formatter;
/// Shared token-role classification for GUI and LSP highlighting.
pub mod highlight;
/// Icon glyph constants for UI primitives.
pub mod icon_glyphs;
pub mod module;
pub mod occurrence;
pub mod parser;
/// Shared actor/property schema for runtime, analyzer, LSP, and GUI.
pub mod schema;
/// Re-export of chumsky for integration tests that need `chumsky::Parser`.
#[doc(hidden)]
pub use chumsky;
/// Canonical semantic diagnostics emitted from the syntax layer.
pub mod semantic_diagnostics;
pub mod source_index;
/// Semantic symbol extraction shared by the typechecker and analyzer.
pub mod symbol_table;
pub mod to_source;
/// Lossless tokenizer for highlighting and position queries.
pub mod token;
pub mod transition_registry;
/// Gradual type checker for component/action parameter validation.
pub mod typecheck;
/// Symbol-aware type inference shared by the typechecker and analyzer.
pub mod typing;
pub mod walk;
