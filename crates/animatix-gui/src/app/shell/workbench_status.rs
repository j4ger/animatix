//! Keep source truth and last-successful preview provenance visible.
use crate::app::GuiShell;

impl GuiShell {
    pub(crate) fn workbench_status_ui(&self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let pending = self.preview_store.has_pending_rebuild()
            || self.preview_store.rebuild_in_progress
            || self.preview_store.in_flight_rebuild.is_some();
        let failed = self.document_store.source.document.last_rebuild_error.is_some();
        let stale = self.document_store.snapshot_is_stale();
        let generation = self.document_store.last_good_snapshot().map(|s| s.generation.0);
        let (label, detail, warning) = preview_provenance(pending, failed, stale, generation);
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
    pending: bool,
    failed: bool,
    stale: bool,
    generation: Option<u64>,
) -> (String, &'static str, bool) {
    let build = generation
        .map(|n| format!("build {n}"))
        .unwrap_or_else(|| "no successful build".into());
    if pending {
        (
            format!("Building · {build}"),
            "The canvas still shows the previous successful source snapshot.",
            true,
        )
    } else if failed {
        (
            format!("Build failed · {build}"),
            "The preview shows the last successful build. See diagnostics for the source error.",
            true,
        )
    } else if stale {
        (
            format!("Out-of-date preview · {build}"),
            "The source changed after this preview was built.",
            true,
        )
    } else if generation.is_none() {
        ("No preview built".into(), "Open a source file to build a preview.", true)
    } else {
        (
            format!("Preview current · {build}"),
            "The preview matches the latest successful source build.",
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::preview_provenance;
    #[test]
    fn pending_and_failed_never_claim_current() {
        assert!(preview_provenance(true, false, false, Some(2)).0.starts_with("Building"));
        assert!(preview_provenance(false, true, false, Some(2)).0.starts_with("Build failed"));
        assert!(preview_provenance(false, false, true, Some(2)).2);
        assert!(preview_provenance(false, false, false, None).2);
        assert!(!preview_provenance(false, false, false, Some(2)).2);
    }
}
