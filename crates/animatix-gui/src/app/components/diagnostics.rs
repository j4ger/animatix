use animatix_syntax::diagnostics::Diagnostic;
use eparts::widget::diagnostics::{DiagnosticEntry, DiagnosticTarget};

use crate::app::design_tokens::semantic::diagnostic as diag;

/// Thin wrapper around `animatix_syntax::diagnostics::Diagnostic` so we can
/// implement the foreign `eparts::DiagnosticEntry` trait without violating
/// the orphan rule (the trait is foreign to this crate, but the wrapper
/// type is local).
pub struct AnimDiagnostic<'a>(pub &'a Diagnostic);

impl<'a> DiagnosticEntry for AnimDiagnostic<'a> {
    fn is_error(&self) -> bool {
        self.0.is_error()
    }

    fn message(&self) -> &str {
        &self.0.message
    }

    fn line(&self) -> Option<usize> {
        self.0.location.line
    }

    fn column(&self) -> Option<usize> {
        self.0.location.column
    }

    fn phase_label(&self) -> Option<&'static str> {
        Some(match self.0.phase {
            animatix_syntax::diagnostics::DiagnosticPhase::Parse => "parse",
            animatix_syntax::diagnostics::DiagnosticPhase::Build => "build",
            animatix_syntax::diagnostics::DiagnosticPhase::Render => "render",
        })
    }

    fn phase_color(&self) -> Option<egui::Color32> {
        Some(match self.0.phase {
            animatix_syntax::diagnostics::DiagnosticPhase::Parse => diag::PHASE_PARSE,
            animatix_syntax::diagnostics::DiagnosticPhase::Build => diag::PHASE_RESOLVE,
            animatix_syntax::diagnostics::DiagnosticPhase::Render => diag::PHASE_COMPILE,
        })
    }
}

/// Call-site-compatible shim. Preserves the original signature so all existing
/// callers resolve without changes.
pub fn diagnostics_list(
    ui: &mut egui::Ui,
    diagnostics: &[Diagnostic],
    visible: &mut bool,
) -> Option<DiagnosticTarget> {
    let wrapped: Vec<AnimDiagnostic<'_>> = diagnostics.iter().map(AnimDiagnostic).collect();
    eparts::widget::diagnostics::diagnostics_list(ui, &wrapped, visible)
}
