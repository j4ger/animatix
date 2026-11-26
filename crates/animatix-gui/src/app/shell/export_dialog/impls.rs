use super::*;

// "Restore defaults" resets the *encoding* settings (resolution, timing, codec,
// preset) — the knobs a preset would have set. It keeps `format` and
// `export_scope` (what to export, and from where) plus `output_path` (a
// destination the user typed), because clobbering those discards a deliberate
// choice rather than a setting. Pinned by
// `restore_defaults_keeps_target_but_resets_encoding`.

impl Default for ExportDialogState {
    fn default() -> Self {
        Self {
            format: ExportFormat::Video,
            width: 1920,
            height: 1080,
            lock_aspect: true,
            follow_canvas: true,
            time_s: 0.0,
            fps: 30,
            auto_duration: true,
            duration_s: 5.0,
            hold_s: 1.0,
            output_path: String::new(),
            export_scope: ExportScope::ActiveScene,
            video_codec: animatix_render::encode::VideoCodec::Auto,
            h264_preset: animatix_render::encode::H264Preset::Medium,
        }
    }
}

impl GuiShell {
    pub(crate) fn export_dialog_ui(&mut self, ui: &mut egui::Ui) {
        use eparts::tokens::motion::{MODAL, MODAL_EXIT, PANEL};
        use eparts::widget::anim::animate_toward_eased;

        let ctx = ui.ctx().clone();
        let id = egui::Id::unique("studio_export");
        let open = self.export_store.export_dialog_open;
        let progress = animate_toward_eased(
            &ctx,
            id.with("open"),
            u8::from(open) as f32,
            if open { MODAL } else { MODAL_EXIT },
        );
        if !open && progress <= 0.0 {
            return;
        }
        let running = matches!(self.export_store.export_status, ExportStatus::Running);
        let confirming = matches!(self.export_store.export_status, ExportStatus::ConfirmDraft(_));
        let screen = ctx.content_rect();
        let theme = eparts::theme(ui);
        let sp = eparts::spatial(ui);
        let width = (screen.width() * 0.48)
            .clamp(380.0, 560.0)
            .min((screen.width() - sp.space_6).max(1.0));
        let form_height = (screen.height() * 0.60).clamp(410.0, 580.0);
        let progress_height = (screen.height() * 0.32).clamp(240.0, 280.0);
        let compact =
            animate_toward_eased(&ctx, id.with("running"), u8::from(running) as f32, PANEL);
        let height = egui::lerp(form_height..=progress_height, compact)
            .min((screen.height() - sp.space_6).max(1.0));
        let mut shadow = theme.elevation.overlay;
        shadow.color = shadow.color.gamma_multiply(progress);
        let frame = egui::Frame::new()
            .corner_radius(RADIUS_XL)
            .shadow(shadow)
            .fill(theme.palette.surface.base.gamma_multiply(progress))
            .stroke(Stroke::new(
                STROKE_WIDTH,
                theme.palette.border.default.gamma_multiply(progress),
            ));
        let area = egui::Modal::default_area(id)
            .fade_in(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, sp.space_3 * (1.0 - progress)]);
        let modal = egui::Modal::new(id)
            .area(area)
            .frame(frame)
            .backdrop_color(theme.palette.overlay.backdrop.gamma_multiply(progress))
            .show(&ctx, |ui| {
                ui.set_opacity(progress);
                if !open {
                    ui.disable();
                }
                let size = Vec2::new(width, height);
                ui.set_min_size(size);
                let rect = egui::Rect::from_min_size(ui.min_rect().min, size);
                ui.set_clip_rect(ui.clip_rect().intersect(rect));
                if running {
                    self.render_export_progress_overlay(ui, rect);
                } else {
                    self.export_form(ui, rect);
                }
            });
        if open && !running && !confirming && modal.should_close() {
            self.export_store.export_dialog_open = false;
        }
        self.export_draft_confirmation(&ctx);
        if open != self.export_store.export_dialog_open
            || running != matches!(self.export_store.export_status, ExportStatus::Running)
        {
            ctx.request_repaint();
        }
    }

    fn export_form(&mut self, ui: &mut egui::Ui, dialog_rect: egui::Rect) {
        let theme = eparts::theme(ui);
        let sp = crate::app::design_tokens::spatial::spatial(ui);
        let content_rect = dialog_rect.shrink(sp.base.space_5);
        let mut cursor_y = content_rect.top();

        // ── Title row ──
        ui.painter().text(
            egui::pos2(content_rect.left(), cursor_y + 14.0),
            egui::Align2::LEFT_CENTER,
            crate::app::i18n::text(ui, "gui-export-title"),
            TextRole::Heading.font_id(),
            theme.palette.text.primary,
        );

        // Close button
        let close_size = Vec2::new(sp.base.row_l, sp.base.row_l);
        let close_rect = egui::Rect::from_min_size(
            egui::pos2(content_rect.right() - close_size.x, cursor_y),
            close_size,
        );
        let close_clicked = ui
            .scope_builder(egui::UiBuilder::new().max_rect(close_rect), |ui| {
                ui.add(
                    Button::icon(egui_phosphor::regular::X)
                        .with_tooltip(crate::app::i18n::text(ui, "gui-action-close-esc")),
                )
                .clicked()
            })
            .inner;
        if close_clicked {
            self.export_store.export_dialog_open = false;
        }

        cursor_y += sp.base.row_l + sp.base.space_4;

        // Divider
        ui.painter().line_segment(
            [
                egui::pos2(content_rect.left(), cursor_y),
                egui::pos2(content_rect.right(), cursor_y),
            ],
            Stroke::new(STROKE_WIDTH, theme.palette.border.default),
        );
        cursor_y += sp.base.space_4;

        // ── Format tabs ──
        let formats = [
            ExportFormat::Image,
            ExportFormat::Video,
            ExportFormat::Gif,
            ExportFormat::WebM,
            ExportFormat::Mov,
            ExportFormat::WebP,
        ];
        let labels: Vec<String> = formats
            .iter()
            .map(|format| match format {
                ExportFormat::Image => text(ui, "gui-export-image"),
                ExportFormat::Video => text(ui, "gui-export-video"),
                ExportFormat::Gif => "GIF".to_owned(),
                ExportFormat::WebM => "WebM".to_owned(),
                ExportFormat::Mov => "MOV".to_owned(),
                ExportFormat::WebP => "WebP".to_owned(),
            })
            .collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut selected = formats
            .iter()
            .position(|format| *format == self.export_store.export_state.format)
            .unwrap_or(0);
        let tab_rect = egui::Rect::from_min_size(
            egui::pos2(content_rect.left(), cursor_y),
            Vec2::new(content_rect.width(), sp.base.component.pill_tab_height),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(tab_rect), |ui| {
            TabBar::new("export_format_tabs", &mut selected, &labels).underlined().show(ui);
        });
        if let Some(format) = formats.get(selected).copied() {
            self.set_export_format(format);
        }
        cursor_y += sp.base.component.pill_tab_height + sp.base.space_3;

        // ── Settings content ──
        let settings_rect = egui::Rect::from_min_size(
            egui::pos2(content_rect.left(), cursor_y),
            Vec2::new(content_rect.width(), content_rect.bottom() - cursor_y - 54.0),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(settings_rect), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, sp.base.space_3);
            ui.set_clip_rect(settings_rect.intersect(ui.clip_rect()));
            egui::ScrollArea::vertical()
                .id_salt("export_settings")
                .max_height(settings_rect.height())
                .show(ui, |ui| self.render_export_settings(ui));
        });

        // ── Status / action bar ──
        cursor_y = content_rect.bottom() - (sp.base.row_l + sp.base.space_4);
        let action_rect = egui::Rect::from_min_size(
            egui::pos2(content_rect.left(), cursor_y),
            Vec2::new(content_rect.width(), sp.base.row_l + sp.base.space_3),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(action_rect), |ui| {
            self.render_export_action_bar(ui);
        });
    }

    fn export_draft_confirmation(&mut self, ctx: &egui::Context) {
        if let ExportStatus::ConfirmDraft(affected) = &self.export_store.export_status {
            let response =
                egui::Modal::new(egui::Id::unique("confirm_narration_draft")).show(ctx, |ui| {
                    ui.set_max_width(440.0);
                    ui.heading(crate::app::i18n::text(ui, "gui-export-incomplete-narration"));
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        for paragraph in affected {
                            let scene = paragraph
                                .scene
                                .clone()
                                .unwrap_or_else(|| text(ui, "gui-export-paragraph"));
                            ui.label(render(
                                ui,
                                "gui-export-paragraph-title",
                                &[
                                    ("scene", scene),
                                    ("number", paragraph.number.to_string()),
                                    ("snippet", paragraph.snippet.clone()),
                                ],
                            ));
                            ui.weak(text(ui, paragraph.effect));
                        }
                    });
                    let mut proceed = false;
                    let mut back = false;
                    ui.horizontal(|ui| {
                        back = ui.button(crate::app::i18n::text(ui, "gui-action-back")).clicked();
                        proceed =
                            ui.button(crate::app::i18n::text(ui, "gui-export-draft")).clicked();
                    });
                    (proceed, back)
                });
            if response.inner.0 {
                self.submit_export(Some(affected.clone()));
            } else if response.inner.1
                || response.should_close()
                || !self.export_store.export_dialog_open
            {
                self.export_store.export_status = ExportStatus::Idle;
            }
        }
    }
}
