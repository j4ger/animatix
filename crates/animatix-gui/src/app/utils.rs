#![allow(dead_code)]

use egui::{Color32, Vec2};

use animatix::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase, diagnostics_summary_by_phase};

pub(super) fn action_button(ui: &mut egui::Ui, label: &str, primary: bool, on_click: impl FnOnce()) {
    let button = if primary {
        egui::Button::new(label).fill(Color32::from_rgb(84, 110, 255))
    } else {
        egui::Button::new(label)
    };

    if ui.add(button).clicked() {
        on_click();
    }
}

pub(super) fn badge(ui: &mut egui::Ui, label: &str, fill: Color32, text: Color32) {
    let badge_w = label.len() as f32 * 7.0 + 16.0;
    let badge_h = 20.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(badge_w, badge_h), egui::Sense::hover());
    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::new(11.0, egui::FontFamily::Proportional),
        text,
    );
}

pub(super) fn diagnostics_summary_color(diagnostics: &[Diagnostic]) -> Color32 {
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == animatix::diagnostics::DiagnosticSeverity::Error)
    {
        Color32::from_rgb(255, 136, 136)
    } else {
        Color32::from_rgb(255, 214, 102)
    }
}

pub(super) fn has_source_load_failure(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.phase == DiagnosticPhase::Parse
            && diagnostic.severity == animatix::diagnostics::DiagnosticSeverity::Error
            && (diagnostic.code == DiagnosticCode::SourceLoadFailure
                || diagnostic.code == DiagnosticCode::ParseError)
    })
}

pub(super) fn primary_diagnostic_phase(diagnostics: &[Diagnostic]) -> Option<DiagnosticPhase> {
    let summaries = diagnostics_summary_by_phase(diagnostics);

    summaries
        .iter()
        .find(|summary| summary.errors > 0)
        .or_else(|| summaries.first())
        .map(|summary| summary.phase)
}

/// Return a banner message for the first diagnostic.
///
/// Priority:
/// 1. First error message (any phase) — actual diagnostic text, truncated.
/// 2. First warning message (any phase) — actual diagnostic text, truncated.
/// 3. Static phase description as a last resort.
pub(super) fn diagnostics_banner_message(diagnostics: &[Diagnostic]) -> Option<String> {
    if diagnostics.is_empty() {
        return None;
    }

    // Show the first error or warning message directly, regardless of phase.
    let first_message = diagnostics
        .iter()
        .find(|d| d.severity == animatix::diagnostics::DiagnosticSeverity::Error)
        .or_else(|| diagnostics.first());

    if let Some(err) = first_message {
        let msg = &err.message;
        let first_line = msg.lines().next().unwrap_or(msg);
        if first_line.len() > 80 {
            return Some(format!("{}...", &first_line[..80]));
        }
        return Some(first_line.to_string());
    }

    None
}
