//! Keep source truth and last-successful preview provenance visible.
use crate::app::GuiShell;

impl GuiShell {
    pub(crate) fn workbench_status_ui(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let pending = self.preview_store.pending_rebuild_at.is_some()
            || self.preview_store.in_flight_rebuild.is_some();
        let failed = self.document_store.source.document.last_rebuild_error.is_some();
        let stale = self.document_store.snapshot_is_stale();
        let generation = self.document_store.last_good_snapshot().map(|s| s.generation.0);
        let (label, detail, warning) = preview_provenance(pending, failed, stale, generation);
        egui::Panel::top("workbench_source_status")
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme.palette.surface.panel)
                    .inner_margin(egui::Margin::symmetric(12, 5)),
            )
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("CODE-FIRST WORKBENCH")
                            .small()
                            .color(theme.palette.text.secondary),
                    );
                    ui.separator();
                    let color = if warning {
                        theme.palette.status.warning
                    } else {
                        theme.palette.status.success
                    };
                    ui.label(egui::RichText::new(label).small().color(color)).on_hover_text(detail);
                    if self.document_store.source.is_dirty() {
                        ui.label(
                            egui::RichText::new("Unsaved source")
                                .small()
                                .color(theme.palette.status.warning),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("Select · inspect · edit in code")
                                .small()
                                .color(theme.palette.text.muted),
                        );
                    });
                });
            });
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
            "Fix the source diagnostics. The preview is not the current source.",
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
            "The preview matches the latest successful source build. This is not an audit result.",
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
