use super::*;

impl GuiShell {
    pub(super) fn render_export_settings(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let format = self.export_store.export_state.format;
        let scene_dims = self.document_store.source.document.scene_dimensions;
        let scope = if self.document_store.source.document.is_composition() {
            ExportScope::WholeComposition
        } else {
            ExportScope::ActiveScene
        };
        let timeline_duration = self
            .document_store
            .source
            .document
            .export_target(scope)
            .map(|t| t.duration_s() as f32);
        let max_time = self.preview_store.preview.playback.duration_s as f32;
        let current_time = self.preview_store.preview.playback.current_time_s() as f32;

        // Scope mutable borrows so we can call &self methods afterward.
        {
            let state = &mut self.export_store.export_state;

            Self::export_shared_settings_rows(ui, &theme, sp, scene_dims, state);

            ui.add_space(sp.base.space_1);

            Self::export_format_settings_rows(
                ui,
                &theme,
                sp,
                format,
                timeline_duration,
                max_time,
                current_time,
                state,
            );

            ui.add_space(sp.base.space_2);

            // ── Output path ──
            Self::export_output_row(ui, state);
        }

        // Export scope selector (for compositions)
        if self.document_store.source.document.is_composition() {
            ui.horizontal(|ui| {
                ui.label("Export:");
                let mut scope = self.export_store.export_state.export_scope.clone();
                let mut changed = false;
                changed |= ui
                    .selectable_value(&mut scope, ExportScope::ActiveScene, "Active Scene")
                    .clicked();
                changed |= ui
                    .selectable_value(
                        &mut scope,
                        ExportScope::WholeComposition,
                        "Whole Composition",
                    )
                    .clicked();
                if changed {
                    self.export_store.export_state.export_scope = scope;
                    self.update_default_export_filename();
                }
            });
            ui.add_space(sp.base.space_2);
        }

        // Default filename hint
        if self.export_store.export_state.output_path.is_empty() {
            let default = self.suggest_export_filename();
            ui.label(
                RichText::new(format!("Default: {}", default.display()))
                    .size(TextRole::Micro.size())
                    .color(theme.palette.text.muted),
            );
        }
    }

    /// Resolution and preset rows shared by every export format.
    fn export_shared_settings_rows(
        ui: &mut egui::Ui,
        theme: &eparts::Theme,
        sp: crate::app::design_tokens::spatial::Spatial,
        scene_dims: animatix::timeline::SceneDimensions,
        state: &mut ExportDialogState,
    ) {
        // ── Resolution row ──
        Self::settings_row(ui, "Resolution", |ui| {
            let mut w_f64 = state.width as f64;
            layout::field_sized(ui, Some(78.0), |ui| {
                NumberField::new(&mut w_f64)
                    .speed(10.0)
                    .range(1.0..=8192.0)
                    .prefix("W: ")
                    .desired_width(78.0)
                    .show(ui);
            });
            state.width = w_f64 as u32;

            ui.add_space(sp.base.space_2);

            let mut h_f64 = state.height as f64;
            layout::field_sized(ui, Some(78.0), |ui| {
                NumberField::new(&mut h_f64)
                    .speed(10.0)
                    .range(1.0..=8192.0)
                    .prefix("H: ")
                    .desired_width(78.0)
                    .show(ui);
            });
            state.height = h_f64 as u32;

            ui.add_space(sp.base.space_2);

            if scene_dims.width > 0 && scene_dims.height > 0 {
                let resp = ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} Scene", egui_phosphor::regular::ARROWS_IN))
                            .size(TextRole::BodyS.size())
                            .color(theme.palette.accent.primary),
                    )
                    .selectable(false),
                );
                if resp.interact(egui::Sense::click()).clicked() {
                    state.width = scene_dims.width;
                    state.height = scene_dims.height;
                }
            }
        });

        ui.add_space(sp.base.space_1);

        // ── Shared export presets ──
        Self::settings_row(ui, "Presets", |ui| {
            for preset in animatix_render::encode::ExportPreset::ALL {
                let resp = ui.add(Button::secondary(preset.name).with_size(eparts::Size::Sm));
                if resp.clicked() {
                    state.width = preset.width;
                    state.height = preset.height;
                    state.fps = preset.fps;
                    state.video_codec = preset.video_codec;
                    state.h264_preset = preset.h264_preset;
                }
            }

            ui.add_space(sp.base.space_2);

            let reset = ui.add(
                Button::icon(egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE)
                    .with_size(eparts::Size::Sm)
                    .with_tooltip(
                        "Restore the encoding settings (keeps the format, scope and output path)",
                    ),
            );
            if reset.clicked() {
                let defaults = ExportDialogState::default();
                state.width = defaults.width;
                state.height = defaults.height;
                state.time_s = defaults.time_s;
                state.fps = defaults.fps;
                state.auto_duration = defaults.auto_duration;
                state.duration_s = defaults.duration_s;
                state.hold_s = defaults.hold_s;
                state.video_codec = defaults.video_codec;
                state.h264_preset = defaults.h264_preset;
            }
        });
    }

    /// Format-specific timing, duration and encoder rows.
    #[allow(clippy::too_many_arguments)]
    fn export_format_settings_rows(
        ui: &mut egui::Ui,
        theme: &eparts::Theme,
        sp: crate::app::design_tokens::spatial::Spatial,
        format: ExportFormat,
        timeline_duration: Option<f32>,
        max_time: f32,
        current_time: f32,
        state: &mut ExportDialogState,
    ) {
        // ── Format-specific settings ──
        match format {
            ExportFormat::Image | ExportFormat::WebP => {
                Self::settings_row(ui, "Time", |ui| {
                    let mut t = state.time_s as f64;
                    layout::field_sized(ui, Some(100.0), |ui| {
                        NumberField::new(&mut t)
                            .speed(0.1)
                            .range(0.0..=max_time as f64)
                            .suffix(" s")
                            .desired_width(100.0)
                            .show(ui);
                    });
                    state.time_s = t as f32;

                    ui.add_space(sp.base.space_2);

                    let resp = ui.add(
                        egui::Label::new(
                            RichText::new(format!("{} Current", egui_phosphor::regular::CLOCK))
                                .size(TextRole::BodyS.size())
                                .color(theme.palette.accent.primary),
                        )
                        .selectable(false),
                    );
                    if resp.interact(egui::Sense::click()).clicked() {
                        state.time_s = current_time;
                    }
                });
            },
            ExportFormat::Video | ExportFormat::Gif | ExportFormat::WebM | ExportFormat::Mov => {
                // FPS
                Self::settings_row(ui, "FPS", |ui| {
                    let mut fps_f64 = state.fps as f64;
                    layout::field_sized(ui, Some(70.0), |ui| {
                        NumberField::new(&mut fps_f64)
                            .speed(1.0)
                            .range(1.0..=120.0)
                            .suffix(" fps")
                            .desired_width(70.0)
                            .show(ui);
                    });
                    state.fps = fps_f64 as u32;
                });

                // Duration mode
                let auto_prev = state.auto_duration;
                Self::settings_row(ui, "Duration", |ui| {
                    ui.add(Checkbox::new(&mut state.auto_duration).label("Auto"));

                    if state.auto_duration {
                        ui.add_space(sp.base.space_2);
                        ui.label(
                            RichText::new("Hold:")
                                .size(TextRole::BodyS.size())
                                .color(theme.palette.text.secondary),
                        );

                        let mut hold = state.hold_s as f64;
                        layout::field_sized(ui, Some(80.0), |ui| {
                            NumberField::new(&mut hold)
                                .speed(0.1)
                                .range(0.0..=10.0)
                                .suffix(" s")
                                .desired_width(80.0)
                                .show(ui);
                        });
                        state.hold_s = hold as f32;
                    } else {
                        ui.add_space(sp.base.space_2);

                        let mut dur = state.duration_s as f64;
                        layout::field_sized(ui, Some(80.0), |ui| {
                            NumberField::new(&mut dur)
                                .speed(0.5)
                                .range(0.1..=3600.0)
                                .suffix(" s")
                                .desired_width(80.0)
                                .show(ui);
                        });
                        state.duration_s = dur as f32;
                    }
                });

                if !auto_prev && state.auto_duration {
                    // Just switched to auto
                } else if auto_prev && !state.auto_duration {
                    let auto_dur = if let Some(dur) = timeline_duration {
                        (dur + state.hold_s.max(0.0)).max(0.5)
                    } else {
                        state.duration_s.max(0.5)
                    };
                    state.duration_s = auto_dur;
                }

                if state.auto_duration {
                    let auto_dur = if let Some(dur) = timeline_duration {
                        (dur + state.hold_s.max(0.0)).max(0.5)
                    } else {
                        state.duration_s.max(0.5)
                    };
                    ui.label(
                        RichText::new(format!("Effective duration: {:.2}s", auto_dur))
                            .size(TextRole::Micro.size())
                            .color(theme.palette.text.muted),
                    );
                }

                // Encoder choice: MP4 and MOV honour it; WebM is pinned to
                // VP9 at the call site and GIF has a single encoder, so the
                // row is hidden there rather than shown as a no-op.
                if matches!(format, ExportFormat::Video | ExportFormat::Mov) {
                    ui.add_space(sp.base.space_1);
                    Self::settings_row(ui, "Encoder", |ui| {
                        // Added right-to-left: preset first so it lands on
                        // the right of the codec selector.
                        const PRESETS: &[animatix_render::encode::H264Preset] =
                            animatix_render::encode::H264Preset::ALL;
                        let preset_names: Vec<&str> = PRESETS.iter().map(|p| p.as_str()).collect();
                        let mut preset_idx = PRESETS.iter().position(|p| p == &state.h264_preset);
                        ui.add_sized(
                            Vec2::new(96.0, sp.base.row_s),
                            eparts::widget::Select::new(
                                "export_h264_preset",
                                &mut preset_idx,
                                &preset_names,
                            )
                            .placeholder("Preset"),
                        );
                        if let Some(idx) = preset_idx {
                            state.h264_preset = PRESETS[idx];
                        }

                        ui.add_space(sp.base.space_2);

                        const CODECS: &[animatix_render::encode::VideoCodec] =
                            animatix_render::encode::VideoCodec::ALL;
                        let codec_names: Vec<String> =
                            CODECS.iter().map(|c| c.to_string()).collect();
                        let mut codec_idx = CODECS.iter().position(|c| c == &state.video_codec);
                        ui.add_sized(
                            Vec2::new(120.0, sp.base.row_s),
                            eparts::widget::Select::new(
                                "export_video_codec",
                                &mut codec_idx,
                                &codec_names,
                            )
                            .placeholder("Codec"),
                        );
                        if let Some(idx) = codec_idx {
                            state.video_codec = CODECS[idx];
                        }
                    });
                }
            },
        }
    }

    /// Output path row.
    fn export_output_row(ui: &mut egui::Ui, state: &mut ExportDialogState) {
        Self::settings_row(ui, "Output", |ui| {
            let path_width = ui.available_width();
            TextField::new(&mut state.output_path)
                .placeholder("output filename…")
                .desired_width(path_width)
                .show(ui);
        });
    }

    /// Render a settings row with a left-aligned label and right-aligned content.
    fn settings_row(ui: &mut egui::Ui, label: &str, add_content: impl FnOnce(&mut egui::Ui)) {
        let theme = eparts::theme(ui);
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let available = ui.available_width();
        let row_h = sp.base.row_m;
        let (row_rect, _) =
            ui.allocate_exact_size(Vec2::new(available, row_h), egui::Sense::hover());

        let label_width = (available * 0.32).clamp(70.0, 110.0);
        let content_left = row_rect.min.x + label_width + sp.base.space_4;

        // Label (left side)
        let label_rect = egui::Rect::from_min_max(
            egui::pos2(row_rect.min.x, row_rect.min.y),
            egui::pos2(row_rect.min.x + label_width, row_rect.max.y),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(label_rect), |ui| {
            ui.with_layout(
                egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(label)
                                .size(TextRole::BodyS.size())
                                .color(theme.palette.text.secondary),
                        )
                        .selectable(false),
                    );
                },
            );
        });

        // Content (right side)
        let content_rect = egui::Rect::from_min_max(
            egui::pos2(content_left, row_rect.min.y),
            egui::pos2(row_rect.max.x, row_rect.max.y),
        );
        ui.scope_builder(egui::UiBuilder::new().max_rect(content_rect), |ui| {
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(false),
                add_content,
            );
        });
    }

    // ─── Action Bar ───────────────────────────────────────────────────────────
}
