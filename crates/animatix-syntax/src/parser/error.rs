//! Structured parse errors.

use std::ops::Range;

use chumsky::error::{RichPattern, RichReason};
use chumsky::prelude::*;

use crate::ast::ByteSpan;
use crate::diagnostics::{
    Diagnostic, DiagnosticCode, DiagnosticPhase, DiagnosticPresentation, ParseExpectation,
    ParsePresentation,
};
use crate::token::TokenKind;

// Chumsky carries custom error payloads as strings. Pack rule identity and its
// source argument there, then restore the unchanged canonical text at the parser boundary.
const FIELD_SEPARATOR: char = '\u{1f}';

pub(super) fn rule_error<'src>(
    span: ByteSpan,
    id: &str,
    canonical: impl std::fmt::Display,
    argument: Option<&str>,
) -> Rich<'src, TokenKind, ByteSpan> {
    let argument = argument.unwrap_or_default();
    Rich::custom(span, format!("{id}{FIELD_SEPARATOR}{argument}{FIELD_SEPARATOR}{canonical}"))
}

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
    /// Display data for frontends that localize parser messages.
    pub presentation: Option<ParsePresentation>,
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
        let diagnostic = Diagnostic::error(DiagnosticCode::ParseError, DiagnosticPhase::Parse, msg)
            .with_location(self.line, self.column, self.span.clone());
        match &self.presentation {
            Some(problem) => diagnostic.with_presentation(DiagnosticPresentation::Parse {
                problem: problem.clone(),
                context: self.context.clone(),
            }),
            None => diagnostic,
        }
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

        let message;
        let presentation;
        let mut expected = Vec::new();
        let mut found = None;

        match err.reason() {
            RichReason::ExpectedFound {
                expected: exp,
                found: f,
            } => {
                // Drop chumsky's placeholder label and dedupe: the raw
                // expectation list leaks combinator internals ("expected
                // something else, statement, something else, ...") that read
                // as noise to authors.
                let mut seen = std::collections::HashSet::new();
                let mut patterns = Vec::new();
                for pattern in exp {
                    let canonical = pattern.to_string();
                    if canonical == "something else" || !seen.insert(canonical.clone()) {
                        continue;
                    }
                    patterns.push(match pattern {
                        RichPattern::Label(label) => ParseExpectation::Rule(label.to_string()),
                        RichPattern::Any => ParseExpectation::Any,
                        RichPattern::EndOfInput => ParseExpectation::EndOfInput,
                        _ => ParseExpectation::Syntax(canonical.clone()),
                    });
                    expected.push(canonical);
                }
                found = f.as_ref().map(|c| c.to_string());
                let expected_str = expected.join(", ");
                message = match (expected_str.is_empty(), found.as_ref()) {
                    (false, Some(f)) => format!("expected {expected_str}, found '{f}'"),
                    (false, None) => format!("expected {expected_str}, found end of input"),
                    (true, Some(f)) => format!("unexpected '{f}'"),
                    (true, None) => "unexpected end of input".to_string(),
                };
                presentation = Some(ParsePresentation::Mismatch {
                    expected: patterns,
                    found: found.clone(),
                });
            },
            RichReason::Custom(msg) => {
                let mut fields = msg.splitn(3, FIELD_SEPARATOR);
                if let (Some(id), Some(argument), Some(canonical)) =
                    (fields.next(), fields.next(), fields.next())
                {
                    message = canonical.to_owned();
                    presentation = Some(ParsePresentation::Rule {
                        id: id.to_owned(),
                        argument: (!argument.is_empty()).then(|| argument.to_owned()),
                    });
                } else {
                    message = msg.clone();
                    presentation = None;
                }
            },
        }

        let context: Vec<String> = err.contexts().map(|(pattern, _)| pattern.to_string()).collect();

        Self {
            message,
            span: start..end,
            line,
            column,
            expected,
            found,
            context,
            presentation,
        }
    }
}
