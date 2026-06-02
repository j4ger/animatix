
pub mod labels;

use egui::{Color32, Vec2};

use crate::app::design_tokens::*;
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
#[cfg(test)]
use animatix_syntax::diagnostics::diagnostics_summary_by_phase;

/// Draw a badge with background and optional stroke at a specific position.
/// Returns the rectangle occupied by the badge.
pub(super) fn draw_badge(
    painter: &egui::Painter,
    pos: egui::Pos2,
    text: &str,
    bg: Color32,
    text_color: Color32,
    stroke: Option<egui::Stroke>,
) -> egui::Rect {
    let galley = painter.layout_no_wrap(
        text.to_string(),
        egui::FontId::proportional(FONT_SIZE_S),
        text_color,
    );
    let size = galley.size() + Vec2::new(PAD_L * 2.0, PAD_S * 2.0);
    let rect = egui::Rect::from_min_size(pos, size);
    painter.rect_filled(rect, RADIUS_S, bg);
    if let Some(s) = stroke {
        painter.rect_stroke(rect, RADIUS_S, s, egui::StrokeKind::Outside);
    }
    painter.galley(rect.min + Vec2::new(PAD_L, PAD_S), galley, text_color);
    rect
}

#[cfg(test)]
pub(super) fn diagnostics_summary_color(diagnostics: &[Diagnostic]) -> Color32 {
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
    {
        DIAGNOSTIC_RED
    } else {
        DIAGNOSTIC_AMBER
    }
}

pub(super) fn has_source_load_failure(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.phase == DiagnosticPhase::Parse
            && diagnostic.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error
            && (diagnostic.code == DiagnosticCode::SourceLoadFailure
                || diagnostic.code == DiagnosticCode::ParseError)
    })
}

#[cfg(test)]
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
#[cfg(test)]
pub(super) fn diagnostics_banner_message(diagnostics: &[Diagnostic]) -> Option<String> {
    if diagnostics.is_empty() {
        return None;
    }

    // Show the first error or warning message directly, regardless of phase.
    let first_message = diagnostics
        .iter()
        .find(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
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


