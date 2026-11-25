use animatix_syntax::diagnostics::{Diagnostic, DiagnosticPhase, diagnostics_summary_by_phase};

use crate::app::document::DocumentSession;
use crate::app::i18n::StatusText;
use crate::app::utils::has_source_load_failure;

pub(crate) fn opened_status(document: &DocumentSession) -> StatusText {
    let path = &document.file_path;
    let mut status = StatusText::message("gui-file-opened", [("path", path.display().to_string())]);
    if has_source_load_failure(&document.diagnostics) {
        status.append(" • ", StatusText::message("gui-file-parse-load", []));
    }
    append_diagnostics(&mut status, &document.diagnostics);
    status
}

pub(super) fn append_diagnostics(status: &mut StatusText, diagnostics: &[Diagnostic]) {
    for (index, summary) in diagnostics_summary_by_phase(diagnostics).into_iter().enumerate() {
        let id = match summary.phase {
            DiagnosticPhase::Parse => "gui-file-summary-parse",
            DiagnosticPhase::Build => "gui-file-summary-build",
            DiagnosticPhase::Render => "gui-file-summary-render",
        };
        status.append(
            if index == 0 { " • " } else { " | " },
            StatusText::message(
                id,
                [
                    ("errors", summary.errors.to_string()),
                    ("warnings", summary.warnings.to_string()),
                ],
            ),
        );
    }
}
