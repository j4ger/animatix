use super::*;

/// Formats a single diagnostic into a rustc-style human-readable string.
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
        },
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
fn extract_source_snippet(source: &str, span: &Range<usize>) -> Option<(String, usize, usize)> {
    // Spans are raw byte offsets and can end inside a multi-byte character
    // (an em-dash in a comment is enough); snap both ends to char boundaries
    // so the slicing below cannot panic.
    if span.start > source.len() || span.end > source.len() {
        return None;
    }
    let mut start = span.start;
    while start > 0 && !source.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = span.end.max(start);
    while end < source.len() && !source.is_char_boundary(end) {
        end += 1;
    }

    // Find the start of the line containing `start`
    let line_start = source[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    // Find the end of the line containing `end`
    let line_end = source[end..].find('\n').map(|i| end + i).unwrap_or(source.len());

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

pub(super) fn severity_summary(warnings: usize, errors: usize) -> String {
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
