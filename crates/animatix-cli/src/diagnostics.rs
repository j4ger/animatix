use animatix_syntax::diagnostics::{
    Diagnostic, DiagnosticCode, DiagnosticPhase, format_diagnostic_with_source,
};
use clap::ValueEnum;

/// Output format for diagnostic reporting.
#[derive(Clone, Copy, ValueEnum, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable text output
    Text,
    /// Structured JSON output
    Json,
}

/// Turn a module-load failure into structured diagnostics so `check`/`lint`
/// report parse errors through the same stdout channel as semantic
/// diagnostics. Previously these printed only via `tracing` (stderr), which
/// meant `--format json` emitted nothing at all for an unparseable file.
///
/// `source` is the entry file's text, used to resolve each parse error's byte
/// span into an end position so JSON consumers get a full range.
pub fn module_error_diagnostics(
    err: &animatix_syntax::module::ModuleError,
    source: &str,
) -> Vec<Diagnostic> {
    use animatix_syntax::module::ModuleError;

    match err {
        ModuleError::ParseErrors(errors) => errors
            .iter()
            .map(|error| {
                let diagnostic = error.to_diagnostic();
                // The error carries a byte span; the end line/column is only
                // knowable with the source in hand.
                let span = error.span.clone();
                let end = animatix_syntax::ast::Span::from_range(source, span);
                diagnostic.with_end_location(end.end_line, end.end_col)
            })
            .collect(),
        other => vec![Diagnostic::error(
            DiagnosticCode::ParseError,
            DiagnosticPhase::Parse,
            other.to_string(),
        )],
    }
}

/// Print a `check` report for one file and exit 1 when any error is present.
///
/// Both diagnostic families share this path: `build` diagnostics (from
/// `ModuleGraph`/build, pretty-printed against the source) and analyzer
/// `semantic` diagnostics. JSON output puts them in one `errors` array so a
/// CI consumer sees a complete report on stdout.
pub fn emit_check_report(
    file_label: &str,
    source: &str,
    diagnostics: Vec<Diagnostic>,
    semantic: Vec<animatix_analyzer::Diagnostic>,
    format: &OutputFormat,
) {
    let has_error =
        diagnostics.iter().any(|d| d.is_error()) || semantic.iter().any(|d| d.is_error());

    match format {
        OutputFormat::Json => {
            if diagnostics.is_empty() && semantic.is_empty() {
                println!(r#"{{"passed":true}}"#);
            } else {
                // `passed` mirrors the exit code: warnings are reported but do
                // not fail the run, so CI consumers never see passed:false
                // alongside exit 0.
                let errors: Vec<serde_json::Value> = diagnostics
                    .iter()
                    .map(diagnostic_to_json)
                    .chain(semantic.iter().map(semantic_diagnostic_to_json))
                    .collect();
                let report = serde_json::json!({
                    "passed": !has_error,
                    "errors": errors,
                });
                println!("{report}");
            }
        },
        OutputFormat::Text => {
            if diagnostics.is_empty() && semantic.is_empty() {
                println!("{file_label}: OK (no diagnostics)");
            } else {
                for diag in &diagnostics {
                    println!("{}", format_diagnostic_with_source(diag, source));
                }
                let (shown, folded) = fold_hints(&semantic);
                for diag in shown {
                    println!("{}:{}", file_label, diag);
                }
                if folded > 0 {
                    println!("{file_label}: {folded} hint(s) folded — --format json lists them");
                }
            }
        },
    }

    if has_error {
        std::process::exit(1);
    }
}

/// Split semantic diagnostics into the ones a text report should print, and how
/// many hints it folded away.
///
/// A hint is an observation about source that is usually deliberate — an
/// unreferenced actor still renders every frame — and this repo's own corpus
/// carries 347 of them, which was enough to make the warning column unreadable
/// (see `docs/roadmap.md`, "Easing pass"). They are folded from the text
/// report rather than deleted: the count is stated, and `--format json` keeps
/// every one.
pub fn fold_hints(
    diagnostics: &[animatix_analyzer::Diagnostic],
) -> (Vec<&animatix_analyzer::Diagnostic>, usize) {
    let shown: Vec<&animatix_analyzer::Diagnostic> = diagnostics
        .iter()
        .filter(|d| d.severity != animatix_analyzer::DiagnosticSeverity::Hint)
        .collect();
    (shown.clone(), diagnostics.len() - shown.len())
}

/// Anchor build diagnostics that name an actor but carry no position.
///
/// `never-revealed` and friends are produced while walking the runtime track
/// table, which keeps no source spans, so they reach the CLI with no location.
/// Their message quotes the actor label in backticks; when the analyzer can
/// resolve that label to a declaration, the diagnostic is pointed there.
pub fn resolve_build_diagnostic_positions(
    diagnostics: &mut [Diagnostic],
    analyzer: &animatix_analyzer::Analyzer,
) {
    for diagnostic in diagnostics.iter_mut() {
        if diagnostic.location.line.is_some() {
            continue;
        }
        let Some(label) = backticked_label(&diagnostic.message) else {
            continue;
        };
        let Some((line, col, end_line, end_col)) = analyzer.declaration_range(&label) else {
            continue;
        };
        // Diagnostics are 1-based on this side; the analyzer reports 0-based.
        diagnostic.location.line = Some(line + 1);
        diagnostic.location.column = Some(col + 1);
        diagnostic.location.end_line = Some(end_line + 1);
        diagnostic.location.end_col = Some(end_col + 1);
    }
}

/// Extract the first `` `label` `` from a diagnostic message.
pub fn backticked_label(message: &str) -> Option<String> {
    let start = message.find('`')? + 1;
    let rest = &message[start..];
    let end = rest.find('`')?;
    let label = &rest[..end];
    (!label.is_empty() && !label.contains(' ')).then(|| label.to_string())
}

/// Serializes a single build diagnostic as a JSON value.
///
/// Both diagnostic families go through this shape so a consumer parses one
/// schema: the same keys are present whether the diagnostic came from the
/// build or from the analyzer, with `phase` naming which.
pub fn diagnostic_to_json(d: &Diagnostic) -> serde_json::Value {
    serde_json::json!({
        "line": d.location.line,
        "col": d.location.column,
        "end_line": d.location.end_line,
        "end_col": d.location.end_col,
        "message": d.message,
        "code": d.code.to_string(),
        "severity": d.severity.to_string(),
        "phase": d.phase.to_string(),
    })
}

/// Serializes a single analyzer (semantic) diagnostic as a JSON value.
///
/// Uses the same keys as [`diagnostic_to_json`]; the analyzer's checks run
/// alongside the build, which is what `phase` reports.
pub fn semantic_diagnostic_to_json(d: &animatix_analyzer::Diagnostic) -> serde_json::Value {
    serde_json::json!({
        "line": d.line,
        "col": d.col,
        "end_line": d.end_line,
        "end_col": d.end_col,
        "message": d.message,
        "code": d.code,
        "severity": format!("{:?}", d.severity).to_lowercase(),
        "phase": "build",
    })
}
