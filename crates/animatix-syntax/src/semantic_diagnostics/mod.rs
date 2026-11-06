//! Canonical semantic diagnostics emitted from the syntax layer.
//!
//! The analyzer converts these once, span-first, into its transport DTO
//! (`analyzer::diagnostics::convert_syntax_diagnostic`) instead of
//! re-implementing label/property/type checks.

use crate::ast::{Span, Stmt, is_array_member_label};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticSeverity};
use crate::symbol_table::SymbolTable;
use crate::token::Token;
use crate::typing;

#[cfg(test)]
use crate::ast::InlineItem;

mod checks;
mod collect;
mod spans;
#[cfg(test)]
mod tests;

pub use collect::{collect_semantic_diagnostics, plot_runtime_params};
pub(crate) use spans::{
    find_decl_type_range, find_ident_range, is_component_array_member, property_positions,
    range_diagnostic, span_diagnostic, span_positions, stage_param_segment, target_source_text,
    type_mismatch_diagnostic,
};
