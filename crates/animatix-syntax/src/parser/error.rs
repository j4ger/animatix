//! Structured parse errors.

use std::ops::Range;

use chumsky::prelude::*;

use crate::ast::ByteSpan;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};

/// A structured parse error with human-readable location and context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Human-readable error message.
    pub message: String,
    /// Byte span in the source where the error occurred.
    pub span: Range<usize>,
    /// 1-based line number of the error.
    pub line: usize,
    /// 1-based column number of the error.
    pub column: usize,
    /// Descriptions of what the parser expected at this point.
    pub expected: Vec<String>,
    /// The token or text the parser actually found, if any.
    pub found: Option<String>,
    /// Parser context stack (labels of enclosing grammar rules).
    pub context: Vec<String>,
}

impl ParseError {
    /// Convert this parse error into a `Diagnostic` for use with the unified
    /// [`format_diagnostic`](crate::diagnostics::format_diagnostic) formatter.
    ///
    /// The resulting diagnostic has:
    /// * `code`: [`DiagnosticCode::ParseError`]
    /// * `phase`: [`DiagnosticPhase::Parse`]
    /// * `severity`: [`crate::diagnostics::DiagnosticSeverity::Error`]
    /// * `message`: the parse error message plus parser context
    /// * `location`: the line, column, and byte span of the error
    pub fn to_diagnostic(&self) -> Diagnostic {
        let mut msg = self.message.clone();
        if !self.context.is_empty() {
            msg.push_str(&format!("\n  in {}", self.context.join(" > ")));
        }
        Diagnostic::error(DiagnosticCode::ParseError, DiagnosticPhase::Parse, msg).with_location(
            self.line,
            self.column,
            self.span.clone(),
        )
    }

    /// Convert a chumsky `Rich` token error into a structured `ParseError`.
    pub fn from_rich_tokens(
        source: &str,
        err: &Rich<'_, crate::token::TokenKind, ByteSpan>,
    ) -> Self {
        let span = err.span();
        let start = span.start;
        let end = span.end;
        let (line0, column0) = crate::token::byte_to_line_col(source, start);
        let line = line0 + 1;
        let column = column0 + 1;

        let mut _message = String::new();
        let mut expected = Vec::new();
        let mut found = None;

        match err.reason() {
            chumsky::error::RichReason::ExpectedFound {
                expected: exp,
                found: f,
            } => {
                // Drop chumsky's placeholder label and dedupe: the raw
                // expectation list leaks combinator internals ("expected
                // something else, statement, something else, ...") that read
                // as noise to authors.
                let mut seen = std::collections::HashSet::new();
                expected = exp
                    .iter()
                    .map(|p| p.to_string())
                    .filter(|label| label != "something else")
                    .filter(|label| seen.insert(label.clone()))
                    .collect();
                found = f.as_ref().map(|c| c.to_string());
                let expected_str = expected.join(", ");
                match (expected_str.is_empty(), found.as_ref()) {
                    (false, Some(f)) => _message = format!("expected {expected_str}, found '{f}'"),
                    (false, None) => {
                        _message = format!("expected {expected_str}, found end of input")
                    },
                    (true, Some(f)) => _message = format!("unexpected '{f}'"),
                    (true, None) => _message = "unexpected end of input".to_string(),
                }
            },
            chumsky::error::RichReason::Custom(msg) => {
                _message = msg.clone();
            },
        }

        let context: Vec<String> = err.contexts().map(|(pattern, _)| pattern.to_string()).collect();

        Self {
            message: _message,
            span: start..end,
            line,
            column,
            expected,
            found,
            context,
        }
    }
}
