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
fn source_load_failure_code_formats_honestly() {
    assert_eq!(DiagnosticCode::SourceLoadFailure.to_string(), "source-load-failure");
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
fn persist_ignores_duration_code_formats_honestly() {
    assert_eq!(DiagnosticCode::PersistIgnoresDuration.to_string(), "persist-ignores-duration");
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
    assert_eq!(DiagnosticCode::PersistAfterRemove.to_string(), "persist-after-remove");
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
