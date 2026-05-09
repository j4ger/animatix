use std::fmt;
use std::ops::Range;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Warning,
    Error,
}

impl fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticSeverity::Warning => write!(f, "warning"),
            DiagnosticSeverity::Error => write!(f, "error"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticPhase {
    Parse,
    Build,
    Render,
}

impl fmt::Display for DiagnosticPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticPhase::Parse => write!(f, "parse"),
            DiagnosticPhase::Build => write!(f, "build"),
            DiagnosticPhase::Render => write!(f, "render"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticCode {
    SourceLoadFailure,
    ParseError,
    RenderFailure,
    UnsupportedModifierKey,
    UnsupportedAssignmentProperty,
    InvalidModifierValue,
    InvalidConfigValue,
    ConflictingModifierKey,
    UnknownAction,
    UnknownColorscheme,
    UnknownColorReference,
    UnsupportedActionTarget,
    UnsupportedSequenceStatement,
    UnknownTargetPath,
    UnknownLookupPath,
    UnsupportedStaggerStatement,
    MediaLoadFailure,
    LayoutSizeFallback,
    UnsupportedMediaAssignment,
    ColorschemeLoadFailure,
    InvalidColorschemeData,
    ColorschemeInheritanceCycle,
    EmptyAutoColorPool,
    ModuleExportEvalError,
    ModifierCompilationError,
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticCode::SourceLoadFailure => write!(f, "source-load-failure"),
            DiagnosticCode::ParseError => write!(f, "parse-error"),
            DiagnosticCode::RenderFailure => write!(f, "render-failure"),
            DiagnosticCode::UnsupportedModifierKey => write!(f, "unsupported-modifier-key"),
            DiagnosticCode::UnsupportedAssignmentProperty => {
                write!(f, "unsupported-assignment-property")
            }
            DiagnosticCode::InvalidModifierValue => write!(f, "invalid-modifier-value"),
            DiagnosticCode::InvalidConfigValue => write!(f, "invalid-config-value"),
            DiagnosticCode::ConflictingModifierKey => write!(f, "conflicting-modifier-key"),
            DiagnosticCode::UnknownAction => write!(f, "unknown-action"),
            DiagnosticCode::UnknownColorscheme => write!(f, "unknown-colorscheme"),
            DiagnosticCode::UnknownColorReference => write!(f, "unknown-color-reference"),
            DiagnosticCode::UnsupportedActionTarget => write!(f, "unsupported-action-target"),
            DiagnosticCode::UnsupportedSequenceStatement => {
                write!(f, "unsupported-sequence-statement")
            }
            DiagnosticCode::UnknownTargetPath => write!(f, "unknown-target-path"),
            DiagnosticCode::UnknownLookupPath => write!(f, "unknown-lookup-path"),
            DiagnosticCode::UnsupportedStaggerStatement => {
                write!(f, "unsupported-stagger-statement")
            }
            DiagnosticCode::MediaLoadFailure => write!(f, "media-load-failure"),
            DiagnosticCode::LayoutSizeFallback => write!(f, "layout-size-fallback"),
            DiagnosticCode::UnsupportedMediaAssignment => write!(f, "unsupported-media-assignment"),
            DiagnosticCode::ColorschemeLoadFailure => write!(f, "colorscheme-load-failure"),
            DiagnosticCode::InvalidColorschemeData => write!(f, "invalid-colorscheme-data"),
            DiagnosticCode::ColorschemeInheritanceCycle => write!(f, "colorscheme-inheritance-cycle"),
            DiagnosticCode::EmptyAutoColorPool => write!(f, "empty-auto-color-pool"),
            DiagnosticCode::ModuleExportEvalError => write!(f, "module-export-eval-error"),
            DiagnosticCode::ModifierCompilationError => write!(f, "modifier-compilation-error"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticLocation {
    pub path: Option<PathBuf>,
    pub subject: Option<String>,
    /// 1-based line number where the error occurs.
    pub line: Option<usize>,
    /// 1-based column number where the error occurs.
    pub column: Option<usize>,
    /// Byte-offset range into the source text.
    pub span: Option<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub phase: DiagnosticPhase,
    pub code: DiagnosticCode,
    pub message: String,
    pub location: DiagnosticLocation,
}

impl Diagnostic {
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

    pub fn error(code: DiagnosticCode, phase: DiagnosticPhase, message: impl Into<String>) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            phase,
            code,
            message: message.into(),
            location: DiagnosticLocation::default(),
        }
    }

    pub fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.location.path = Some(path.into());
        self
    }

    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.location.subject = Some(subject.into());
        self
    }

    pub fn with_location(mut self, line: usize, column: usize, span: Range<usize>) -> Self {
        self.location.line = Some(line);
        self.location.column = Some(column);
        self.location.span = Some(span);
        self
    }

    /// Returns true if this diagnostic represents an error (not a warning).
    pub fn is_error(&self) -> bool {
        self.severity == DiagnosticSeverity::Error
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildReport<T> {
    pub output: T,
    pub diagnostics: Vec<Diagnostic>,
}

impl<T> BuildReport<T> {
    pub fn new(output: T, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            output,
            diagnostics,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticPhaseSummary {
    pub phase: DiagnosticPhase,
    pub warnings: usize,
    pub errors: usize,
}

impl DiagnosticPhaseSummary {
    pub fn total(&self) -> usize {
        self.warnings + self.errors
    }

    pub fn label(&self) -> String {
        format!(
            "{}: {}",
            self.phase,
            severity_summary(self.warnings, self.errors)
        )
    }
}

pub fn format_diagnostic(diagnostic: &Diagnostic) -> String {
    let mut parts = vec![format!(
        "{}[{}:{}] {}",
        diagnostic.severity, diagnostic.phase, diagnostic.code, diagnostic.message
    )];

    if let Some(subject) = &diagnostic.location.subject {
        parts.push(format!("subject: {subject}"));
    }

    if let Some(path) = &diagnostic.location.path {
        parts.push(format!("path: {}", path.display()));
    }

    parts.join(" • ")
}

pub fn diagnostics_summary(diagnostics: &[Diagnostic]) -> String {
    let warnings = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning)
        .count();
    let errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
        .count();

    severity_summary(warnings, errors)
}

pub fn diagnostics_summary_by_phase(diagnostics: &[Diagnostic]) -> Vec<DiagnosticPhaseSummary> {
    [
        DiagnosticPhase::Parse,
        DiagnosticPhase::Build,
        DiagnosticPhase::Render,
    ]
    .into_iter()
    .filter_map(|phase| {
        let warnings = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.phase == phase && diagnostic.severity == DiagnosticSeverity::Warning
            })
            .count();
        let errors = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.phase == phase && diagnostic.severity == DiagnosticSeverity::Error
            })
            .count();

        let summary = DiagnosticPhaseSummary {
            phase,
            warnings,
            errors,
        };
        (summary.total() > 0).then_some(summary)
    })
    .collect()
}

pub fn diagnostics_phase_summary(diagnostics: &[Diagnostic]) -> String {
    let summaries = diagnostics_summary_by_phase(diagnostics);

    if summaries.is_empty() {
        return "No diagnostics".to_string();
    }

    summaries
        .into_iter()
        .map(|summary| summary.label())
        .collect::<Vec<_>>()
        .join(" | ")
}

fn severity_summary(warnings: usize, errors: usize) -> String {
    match (warnings, errors) {
        (0, 0) => "No diagnostics".to_string(),
        (warnings, 0) => format!("{warnings} warning{}", if warnings == 1 { "" } else { "s" }),
        (0, errors) => format!("{errors} error{}", if errors == 1 { "" } else { "s" }),
        (warnings, errors) => format!(
            "{warnings} warning{} and {errors} error{}",
            if warnings == 1 { "" } else { "s" },
            if errors == 1 { "" } else { "s" }
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_load_failure_code_formats_honestly() {
        assert_eq!(
            DiagnosticCode::SourceLoadFailure.to_string(),
            "source-load-failure"
        );
    }

    #[test]
    fn render_failure_code_formats_honestly() {
        assert_eq!(DiagnosticCode::RenderFailure.to_string(), "render-failure");
    }

    #[test]
    fn format_diagnostic_includes_parse_source_load_failure() {
        let diagnostic = Diagnostic::error(
            DiagnosticCode::SourceLoadFailure,
            DiagnosticPhase::Parse,
            "Failed to load or parse source",
        )
        .with_path("examples/showcase.amx");

        let formatted = format_diagnostic(&diagnostic);

        assert!(
            formatted.contains("error[parse:source-load-failure] Failed to load or parse source")
        );
        assert!(formatted.contains("path: examples/showcase.amx"));
    }

    #[test]
    fn diagnostics_summary_by_phase_counts_mixed_diagnostics() {
        let diagnostics = vec![
            Diagnostic::error(
                DiagnosticCode::SourceLoadFailure,
                DiagnosticPhase::Parse,
                "parse failed",
            ),
            Diagnostic::warning(
                DiagnosticCode::UnsupportedModifierKey,
                DiagnosticPhase::Build,
                "unsupported modifier",
            ),
            Diagnostic::error(
                DiagnosticCode::UnknownAction,
                DiagnosticPhase::Build,
                "unknown action",
            ),
            Diagnostic::warning(
                DiagnosticCode::MediaLoadFailure,
                DiagnosticPhase::Render,
                "preview issue",
            ),
        ];

        let summaries = diagnostics_summary_by_phase(&diagnostics);

        assert_eq!(
            summaries,
            vec![
                DiagnosticPhaseSummary {
                    phase: DiagnosticPhase::Parse,
                    warnings: 0,
                    errors: 1,
                },
                DiagnosticPhaseSummary {
                    phase: DiagnosticPhase::Build,
                    warnings: 1,
                    errors: 1,
                },
                DiagnosticPhaseSummary {
                    phase: DiagnosticPhase::Render,
                    warnings: 1,
                    errors: 0,
                },
            ]
        );
    }

    #[test]
    fn diagnostics_phase_summary_formats_present_phases_only() {
        let diagnostics = vec![
            Diagnostic::error(
                DiagnosticCode::SourceLoadFailure,
                DiagnosticPhase::Parse,
                "parse failed",
            ),
            Diagnostic::warning(
                DiagnosticCode::UnknownTargetPath,
                DiagnosticPhase::Build,
                "missing target",
            ),
        ];

        assert_eq!(
            diagnostics_phase_summary(&diagnostics),
            "parse: 1 error | build: 1 warning"
        );
    }

    #[test]
    fn diagnostics_phase_summary_handles_empty_input() {
        assert_eq!(diagnostics_phase_summary(&[]), "No diagnostics");
    }
}
