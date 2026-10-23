use super::*;

impl GuiShell {
    pub(super) fn render_export_progress_overlay(
        &mut self,
        ui: &mut egui::Ui,
        dialog_rect: egui::Rect,
    ) {
        let theme = eparts::theme(ui);
        // Keep spinner animating
        ui.ctx().request_repaint();
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let content_rect = dialog_rect.shrink(sp.base.space_5);
        let center_y = content_rect.center().y;
        let spinner_center = egui::pos2(content_rect.center().x, center_y - 36.0);

        // Animated spinner ring
        let time = ui.ctx().input(|i| i.time);
        let n_dots = 8;
        let radius = 14.0;
        let base_alpha = theme.palette.status.warning.a();
        for i in 0..n_dots {
            let angle = (i as f32 / n_dots as f32) * std::f32::consts::TAU - (time * 3.0) as f32;
            let pos = spinner_center + Vec2::new(angle.cos() * radius, angle.sin() * radius);
            let fade = ((i as f32) / (n_dots as f32 - 1.0)).clamp(0.0, 1.0);
            let alpha = (fade * base_alpha as f32) as u8;
            ui.painter().circle_filled(
                pos,
                2.5,
                Color32::from_rgba_premultiplied(
                    theme.palette.status.warning.r(),
                    theme.palette.status.warning.g(),
                    theme.palette.status.warning.b(),
                    alpha,
                ),
            );
        }

        // Status text
        ui.painter().text(
            egui::pos2(content_rect.center().x, center_y + 4.0),
            egui::Align2::CENTER_CENTER,
            "Exporting…",
            TextRole::Title.font_id(),
            theme.palette.text.primary,
        );

        // Subtitle
        let format_label = match self.export_store.export_state.format {
            ExportFormat::Image => "Rendering single frame",
            ExportFormat::Video => "Rendering video frames",
            ExportFormat::Gif => "Rendering GIF frames",
            ExportFormat::WebM => "Rendering WebM frames",
            ExportFormat::Mov => "Rendering MOV frames",
            ExportFormat::WebP => "Rendering WebP frame",
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
            "Frame 1/1".to_string()
        } else {
            format!("Frame {}/{}  ({:.0}%)", progress.min(total), total, pct * 100.0)
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
            let time_str = format!("Elapsed: {:02}:{:02}", mins, secs);
            ui.painter().text(
                egui::pos2(content_rect.center().x, bar_y + 28.0),
                egui::Align2::CENTER_TOP,
                time_str,
                TextRole::Micro.font_id(),
                theme.palette.text.muted,
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
                ui.add(Button::secondary("Cancel")).clicked()
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
