//! Serializable diagnostic shape crossing the wasm boundary to the JS shell.

use serde::Serialize;

use animatix_syntax::diagnostics::{Diagnostic, DiagnosticSeverity};

/// A flattened, JSON-friendly diagnostic for the web editor.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct DiagnosticDto {
    /// "error" | "warning" | "info" | "hint".
    pub severity: String,
    /// Kebab-case diagnostic code (e.g. "parse-error"), empty when unknown.
    pub code: String,
    pub message: String,
    /// Subject the diagnostic refers to, when known.
    pub subject: Option<String>,
    /// 1-based source line, when known.
    pub line: Option<usize>,
    /// 1-based source column (character offset), when known.
    pub column: Option<usize>,
    /// Byte-offset range into the source text, when known.
    pub span: Option<[usize; 2]>,
}

impl DiagnosticDto {
    pub fn from_diagnostic(d: &Diagnostic) -> Self {
        let severity = match d.severity {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Info => "info",
            DiagnosticSeverity::Hint => "hint",
        };
        Self {
            severity: severity.to_string(),
            code: d.code.to_string(),
            message: d.message.clone(),
            subject: d.location.subject.clone(),
            line: d.location.line,
            column: d.location.column,
            span: d.location.span.as_ref().map(|r| [r.start, r.end]),
        }
    }
}

/// Result payload returned to JS from `load_source`.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct LoadResultDto {
    /// Whether a renderable document is now loaded (parse + build succeeded).
    pub ok: bool,
    /// Document duration in seconds (0.1 floor, mirroring the GUI).
    pub duration_s: f64,
    /// Scene dimensions in pixels (the canvas aspect should follow these).
    pub width: u32,
    pub height: u32,
    pub diagnostics: Vec<DiagnosticDto>,
    /// Resolved import paths the module graph could not find, when the load
    /// stopped on a missing file. The shell fetches each (relative to the
    /// scene URL), registers it via `add_module`, and calls `load_source`
    /// again — the closure loop that lets a page play scenes importing
    /// `.amx` files beyond the bundled library. Empty on every other outcome.
    pub missing_imports: Vec<String>,
}
