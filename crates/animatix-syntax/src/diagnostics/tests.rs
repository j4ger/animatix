use super::*;

#[test]
fn source_snippet_snaps_spans_onto_char_boundaries() {
    // A span that ends inside the em-dash (raw byte offset) used to panic
    // the snippet extractor; it must render instead.
    let source = "// probe — comment with a dash\nr: Rect\n";
    let dash_start = source.find('—').unwrap();
    let mut diagnostic =
        Diagnostic::error(DiagnosticCode::ParseError, DiagnosticPhase::Parse, "bad");
    diagnostic.location.span = Some(dash_start + 1..dash_start + 3);
    let formatted = format_diagnostic_with_source(&diagnostic, source);
    assert!(formatted.contains("// probe — comment with a dash"));
}

#[test]
fn diagnostic_codes_format_honestly() {
    for (code, expected) in [
        (DiagnosticCode::SourceLoadFailure, "source-load-failure"),
        (DiagnosticCode::RenderFailure, "render-failure"),
        (DiagnosticCode::PersistIgnoresDuration, "persist-ignores-duration"),
        (DiagnosticCode::PersistLayoutManagedChild, "persist-layout-managed-child"),
        (DiagnosticCode::PersistTargetNotCarried, "persist-target-not-carried"),
        (DiagnosticCode::CarryAmbiguousPredecessor, "carry-ambiguous-predecessor"),
        (DiagnosticCode::PersistAfterRemove, "persist-after-remove"),
    ] {
        assert_eq!(code.to_string(), expected, "diagnostic code drifted");
    }
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

    assert!(formatted.contains("error[parse:source-load-failure] Failed to load or parse source"));
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
        Diagnostic::error(DiagnosticCode::UnknownAction, DiagnosticPhase::Build, "unknown action"),
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

    assert_eq!(diagnostics_phase_summary(&diagnostics), "parse: 1 error | build: 1 warning");
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
    assert!(report.diagnostics.iter().any(|d| d.message == "Unknown action 'spin'"));
    assert!(report.diagnostics.iter().any(|d| d.message == "Unknown action 'pulse'"));
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
fn persist_and_carry_diagnostics_are_warnings() {
    for (code, message) in [
        (
            DiagnosticCode::PersistIgnoresDuration,
            "Persist ignores duration; duration value will be ignored",
        ),
        (DiagnosticCode::PersistLayoutManagedChild, "Persist layout-managed leaf child"),
        (
            DiagnosticCode::PersistTargetNotCarried,
            "Persist target not carried to any scene",
        ),
        (
            DiagnosticCode::CarryAmbiguousPredecessor,
            "Scene has multiple predecessors in play graph",
        ),
        (DiagnosticCode::PersistAfterRemove, "Persist follows remove in the same scene"),
    ] {
        let diagnostic = Diagnostic::warning(code, DiagnosticPhase::Build, message);
        assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
        assert_eq!(diagnostic.code, code);
    }
}
