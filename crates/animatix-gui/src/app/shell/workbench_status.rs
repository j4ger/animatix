//! Keep source truth and last-successful preview provenance visible.
use crate::app::GuiShell;
use crate::app::i18n::{Language, current_language, text_for, with_arg};

impl GuiShell {
    pub(crate) fn workbench_status_ui(&self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let pending = self.preview_store.has_pending_rebuild()
            || self.preview_store.rebuild_in_progress
            || self.preview_store.in_flight_rebuild.is_some();
        let failed = self.document_store.source.document.last_rebuild_error.is_some();
        let stale = self.document_store.snapshot_is_stale();
        let generation = self.document_store.last_good_snapshot().map(|s| s.generation.0);
        let narration_pending = self.document_store.source.document.diagnostics.iter().any(|d| {
            d.code == animatix_syntax::diagnostics::DiagnosticCode::NarrationBindingUnresolved
        });
        let (label, detail, warning) = preview_provenance(
            current_language(ui.ctx()),
            pending,
            failed,
            stale,
            generation,
            narration_pending,
        );
        let color = if warning {
            theme.palette.status.warning
        } else {
            theme.palette.status.success
        };
        let (indicator, _) = ui.allocate_exact_size(egui::Vec2::splat(6.0), egui::Sense::hover());
        ui.painter().circle_filled(indicator.center(), 3.0, color);
        ui.label(
            egui::RichText::new(label)
                .font(crate::app::design_tokens::typography::TextRole::BodyS.font_id())
                .color(theme.palette.text.primary),
        )
        .on_hover_text(detail);
    }
}

fn preview_provenance(
    language: Language,
    pending: bool,
    failed: bool,
    stale: bool,
    generation: Option<u64>,
    narration_pending: bool,
) -> (String, String, bool) {
    let build = generation
        .map(|n| with_arg(language, "gui-status-build-number", "number", n.to_string()))
        .unwrap_or_else(|| text_for(language, "gui-status-never-built"));
    if pending {
        (
            with_arg(language, "gui-status-preview-building", "build", build),
            text_for(language, "gui-status-preview-building-help"),
            true,
        )
    } else if failed {
        (
            with_arg(language, "gui-status-preview-build-failed", "build", build),
            text_for(language, "gui-status-preview-failed-help"),
            true,
        )
    } else if stale {
        (
            with_arg(language, "gui-status-preview-stale", "build", build),
            text_for(language, "gui-status-preview-stale-help"),
            true,
        )
    } else if generation.is_none() {
        (
            text_for(language, "gui-status-preview-empty"),
            text_for(language, "gui-status-preview-empty-help"),
            true,
        )
    } else if narration_pending {
        (
            text_for(language, "gui-status-preview-narration"),
            text_for(language, "gui-status-preview-narration-help"),
            true,
        )
    } else {
        (
            text_for(language, "gui-status-preview-current"),
            text_for(language, "gui-status-preview-current-help"),
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Language, preview_provenance};
    #[test]
    fn pending_and_failed_never_claim_current() {
        assert!(
            preview_provenance(Language::English, true, false, false, Some(2), false)
                .0
                .starts_with("Building")
        );
        assert!(
            preview_provenance(Language::English, false, true, false, Some(2), false)
                .0
                .starts_with("Build failed")
        );
        assert!(preview_provenance(Language::English, false, false, true, Some(2), false).2);
        assert!(preview_provenance(Language::English, false, false, false, None, false).2);
        assert!(preview_provenance(Language::English, false, false, false, Some(2), true).2);
        assert!(!preview_provenance(Language::English, false, false, false, Some(2), false).2);
    }
}
