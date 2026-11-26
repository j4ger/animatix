use super::*;

impl GuiShell {
    pub(super) fn render_export_progress_overlay(
        &mut self,
        ui: &mut egui::Ui,
        dialog_rect: egui::Rect,
    ) {
        let theme = eparts::theme(ui);
        // Progress counters are published by the encoder worker.
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let content_rect = dialog_rect.shrink(sp.base.space_5);
        let center_y = content_rect.center().y;
        let spinner_center = egui::pos2(content_rect.center().x, center_y - 36.0);

        let indicator = egui::Rect::from_center_size(spinner_center, Vec2::splat(28.0));
        ui.put(
            indicator,
            eparts::Spinner::new().set_size(28.0).with_color(theme.palette.status.warning),
        );

        // Status text
        ui.painter().text(
            egui::pos2(content_rect.center().x, center_y + 4.0),
            egui::Align2::CENTER_CENTER,
            text(ui, "gui-export-in-progress"),
            TextRole::Title.font_id(),
            theme.palette.text.primary,
        );

        // Subtitle
        let format_label = match self.export_store.export_state.format {
            ExportFormat::Image => text(ui, "gui-export-render-single-frame"),
            ExportFormat::Video => text(ui, "gui-export-render-video-frames"),
            ExportFormat::Gif => text(ui, "gui-export-render-gif-frames"),
            ExportFormat::WebM => text(ui, "gui-export-render-webm-frames"),
            ExportFormat::Mov => text(ui, "gui-export-render-mov-frames"),
            ExportFormat::WebP => text(ui, "gui-export-render-webp-frame"),
        };
        ui.painter().text(
            egui::pos2(content_rect.center().x, center_y + 26.0),
            egui::Align2::CENTER_CENTER,
            format_label,
            TextRole::BodyS.font_id(),
            theme.palette.text.muted,
        );

        // Progress bar + frame count
        let progress = self.export_store.export_progress.load(std::sync::atomic::Ordering::Relaxed);
        let total = self.export_store.export_total_frames.max(1);
        let pct = (progress as f32 / total as f32).clamp(0.0, 1.0);

        let bar_w = content_rect.width().min(280.0);
        let bar_y = center_y + 50.0;
        let bar_rect = egui::Rect::from_center_size(
            egui::pos2(content_rect.center().x, bar_y),
            Vec2::new(bar_w, 20.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(bar_rect), |ui| {
            ui.add(ProgressBar::new(pct));
        });

        // Frame count / percentage text
        let progress_text = if matches!(
            self.export_store.export_state.format,
            ExportFormat::Image | ExportFormat::WebP
        ) {
            text(ui, "gui-export-single-frame")
        } else {
            render(
                ui,
                "gui-export-frame-progress",
                &[
                    ("current", progress.min(total).to_string()),
                    ("total", total.to_string()),
                    ("percent", format!("{:.0}", pct * 100.0)),
                ],
            )
        };
        ui.painter().text(
            egui::pos2(content_rect.center().x, bar_y + 14.0),
            egui::Align2::CENTER_TOP,
            progress_text,
            TextRole::Micro.font_id(),
            theme.palette.text.muted,
        );

        // Elapsed time
        if let Some(start) = self.export_store.export_start_time {
            let elapsed = start.elapsed().as_secs_f32();
            let mins = (elapsed / 60.0) as u32;
            let secs = (elapsed % 60.0) as u32;
            let time_str =
                render(ui, "gui-export-elapsed", &[("time", format!("{mins:02}:{secs:02}"))]);
            ui.painter().text(
                egui::pos2(content_rect.center().x, bar_y + 28.0),
                egui::Align2::CENTER_TOP,
                time_str,
                TextRole::Micro.font_id(),
                theme.palette.text.muted,
            );
        }

        if let Some(warning) = self.export_store.export_warnings.first() {
            let warning_rect = egui::Rect::from_center_size(
                egui::pos2(content_rect.center().x, content_rect.top() + 22.0),
                egui::vec2(content_rect.width().min(420.0), 24.0),
            );
            ui.put(
                warning_rect,
                egui::Label::new(
                    RichText::new(warning.display(self.ui_store.view.language))
                        .color(theme.palette.status.warning),
                )
                .truncate(),
            )
            .on_hover_text(
                self.export_store
                    .export_warnings
                    .iter()
                    .map(|warning| warning.display(self.ui_store.view.language))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }

        // Cancel button
        let btn_size = Vec2::new(100.0, sp.base.row_m);
        let btn_rect = egui::Rect::from_center_size(
            egui::pos2(content_rect.center().x, content_rect.bottom() - 20.0),
            btn_size,
        );
        let cancel_clicked = ui
            .scope_builder(egui::UiBuilder::new().max_rect(btn_rect), |ui| {
                ui.add(Button::secondary(crate::app::i18n::text(ui, "gui-action-cancel")))
                    .clicked()
            })
            .inner;
        if cancel_clicked {
            self.export_store
                .export_cancelled
                .store(true, std::sync::atomic::Ordering::Relaxed);
            self.export_store.export_status = ExportStatus::Idle;
            self.export_store.export_dialog_open = false;
        }
    }

    // ─── Settings Form ────────────────────────────────────────────────────────
}
