use std::fmt;
use std::ops::Range;
use std::path::PathBuf;

/// Severity level of a diagnostic message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// A non-fatal issue that should be reported but does not stop processing.
    Warning,
    /// A fatal issue that prevents successful completion of the current phase.
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

/// Phase of the pipeline that produced a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticPhase {
    /// The parsing phase.
    Parse,
    /// The build (compilation) phase.
    Build,
    /// The render phase.
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

/// Unique code identifying the kind of diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    /// Failed to load a source file.
    SourceLoadFailure,
    /// A syntax or parse error was encountered.
    ParseError,
    /// The render step failed.
    RenderFailure,
    /// A modifier key is not supported in the current context.
    UnsupportedModifierKey,
    /// An assignment property is not supported.
    UnsupportedAssignmentProperty,
    /// The target of an assignment is invalid.
    InvalidAssignmentTarget,
    /// A modifier value is invalid.
    InvalidModifierValue,
    /// A configuration value is invalid.
    InvalidConfigValue,
    /// Two modifier keys conflict with each other.
    ConflictingModifierKey,
    /// The referenced action is unknown.
    UnknownAction,
    /// The referenced colorscheme is unknown.
    UnknownColorscheme,
    /// The referenced color is unknown.
    UnknownColorReference,
    /// The target of an action is not supported.
    UnsupportedActionTarget,
    /// A statement inside a sequence is not supported.
    UnsupportedSequenceStatement,
    /// The target path could not be resolved.
    UnknownTargetPath,
    /// The lookup path could not be resolved.
    UnknownLookupPath,
    /// A statement inside a stagger is not supported.
    UnsupportedStaggerStatement,
    /// Failed to load a media asset.
    MediaLoadFailure,
    /// Layout size had to fall back to a default.
    LayoutSizeFallback,
    /// A media assignment is not supported.
    UnsupportedMediaAssignment,
    /// Colorscheme data is malformed or invalid.
    InvalidColorschemeData,
    /// A cycle was detected in colorscheme inheritance.
    ColorschemeInheritanceCycle,
    /// Evaluation of a module export failed.
    ModuleExportEvalError,
    /// A modifier failed to compile.
    ModifierCompilationError,
    /// Coordinate system friction: two position bindings conflict.
    ConflictingPositionBinding,
    /// Coordinate system friction: an offset was ignored.
    IgnoredOffset,
    /// Multi-scene composition: a scene name was used more than once.
    DuplicateSceneName,
    /// Multi-scene composition: the play target was not found.
    PlayTargetNotFound,
    /// Multi-scene composition: a play cycle was detected.
    PlayCycleDetected,
    /// Multi-scene composition: a scene has multiple `play` statements (only the first is used).
    MultiplePlayTargets,
    /// Multi-scene composition: a scene is unreachable (no `play` edge leads to it).
    OrphanScene,
    /// Scene persistence: `persist` was given a duration argument, which is ignored.
    PersistIgnoresDuration,
    /// Scene persistence: `persist` targets a layout-managed leaf child directly.
    PersistLayoutManagedChild,
    /// Scene persistence: `persist` is used in a single-scene file or the last scene (no successor).
    PersistTargetNotCarried,
    /// Scene persistence: a scene has multiple predecessors in the play graph.
    CarryAmbiguousPredecessor,
    /// Scene persistence: `persist` follows `remove` in the same scene.
    PersistAfterRemove,
    /// The plot function is invalid.
    InvalidPlotFunc,
    /// The actor type is unknown.
    UnknownActorType,
    /// A type mismatch was detected during type checking.
    TypeMismatch,
    /// A modifier failed at runtime during frame evaluation.
    ModifierRuntimeError,
    /// A component instance property does not match any defined parameter.
    UnknownComponentProperty,
    /// An `always` block writes to a property that also has keyframes.
    AlwaysOverridesKeyframes,
    /// An `at` or `position` property was set on a layout-managed child.
    AbsolutePositionOnLayoutManagedChild,
    /// A deprecated primitive was used; the user should migrate to the replacement.
    DeprecatedPrimitive,
    /// A property value is invalid for the expected type or shape.
    InvalidPropertyValue,
    /// Actor label uses reserved `__` prefix.
    ReservedLabelPrefix,
    /// A property was placed inside braces without a preceding actor, so it was dropped.
    BracedPropertySilentDrop,
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
            DiagnosticCode::InvalidAssignmentTarget => {
                write!(f, "invalid-assignment-target")
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
            DiagnosticCode::InvalidColorschemeData => write!(f, "invalid-colorscheme-data"),
            DiagnosticCode::ColorschemeInheritanceCycle => write!(f, "colorscheme-inheritance-cycle"),
            DiagnosticCode::ModuleExportEvalError => write!(f, "module-export-eval-error"),
            DiagnosticCode::ModifierCompilationError => write!(f, "modifier-compilation-error"),
            DiagnosticCode::ConflictingPositionBinding => {
                write!(f, "conflicting-position-binding")
            }
            DiagnosticCode::IgnoredOffset => write!(f, "ignored-offset"),
            DiagnosticCode::DuplicateSceneName => write!(f, "duplicate-scene-name"),
            DiagnosticCode::PlayTargetNotFound => write!(f, "play-target-not-found"),
            DiagnosticCode::PlayCycleDetected => write!(f, "play-cycle-detected"),
            DiagnosticCode::MultiplePlayTargets => write!(f, "multiple-play-targets"),
            DiagnosticCode::OrphanScene => write!(f, "orphan-scene"),
            DiagnosticCode::PersistIgnoresDuration => write!(f, "persist-ignores-duration"),
            DiagnosticCode::PersistLayoutManagedChild => write!(f, "persist-layout-managed-child"),
            DiagnosticCode::PersistTargetNotCarried => write!(f, "persist-target-not-carried"),
            DiagnosticCode::CarryAmbiguousPredecessor => write!(f, "carry-ambiguous-predecessor"),
            DiagnosticCode::PersistAfterRemove => write!(f, "persist-after-remove"),
            DiagnosticCode::InvalidPlotFunc => write!(f, "invalid-plot-func"),
            DiagnosticCode::UnknownActorType => write!(f, "unknown-actor-type"),
            DiagnosticCode::TypeMismatch => write!(f, "type-mismatch"),
            DiagnosticCode::ModifierRuntimeError => write!(f, "modifier-runtime-error"),
            DiagnosticCode::UnknownComponentProperty => write!(f, "unknown-component-property"),
            DiagnosticCode::AlwaysOverridesKeyframes => {
                write!(f, "always-overrides-keyframes")
            }
            DiagnosticCode::AbsolutePositionOnLayoutManagedChild => {
                write!(f, "absolute-position-on-layout-managed-child")
            }
            DiagnosticCode::DeprecatedPrimitive => write!(f, "deprecated-primitive"),
            DiagnosticCode::InvalidPropertyValue => write!(f, "invalid-property-value"),
            DiagnosticCode::ReservedLabelPrefix => write!(f, "reserved-label-prefix"),
            DiagnosticCode::BracedPropertySilentDrop => {
                write!(f, "braced-property-silent-drop")
            }
        }
    }
}

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
    /// for multi-byte UTF-8 characters. Use [`Span::from_range`] which
    /// performs this conversion correctly.
    pub column: Option<usize>,
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
            let key = (
                d.code,
                d.message.clone(),
                d.location.subject.clone(),
            );
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
        format!(
            "{}: {}",
            self.phase,
            severity_summary(self.warnings, self.errors)
        )
    }
}

/// Formats a single diagnostic into a rustc-style human-readable string.
///
/// Format:
/// ```text
/// [severity:code] message
///  --> path:line:col
/// ```
/// With indented continuation lines for subject when present.
pub fn format_diagnostic(diagnostic: &Diagnostic) -> String {
    let mut lines = Vec::new();

    // Primary line: severity + code + message
    lines.push(format!(
        "{}[{}:{}] {}",
        diagnostic.severity, diagnostic.phase, diagnostic.code, diagnostic.message
    ));

    // Location line
    let location = match (
        diagnostic.location.path.as_ref(),
        diagnostic.location.line,
        diagnostic.location.column,
    ) {
        (Some(path), Some(line), Some(col)) => {
            format!(" --> {}:{}:{}", path.display(), line, col)
        }
        (Some(path), None, None) => format!(" --> {}", path.display()),
        (None, Some(line), Some(col)) => format!(" --> {}:{}", line, col),
        _ => String::new(),
    };
    if !location.is_empty() {
        lines.push(location);
    }

    // Subject line
    if let Some(subject) = &diagnostic.location.subject {
        lines.push(format!("  subject: {subject}"));
    }

    lines.join("\n")
}

/// Formats a diagnostic with a source snippet when a byte span is available.
///
/// If the diagnostic has `location.span`, extracts the offending source line
/// and prints it with a `^` underline pointing to the exact token.
pub fn format_diagnostic_with_source(diagnostic: &Diagnostic, source: &str) -> String {
    let mut output = format_diagnostic(diagnostic);

    if let Some(span) = &diagnostic.location.span {
        if let Some((line_text, caret_offset, caret_len)) = extract_source_snippet(source, span) {
            output.push('\n');
            output.push_str(&line_text);
            output.push('\n');
            output.push_str(&" ".repeat(caret_offset));
            output.push_str(&"^".repeat(caret_len.max(1)));
        }
    }

    output
}

/// Extract the source line containing `span` and the caret position within it.
fn extract_source_snippet(
    source: &str,
    span: &Range<usize>,
) -> Option<(String, usize, usize)> {
    let start = span.start;
    let end = span.end;
    if start > source.len() || end > source.len() {
        return None;
    }

    // Find the start of the line containing `start`
    let line_start = source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    // Find the end of the line containing `end`
    let line_end = source[end..]
        .find('\n')
        .map(|i| end + i)
        .unwrap_or(source.len());

    let line_text = source[line_start..line_end].to_string();
    let caret_offset = start - line_start;
    let caret_len = end.saturating_sub(start);

    Some((line_text, caret_offset, caret_len))
}

/// Returns a summary string counting total warnings and errors.
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

/// Returns a per-phase summary of diagnostics, omitting phases with no issues.
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

/// Returns a formatted summary string of diagnostics grouped by phase.
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
        .with_path("test/file.amx");

        let formatted = format_diagnostic(&diagnostic);

        assert!(
            formatted.contains("error[parse:source-load-failure] Failed to load or parse source")
        );
        assert!(formatted.contains(" --> test/file.amx"));
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

    #[test]
    fn build_report_deduplicates_identical_diagnostics() {
        let diagnostics = vec![
            Diagnostic::warning(
                DiagnosticCode::UnknownAction,
                DiagnosticPhase::Build,
                "Unknown action 'spin'",
            )
            .with_subject("spin"),
            Diagnostic::warning(
                DiagnosticCode::UnknownAction,
                DiagnosticPhase::Build,
                "Unknown action 'spin'",
            )
            .with_subject("spin"),
            Diagnostic::warning(
                DiagnosticCode::UnknownAction,
                DiagnosticPhase::Build,
                "Unknown action 'pulse'",
            )
            .with_subject("pulse"),
        ];

        let report: BuildReport<()> = BuildReport::new((), diagnostics);
        assert_eq!(report.diagnostics.len(), 2);
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.message == "Unknown action 'spin'"));
        assert!(report
            .diagnostics
            .iter()
            .any(|d| d.message == "Unknown action 'pulse'"));
    }

    #[test]
    fn build_report_keeps_different_subjects() {
        let diagnostics = vec![
            Diagnostic::warning(
                DiagnosticCode::ConflictingPositionBinding,
                DiagnosticPhase::Build,
                "`at` and `anchor` both specify position for 'a'",
            )
            .with_subject("a"),
            Diagnostic::warning(
                DiagnosticCode::ConflictingPositionBinding,
                DiagnosticPhase::Build,
                "`at` and `anchor` both specify position for 'b'",
            )
            .with_subject("b"),
        ];

        let report: BuildReport<()> = BuildReport::new((), diagnostics);
        assert_eq!(report.diagnostics.len(), 2);
    }

    #[test]
    fn persist_ignores_duration_code_formats_honestly() {
        assert_eq!(
            DiagnosticCode::PersistIgnoresDuration.to_string(),
            "persist-ignores-duration"
        );
    }

    #[test]
    fn persist_layout_managed_child_code_formats_honestly() {
        assert_eq!(
            DiagnosticCode::PersistLayoutManagedChild.to_string(),
            "persist-layout-managed-child"
        );
    }

    #[test]
    fn persist_target_not_carried_code_formats_honestly() {
        assert_eq!(
            DiagnosticCode::PersistTargetNotCarried.to_string(),
            "persist-target-not-carried"
        );
    }

    #[test]
    fn carry_ambiguous_predecessor_code_formats_honestly() {
        assert_eq!(
            DiagnosticCode::CarryAmbiguousPredecessor.to_string(),
            "carry-ambiguous-predecessor"
        );
    }

    #[test]
    fn persist_after_remove_code_formats_honestly() {
        assert_eq!(
            DiagnosticCode::PersistAfterRemove.to_string(),
            "persist-after-remove"
        );
    }

    #[test]
    fn persist_ignores_duration_diagnostic_is_warning() {
        let diagnostic = Diagnostic::warning(
            DiagnosticCode::PersistIgnoresDuration,
            DiagnosticPhase::Build,
            "Persist ignores duration; duration value will be ignored",
        );
        assert!(diagnostic.severity == DiagnosticSeverity::Warning);
    }

    #[test]
    fn persist_layout_managed_child_diagnostic_is_warning() {
        let diagnostic = Diagnostic::warning(
            DiagnosticCode::PersistLayoutManagedChild,
            DiagnosticPhase::Build,
            "Persist layout-managed leaf child",
        );
        assert!(diagnostic.severity == DiagnosticSeverity::Warning);
    }

    #[test]
    fn persist_target_not_carried_diagnostic_is_warning() {
        let diagnostic = Diagnostic::warning(
            DiagnosticCode::PersistTargetNotCarried,
            DiagnosticPhase::Build,
            "Persist target not carried to any scene",
        );
        assert!(diagnostic.severity == DiagnosticSeverity::Warning);
        assert!(diagnostic.code == DiagnosticCode::PersistTargetNotCarried);
    }

    #[test]
    fn carry_ambiguous_predecessor_diagnostic_is_warning() {
        let diagnostic = Diagnostic::warning(
            DiagnosticCode::CarryAmbiguousPredecessor,
            DiagnosticPhase::Build,
            "Scene has multiple predecessors in play graph",
        );
        assert!(diagnostic.severity == DiagnosticSeverity::Warning);
        assert!(diagnostic.code == DiagnosticCode::CarryAmbiguousPredecessor);
    }

    #[test]
    fn persist_after_remove_diagnostic_is_warning() {
        let diagnostic = Diagnostic::warning(
            DiagnosticCode::PersistAfterRemove,
            DiagnosticPhase::Build,
            "Persist follows remove in the same scene",
        );
        assert!(diagnostic.severity == DiagnosticSeverity::Warning);
        assert!(diagnostic.code == DiagnosticCode::PersistAfterRemove);
    }
}
