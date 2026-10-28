//! Diagnostic collection from parse errors and semantic checks.

use std::collections::HashSet;

use animatix_syntax::ast::*;
use animatix_syntax::parser::ParseError;

use crate::symbol_table::SymbolTable;

/// A diagnostic message (error, warning, etc.).
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The severity level of this diagnostic.
    pub severity: DiagnosticSeverity,
    /// The 0-based line number where the issue starts.
    pub line: usize,
    /// The 0-based column number where the issue starts.
    pub col: usize,
    /// The 0-based line number where the issue ends.
    pub end_line: usize,
    /// The 0-based column number where the issue ends.
    pub end_col: usize,
    /// A human-readable description of the issue.
    pub message: String,
    /// An optional error code for categorisation.
    pub code: Option<String>,
    /// The identifier the diagnostic is about, when the producer recorded
    /// one (`undefined-label` → the missing label, `unknown-action` → the
    /// verb, `unused-label` → the declaration's name).
    ///
    /// Consumers must read this instead of re-parsing the English message.
    pub subject: Option<String>,
}

/// The severity of a diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// An error that will prevent correct rendering.
    Error,
    /// A warning about a potential issue.
    Warning,
    /// An informative message.
    Info,
    /// A helpful suggestion or hint.
    Hint,
}

impl Diagnostic {
    /// Returns true if this diagnostic is an error.
    pub fn is_error(&self) -> bool {
        self.severity == DiagnosticSeverity::Error
    }

    /// Returns true if this diagnostic is a warning.
    pub fn is_warning(&self) -> bool {
        self.severity == DiagnosticSeverity::Warning
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let severity = match self.severity {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Info => "info",
            DiagnosticSeverity::Hint => "hint",
        };
        let code = self.code.as_deref().unwrap_or("");
        write!(f, "{}:{}:{}: {}: {}", self.line + 1, self.col + 1, severity, code, self.message)
    }
}

/// Lint configuration for suppressing specific diagnostics.
#[derive(Debug, Default, Clone)]
pub struct LintConfig {
    /// Diagnostic codes to suppress (e.g., "unused-label", "unknown-property").
    pub disabled: HashSet<String>,
    /// Whether to disable all warnings.
    pub disable_all_warnings: bool,
}

impl LintConfig {
    /// A stable fingerprint of the configuration's effect on output.
    ///
    /// `HashSet` iteration order is unspecified, so the disabled codes are
    /// sorted before hashing; two configs with the same codes and the same
    /// `disable_all_warnings` always produce the same value, which is what a
    /// diagnostics cache keys on.
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};

        let mut codes: Vec<&str> = self.disabled.iter().map(String::as_str).collect();
        codes.sort_unstable();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        codes.hash(&mut hasher);
        self.disable_all_warnings.hash(&mut hasher);
        hasher.finish()
    }

    /// Parse lint config from inline comments in the source.
    /// Looks for `// lint-disable: code1, code2` and `// lint-disable-all-warnings`.
    pub fn from_source(source: &str) -> Self {
        let mut config = Self::default();
        for line in source.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("// lint-disable:") {
                for code in rest.split(',') {
                    let code = code.trim().to_lowercase();
                    if !code.is_empty() {
                        config.disabled.insert(code);
                    }
                }
            } else if trimmed == "// lint-disable-all-warnings" {
                config.disable_all_warnings = true;
            }
        }
        config
    }

    /// Load lint config from an `.amx.toml` file.
    pub fn from_file(path: &std::path::Path) -> Self {
        let Ok(content) = std::fs::read_to_string(path) else {
            return Self::default();
        };

        #[derive(serde::Deserialize)]
        struct LintFile {
            lint: Option<LintSection>,
        }

        #[derive(serde::Deserialize)]
        struct LintSection {
            disabled: Option<Vec<String>>,
            disable_all_warnings: Option<bool>,
        }

        match toml::from_str::<LintFile>(&content) {
            Ok(file) => {
                let mut config = Self::default();
                if let Some(lint) = file.lint {
                    if let Some(codes) = lint.disabled {
                        config.disabled = codes.into_iter().map(|c| c.to_lowercase()).collect();
                    }
                    config.disable_all_warnings = lint.disable_all_warnings.unwrap_or(false);
                }
                config
            },
            Err(_) => Self::default(),
        }
    }

    /// Merge another config into this one (combines disabled codes).
    pub fn merge(&mut self, other: &LintConfig) {
        self.disabled.extend(other.disabled.iter().cloned());
        if other.disable_all_warnings {
            self.disable_all_warnings = true;
        }
    }

    /// Check if a diagnostic code is disabled.
    pub fn is_disabled(&self, code: &str) -> bool {
        self.disabled.contains(code)
    }
}

/// Collect all diagnostics from the source.
pub fn collect_diagnostics(
    source: &str,
    parse_errors: &[ParseError],
    symbols: &SymbolTable,
    ast: Option<&[Stmt]>,
    tokens: &[animatix_syntax::token::Token],
) -> Vec<Diagnostic> {
    collect_diagnostics_with_config(
        source,
        parse_errors,
        symbols,
        ast,
        tokens,
        &LintConfig::default(),
    )
}

/// Collect all diagnostics from the source with lint configuration.
pub fn collect_diagnostics_with_config(
    source: &str,
    parse_errors: &[ParseError],
    symbols: &SymbolTable,
    ast: Option<&[Stmt]>,
    tokens: &[animatix_syntax::token::Token],
    config: &LintConfig,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // 1. Chumsky parse errors (structured with positions)
    for error in parse_errors {
        let end_span = Span::from_range(source, error.span.clone());
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            line: error.line.saturating_sub(1),
            col: error.column.saturating_sub(1),
            end_line: end_span.end_line.saturating_sub(1),
            end_col: end_span.end_col.saturating_sub(1),
            message: error.message.clone(),
            // One stable code so `lint-disable: parse-error` works regardless
            // of how many errors a file has.
            code: Some("parse-error".to_string()),
            subject: None,
        });
    }

    // 2. Semantic checks (if AST is available)
    if let Some(stmts) = ast {
        let syntax_diagnostics =
            animatix_syntax::semantic_diagnostics::collect_semantic_diagnostics(
                stmts, symbols, tokens, source,
            );
        diagnostics
            .extend(syntax_diagnostics.into_iter().map(|d| convert_syntax_diagnostic(source, d)));
    }

    // 2b. Suppress semantic warnings that share a line with a parse error.
    //
    // Error recovery (chumsky `skip_then_retry_until`) resumes at the next
    // statement start, so the tokens it skipped can still be re-read as a
    // malformed statement — e.g. a garbage line `@@@ nope @@@` parses as an
    // action `nope`, yielding a phantom "unknown action". That warning is an
    // artefact of the recovery, not a real problem with the source, so drop
    // warnings on any line that already carries a parse error. Errors are
    // never suppressed.
    let parse_error_lines: HashSet<usize> =
        diagnostics.iter().filter(|d| d.is_error()).map(|d| d.line).collect();
    if !parse_error_lines.is_empty() {
        diagnostics.retain(|d| {
            d.is_error()
                || d.severity == DiagnosticSeverity::Error
                || !parse_error_lines.contains(&d.line)
        });
    }

    // 3. Filter based on lint config
    diagnostics.retain(|d| {
        // Never suppress errors
        if d.severity == DiagnosticSeverity::Error {
            return true;
        }
        // Check if all warnings are disabled
        if config.disable_all_warnings && d.severity == DiagnosticSeverity::Warning {
            return false;
        }
        // Check if specific code is disabled
        if let Some(code) = &d.code {
            if config.is_disabled(code) {
                return false;
            }
        }
        true
    });

    diagnostics
}

/// Convert a syntax-level diagnostic into the analyzer DTO.
///
/// This is the **only** syntax→analyzer boundary: syntax locations are 1-based
/// and may carry either a byte span or line/column, while the analyzer DTO is
/// 0-based. Byte spans win when the producer recorded one, because producers
/// such as the type checker hold only a property's `value_span` and no text;
/// resolving that span against `source` is what lands the diagnostic on the
/// offending property instead of the file start.
pub(crate) fn convert_syntax_diagnostic(
    source: &str,
    d: animatix_syntax::diagnostics::Diagnostic,
) -> Diagnostic {
    let severity = match d.severity {
        animatix_syntax::diagnostics::DiagnosticSeverity::Error => DiagnosticSeverity::Error,
        animatix_syntax::diagnostics::DiagnosticSeverity::Warning => DiagnosticSeverity::Warning,
        animatix_syntax::diagnostics::DiagnosticSeverity::Info => DiagnosticSeverity::Info,
        animatix_syntax::diagnostics::DiagnosticSeverity::Hint => DiagnosticSeverity::Hint,
    };
    let (line, col, end_line, end_col) = match &d.location.span {
        // A span that runs past the end of `source` is stale (the file was
        // edited); fall back to the recorded line/column rather than panicking.
        Some(span) if span.end <= source.len() => {
            let s = Span::from_range(source, span.clone());
            (
                s.start_line.saturating_sub(1),
                s.start_col.saturating_sub(1),
                s.end_line.saturating_sub(1),
                s.end_col.saturating_sub(1),
            )
        },
        _ => {
            let line = d.location.line.unwrap_or(1).saturating_sub(1);
            let col = d.location.column.unwrap_or(1).saturating_sub(1);
            let end_line = d.location.end_line.map(|v| v.saturating_sub(1)).unwrap_or(line);
            let end_col = d.location.end_col.map(|v| v.saturating_sub(1)).unwrap_or(col + 1);
            (line, col, end_line, end_col)
        },
    };
    Diagnostic {
        severity,
        line,
        col,
        end_line,
        end_col,
        message: d.message,
        code: Some(d.code.to_string()),
        subject: d.location.subject,
    }
}

#[cfg(test)]
mod tests;
