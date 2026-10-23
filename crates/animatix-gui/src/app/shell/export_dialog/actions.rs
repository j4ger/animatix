use super::*;

impl GuiShell {
    pub(super) fn render_export_action_bar(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        ui.horizontal(|ui| {
            // Status message (left side)
            match &self.export_store.export_status {
                ExportStatus::Idle => {},
                ExportStatus::Complete { path } => {
                    let path_str = path.display().to_string();
                    let label = truncate_middle(&path_str, 15, 15);
                    let resp = ui.add(
                        Tag::new(format!("{} {}", egui_phosphor::regular::CHECK, label))
                            .color(theme.palette.status.success)
                            .removable(true),
                    );
                    if resp.clicked() {
                        self.export_store.export_status = ExportStatus::Idle;
                    }
                },
                ExportStatus::Failed(err) => {
                    // Keep the bar readable but never hide the reason: the full
                    // message is one hover away and copied on request, since
                    // encoder failures carry the actionable detail.
                    let truncated = truncate_chars(err, 37);
                    let resp = ui.add(
                        Tag::new(format!("{} {}", egui_phosphor::regular::WARNING, truncated))
                            .color(theme.palette.status.error)
                            .removable(true),
                    );
                    let resp = resp.on_hover_ui(|ui| {
                        ui.set_max_width(420.0);
                        ui.label(RichText::new(err).size(TextRole::BodyS.size()));
                        ui.add_space(sp.base.space_1);
                        if ui
                            .add(Button::ghost("Copy").with_icon(egui_phosphor::regular::COPY))
                            .clicked()
                        {
                            ui.ctx().copy_text(err.clone());
                        }
                    });
                    if resp.clicked() {
                        self.export_store.export_status = ExportStatus::Idle;
                    }
                },
                ExportStatus::Running => {},
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Export button
                let btn_text = match self.export_store.export_state.format {
                    ExportFormat::Image => "Export Image",
                    ExportFormat::Video => "Export Video",
                    ExportFormat::Gif => "Export GIF",
                    ExportFormat::WebM => "Export WebM",
                    ExportFormat::Mov => "Export MOV",
                    ExportFormat::WebP => "Export WebP",
                };
                let btn_size = Vec2::new(120.0, sp.base.row_m);
                let (btn_rect, btn_resp) = ui.allocate_exact_size(btn_size, egui::Sense::click());

                let btn_bg = theme.palette.status.warning;

                ui.painter().rect_filled(btn_rect, RADIUS_M, btn_bg);
                ui.painter().text(
                    btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    btn_text,
                    TextRole::Body.font_id(),
                    theme.palette.surface.base,
                );

                if btn_resp.clicked() {
                    self.start_export();
                }
            });
        });
    }

    pub(super) fn suggest_export_filename(&self) -> PathBuf {
        let ext = match self.export_store.export_state.format {
            ExportFormat::Image => "png",
            ExportFormat::Video => "mp4",
            ExportFormat::Gif => "gif",
            ExportFormat::WebM => "webm",
            ExportFormat::Mov => "mov",
            ExportFormat::WebP => "webp",
        };
        let stem = self
            .document_store
            .source
            .document
            .file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("animatix");
        let workspace = self
            .document_store
            .source
            .document
            .file_path
            .parent()
            .unwrap_or(std::path::Path::new("."));
        workspace.join(format!("{}_export.{ext}", stem))
    }

    pub(crate) fn update_default_export_filename(&mut self) {
        let path = self.suggest_export_filename();
        self.export_store.export_state.output_path = path.to_string_lossy().to_string();
    }

    pub(super) fn start_export(&mut self) {
        // Join any finished thread first so a stale handle cannot block a new export.
        self.export_store.poll_export_status();
        if self.export_store.export_thread.is_some() {
            self.export_store.export_status = ExportStatus::Running;
            self.ui_store.toasts.push(crate::app::components::toast::Toast::warning(
                "Previous export is still finishing".to_string(),
            ));
            return;
        }

        let scope = self.export_store.export_state.export_scope.clone();

        let target = match self.document_store.source.document.export_target(scope) {
            Some(t) => t,
            None => {
                self.export_store.export_status =
                    ExportStatus::Failed("No timeline or composition to export".into());
                return;
            },
        };

        let cloned_target = match target {
            crate::app::document::export_target::ExportTargetRef::Timeline { timeline, .. } => {
                crate::app::document::export_target::ExportTargetOwned::Timeline(Box::new(
                    timeline.clone(),
                ))
            },
            crate::app::document::export_target::ExportTargetRef::Composition {
                composition,
                ..
            } => crate::app::document::export_target::ExportTargetOwned::Composition(Box::new(
                composition.clone(),
            )),
        };

        let effective_duration_s = target.duration_s();

        // Keep the full export target for dispatch below.
        // Timeline targets go to render_*_timeline_with_progress,
        // Composition targets go to render_*_composition_with_progress.
        let has_composition = matches!(
            cloned_target,
            crate::app::document::export_target::ExportTargetOwned::Composition(_)
        );

        let state = self.export_store.export_state.clone();
        let output_path = if state.output_path.is_empty() {
            self.suggest_export_filename()
        } else {
            PathBuf::from(&state.output_path)
        };

        // ── Margin-case validation ──
        if state.width == 0 || state.height == 0 {
            self.export_store.export_status = ExportStatus::Failed("Resolution must be > 0".into());
            return;
        }
        if state.width > 8192 || state.height > 8192 {
            self.export_store.export_status =
                ExportStatus::Failed("Resolution exceeds 8192px limit".into());
            return;
        }
        match state.format {
            ExportFormat::Video | ExportFormat::Gif | ExportFormat::WebM | ExportFormat::Mov => {
                if state.fps == 0 {
                    self.export_store.export_status =
                        ExportStatus::Failed("FPS must be > 0".into());
                    return;
                }
                let duration = if state.auto_duration {
                    let d = effective_duration_s as f32 + state.hold_s.max(0.0);
                    d.max(0.5)
                } else {
                    state.duration_s
                };
                if duration <= 0.0 {
                    self.export_store.export_status =
                        ExportStatus::Failed("Duration must be > 0".into());
                    return;
                }
            },
            _ => {},
        }
        if output_path.as_os_str().is_empty() {
            self.export_store.export_status = ExportStatus::Failed("Output path is empty".into());
            return;
        }
        if let Some(parent) = output_path.parent() {
            if !parent.exists() {
                self.export_store.export_status =
                    ExportStatus::Failed(format!("Directory does not exist: {}", parent.display()));
                return;
            }
        }

        let debug = animatix::timeline::DebugRenderOptions {
            draw_bounds: self.ui_store.view.debug_bounds,
            compute_hit_regions: true,
            draw_layout_debug: self.ui_store.view.debug_layout,
            draw_spacing: self.ui_store.view.debug_spacing,
        };

        // Reset progress / cancel state
        self.export_store.export_progress.store(0, std::sync::atomic::Ordering::Relaxed);
        self.export_store
            .export_cancelled
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.export_store.export_start_time = Some(std::time::Instant::now());
        self.export_store.export_status = ExportStatus::Running;

        // Compute total frames for progress display
        self.export_store.export_total_frames = match state.format {
            ExportFormat::Image | ExportFormat::WebP => 1,
            ExportFormat::Video | ExportFormat::Gif | ExportFormat::WebM | ExportFormat::Mov => {
                let duration = if state.auto_duration {
                    let d = effective_duration_s as f32 + state.hold_s.max(0.0);
                    d.max(0.5)
                } else {
                    state.duration_s
                };
                (duration * state.fps as f32).ceil() as u32
            },
        };

        // ── Spawn background export worker (always; format-specific
        //     paths below are individually cfg-gated) ────────────────
        let result_path = output_path.clone();
        let progress = Arc::clone(&self.export_store.export_progress);
        let cancel = Arc::clone(&self.export_store.export_cancelled);
        let handle = std::thread::spawn(move || {
            let progress_ref = Some(progress.as_ref());
            let cancel_ref = Some(cancel.as_ref());
            let result: Result<(), animatix_render::encode::ExportError> = match state.format {
                ExportFormat::Image | ExportFormat::WebP => {
                    // Ungated image path — no FFmpeg needed.
                    if has_composition {
                        match &cloned_target {
                            crate::app::document::export_target::ExportTargetOwned::Composition(
                                comp,
                            ) => animatix_render::encode::render_image_composition(
                                comp,
                                state.width,
                                state.height,
                                state.time_s,
                                &output_path,
                            ),
                            _ => Err(animatix_render::encode::ExportError::Internal(format!(
                                "export worker: format {:?} expected a composition target",
                                state.format
                            ))),
                        }
                    } else {
                        match &cloned_target {
                            crate::app::document::export_target::ExportTargetOwned::Timeline(
                                tl,
                            ) => {
                                let timeline = tl.as_ref().clone();
                                animatix_render::encode::render_image_timeline_with_progress(
                                    timeline,
                                    state.width,
                                    state.height,
                                    state.time_s,
                                    &output_path,
                                    debug,
                                    progress_ref,
                                    cancel_ref,
                                )
                            },
                            _ => Err(animatix_render::encode::ExportError::Internal(format!(
                                "export worker: format {:?} expected a timeline target",
                                state.format
                            ))),
                        }
                    }
                },
                #[cfg(feature = "video")]
                ExportFormat::Video => {
                    let duration = if state.auto_duration {
                        let d = effective_duration_s as f32 + state.hold_s.max(0.0);
                        d.max(0.5)
                    } else {
                        state.duration_s
                    };
                    if has_composition {
                        match &cloned_target {
                            crate::app::document::export_target::ExportTargetOwned::Composition(
                                comp,
                            ) => animatix_render::encode::render_video_composition_with_progress(
                                comp,
                                state.width,
                                state.height,
                                state.fps,
                                duration,
                                &output_path,
                                debug,
                                animatix_render::encode::ExportSettings {
                                    video_codec: state.video_codec,
                                    h264_preset: state.h264_preset,
                                    ..Default::default()
                                },
                                progress_ref,
                                cancel_ref,
                            ),
                            _ => Err(animatix_render::encode::ExportError::Internal(format!(
                                "export worker: format {:?} expected a composition target",
                                state.format
                            ))),
                        }
                    } else {
                        match &cloned_target {
                            crate::app::document::export_target::ExportTargetOwned::Timeline(
                                tl,
                            ) => {
                                let timeline = tl.as_ref().clone();
                                animatix_render::encode::render_video_timeline_with_progress(
                                    timeline,
                                    state.width,
                                    state.height,
                                    state.fps,
                                    duration,
                                    &output_path,
                                    debug,
                                    animatix_render::encode::ExportSettings {
                                        video_codec: state.video_codec,
                                        h264_preset: state.h264_preset,
                                        ..Default::default()
                                    },
                                    progress_ref,
                                    cancel_ref,
                                )
                            },
                            _ => Err(animatix_render::encode::ExportError::Internal(format!(
                                "export worker: format {:?} expected a timeline target",
                                state.format
                            ))),
                        }
                    }
                },
                #[cfg(feature = "video")]
                ExportFormat::WebM => {
                    let duration = if state.auto_duration {
                        let d = effective_duration_s as f32 + state.hold_s.max(0.0);
                        d.max(0.5)
                    } else {
                        state.duration_s
                    };
                    if has_composition {
                        match &cloned_target {
                            crate::app::document::export_target::ExportTargetOwned::Composition(
                                comp,
                            ) => animatix_render::encode::render_video_composition_with_progress(
                                comp,
                                state.width,
                                state.height,
                                state.fps,
                                duration,
                                &output_path,
                                debug,
                                animatix_render::encode::ExportSettings {
                                    video_codec: animatix_render::encode::VideoCodec::Vp9,
                                    ..Default::default()
                                },
                                progress_ref,
                                cancel_ref,
                            ),
                            _ => Err(animatix_render::encode::ExportError::Internal(format!(
                                "export worker: format {:?} expected a composition target",
                                state.format
                            ))),
                        }
                    } else {
                        match &cloned_target {
                            crate::app::document::export_target::ExportTargetOwned::Timeline(
                                tl,
                            ) => {
                                let timeline = tl.as_ref().clone();
                                animatix_render::encode::render_video_timeline_with_progress(
                                    timeline,
                                    state.width,
                                    state.height,
                                    state.fps,
                                    duration,
                                    &output_path,
                                    debug,
                                    animatix_render::encode::ExportSettings {
                                        video_codec: animatix_render::encode::VideoCodec::Vp9,
                                        ..Default::default()
                                    },
                                    progress_ref,
                                    cancel_ref,
                                )
                            },
                            _ => Err(animatix_render::encode::ExportError::Internal(format!(
                                "export worker: format {:?} expected a timeline target",
                                state.format
                            ))),
                        }
                    }
                },
                #[cfg(feature = "video")]
                ExportFormat::Mov => {
                    let duration = if state.auto_duration {
                        let d = effective_duration_s as f32 + state.hold_s.max(0.0);
                        d.max(0.5)
                    } else {
                        state.duration_s
                    };
                    if has_composition {
                        match &cloned_target {
                            crate::app::document::export_target::ExportTargetOwned::Composition(
                                comp,
                            ) => animatix_render::encode::render_video_composition_with_progress(
                                comp,
                                state.width,
                                state.height,
                                state.fps,
                                duration,
                                &output_path,
                                debug,
                                animatix_render::encode::ExportSettings {
                                    video_codec: state.video_codec,
                                    h264_preset: state.h264_preset,
                                    ..Default::default()
                                },
                                progress_ref,
                                cancel_ref,
                            ),
                            _ => Err(animatix_render::encode::ExportError::Internal(format!(
                                "export worker: format {:?} expected a composition target",
                                state.format
                            ))),
                        }
                    } else {
                        match &cloned_target {
                            crate::app::document::export_target::ExportTargetOwned::Timeline(
                                tl,
                            ) => {
                                let timeline = tl.as_ref().clone();
                                animatix_render::encode::render_video_timeline_with_progress(
                                    timeline,
                                    state.width,
                                    state.height,
                                    state.fps,
                                    duration,
                                    &output_path,
                                    debug,
                                    animatix_render::encode::ExportSettings {
                                        video_codec: state.video_codec,
                                        h264_preset: state.h264_preset,
                                        ..Default::default()
                                    },
                                    progress_ref,
                                    cancel_ref,
                                )
                            },
                            _ => Err(animatix_render::encode::ExportError::Internal(format!(
                                "export worker: format {:?} expected a timeline target",
                                state.format
                            ))),
                        }
                    }
                },
                #[cfg(feature = "video")]
                ExportFormat::Gif => {
                    let duration = if state.auto_duration {
                        let d = effective_duration_s as f32 + state.hold_s.max(0.0);
                        d.max(0.5)
                    } else {
                        state.duration_s
                    };
                    if has_composition {
                        match &cloned_target {
                            crate::app::document::export_target::ExportTargetOwned::Composition(
                                comp,
                            ) => animatix_render::encode::render_gif_composition_with_progress(
                                comp,
                                state.width,
                                state.height,
                                state.fps,
                                duration,
                                &output_path,
                                debug,
                                animatix_render::encode::ExportSettings {
                                    video_codec: state.video_codec,
                                    h264_preset: state.h264_preset,
                                    ..Default::default()
                                },
                                progress_ref,
                                cancel_ref,
                            ),
                            _ => Err(animatix_render::encode::ExportError::Internal(format!(
                                "export worker: format {:?} expected a composition target",
                                state.format
                            ))),
                        }
                    } else {
                        match &cloned_target {
                            crate::app::document::export_target::ExportTargetOwned::Timeline(
                                tl,
                            ) => {
                                let timeline = tl.as_ref().clone();
                                animatix_render::encode::render_gif_timeline_with_progress(
                                    timeline,
                                    state.width,
                                    state.height,
                                    state.fps,
                                    duration,
                                    &output_path,
                                    debug,
                                    animatix_render::encode::ExportSettings {
                                        video_codec: state.video_codec,
                                        h264_preset: state.h264_preset,
                                        ..Default::default()
                                    },
                                    progress_ref,
                                    cancel_ref,
                                )
                            },
                            _ => Err(animatix_render::encode::ExportError::Internal(format!(
                                "export worker: format {:?} expected a timeline target",
                                state.format
                            ))),
                        }
                    }
                },
                // FFmpeg-only formats without the video feature:
                #[cfg(not(feature = "video"))]
                ExportFormat::Video
                | ExportFormat::WebM
                | ExportFormat::Mov
                | ExportFormat::Gif => Err(animatix_render::encode::ExportError::Internal(
                    "This format requires the 'video' feature (FFmpeg)".into(),
                )),
            };
            (result, result_path)
        });
        self.export_store.export_thread = Some(handle);
    }
}
