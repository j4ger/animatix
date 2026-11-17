//! Lowered inputs to the built-in track writers.
//!
//! This is an engine instruction, not an authored statement or a second call
//! syntax. The plan lowerer resolves typed arguments and actor identities
//! before handing an instruction to these existing track-writing systems.

use crate::ast::{ByteSpan, Expr, Modifier};

/// A validated, scheduled built-in operation ready to write ordinary tracks.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltinInvocation {
    /// Canonical unqualified standard function name.
    pub verb: String,
    /// Resolved actor labels at the build-only backend boundary.
    pub targets: Vec<String>,
    /// Index expressions retained by direct backend tests and generated paths.
    pub target_index: Vec<Option<Expr>>,
    /// Indexed segment positions, aligned with targets.
    pub target_index_position: Vec<Option<usize>>,
    /// Backend-specific positional payloads.
    pub args: Vec<Expr>,
    /// Typed values encoded as expressions for existing property readers.
    pub modifiers: Vec<Modifier>,
    /// Source location of the scheduled call.
    pub byte_span: Option<ByteSpan>,
}
