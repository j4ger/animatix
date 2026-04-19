use std::fmt;
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
    UnsupportedMediaAssignment,
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
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
            DiagnosticCode::UnsupportedMediaAssignment => write!(f, "unsupported-media-assignment"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiagnosticLocation {
    pub path: Option<PathBuf>,
    pub subject: Option<String>,
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
