use super::format::severity_summary;
use super::*;

/// Source location associated with a diagnostic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticLocation {
    /// Path to the source file, if known.
    pub path: Option<PathBuf>,
    /// Subject (e.g. identifier) the diagnostic refers to, if any.
    pub subject: Option<String>,
    /// 1-based line number where the error occurs.
    pub line: Option<usize>,
    /// 1-based column number where the error occurs.
    ///
    /// This is a **character (grapheme) offset**, not a byte offset.
    /// Converting from byte offsets (e.g. from parser spans) must account
    /// for multi-byte UTF-8 characters. Use [`crate::ast::Span::from_range`]
    /// which performs this conversion correctly.
    pub column: Option<usize>,
    /// 1-based line number where the diagnostic range ends.
    pub end_line: Option<usize>,
    /// 1-based column number where the diagnostic range ends.
    pub end_col: Option<usize>,
    /// Byte-offset range into the source text.
    pub span: Option<Range<usize>>,
}

/// A single diagnostic message produced by the compiler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// Severity of this diagnostic.
    pub severity: DiagnosticSeverity,
    /// Pipeline phase that produced this diagnostic.
    pub phase: DiagnosticPhase,
    /// Machine-readable code identifying the diagnostic kind.
    pub code: DiagnosticCode,
    /// Human-readable message describing the issue.
    pub message: String,
    /// Source location associated with the diagnostic.
    pub location: DiagnosticLocation,
}

impl Diagnostic {
    /// Creates a new warning diagnostic with the given code, phase, and message.
    pub fn warning(
        code: DiagnosticCode,
        phase: DiagnosticPhase,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            phase,
            code,
            message: message.into(),
            location: DiagnosticLocation::default(),
        }
    }

    /// Creates a new error diagnostic with the given code, phase, and message.
    pub fn error(code: DiagnosticCode, phase: DiagnosticPhase, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            phase,
            code,
            message: message.into(),
            location: DiagnosticLocation::default(),
        }
    }

    /// Attaches a source file path to the diagnostic.
    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.location.path = Some(path.into());
        self
    }

    /// Attaches a subject identifier to the diagnostic.
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.location.subject = Some(subject.into());
        self
    }

    /// Attaches line, column, and byte span information to the diagnostic.
    pub fn with_location(mut self, line: usize, column: usize, span: Range<usize>) -> Self {
        self.location.line = Some(line);
        self.location.column = Some(column);
        self.location.span = Some(span);
        self
    }

    /// Attach source location from an AST span (line/col, no byte offsets).
    pub fn with_ast_span(mut self, span: Option<crate::ast::Span>) -> Self {
        if let Some(s) = span {
            self.location.line = Some(s.start_line);
            self.location.column = Some(s.start_col);
        }
        self
    }

    /// Attaches a byte span without line/column information.
    ///
    /// For producers that hold a span (e.g. a property's `value_span`) but have
    /// no access to the source text: a consumer that does hold the source can
    /// derive the line/column from the span.
    pub fn with_byte_span(mut self, span: Option<crate::ast::ByteSpan>) -> Self {
        if let Some(span) = span {
            self.location.span = Some(span.start..span.end);
        }
        self
    }

    /// Attaches an end position, so the diagnostic describes a range rather
    /// than a single point. Pairs with [`Diagnostic::with_location`] or
    /// [`Diagnostic::with_byte_span`].
    pub fn with_end_location(mut self, end_line: usize, end_col: usize) -> Self {
        self.location.end_line = Some(end_line);
        self.location.end_col = Some(end_col);
        self
    }

    /// Returns true if this diagnostic represents an error (not a warning).
    pub fn is_error(&self) -> bool {
        self.severity == DiagnosticSeverity::Error
    }
}

/// Result of a build step, carrying the output value plus any diagnostics.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildReport<T> {
    /// The successfully produced output value.
    pub output: T,
    /// All diagnostics emitted during the build.
    pub diagnostics: Vec<Diagnostic>,
}

impl<T> BuildReport<T> {
    /// Creates a new `BuildReport`, deduplicating diagnostics by (code, message,
    /// subject).
    pub fn new(output: T, mut diagnostics: Vec<Diagnostic>) -> Self {
        // Deduplicate: keep only the first occurrence of each
        // (code, message, subject) combination.
        let mut seen = std::collections::HashSet::new();
        diagnostics.retain(|d| {
            let key = (d.code, d.message.clone(), d.location.subject.clone());
            seen.insert(key)
        });
        Self {
            output,
            diagnostics,
        }
    }
}

/// Summary of diagnostics for a single pipeline phase.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticPhaseSummary {
    /// The pipeline phase being summarized.
    pub phase: DiagnosticPhase,
    /// Number of warnings in this phase.
    pub warnings: usize,
    /// Number of errors in this phase.
    pub errors: usize,
}

impl DiagnosticPhaseSummary {
    /// Returns the total number of diagnostics (warnings + errors) in this phase.
    pub fn total(&self) -> usize {
        self.warnings + self.errors
    }

    /// Returns a human-readable label for this phase summary.
    pub fn label(&self) -> String {
        format!("{}: {}", self.phase, severity_summary(self.warnings, self.errors))
    }
}
