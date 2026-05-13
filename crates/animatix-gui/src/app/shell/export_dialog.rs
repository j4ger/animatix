use egui::{Color32, RichText, Stroke, Vec2};
use std::path::PathBuf;

use crate::app::theme::*;
use crate::app::components::widgets::pill_tab_bar;
use crate::app::GuiShell;


// ─── Export Configuration ───────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    Image,
    Video,
    Gif,
}

#[derive(Clone, Debug)]
pub(crate) struct ExportDialogState {
    pub(crate) format: ExportFormat,
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// For Image export — time in seconds.
    pub(crate) time_s: f32,
    /// For Video/GIF export.
    pub(crate) fps: u32,
    /// When true, duration is auto-detected from timeline + hold.
    pub(crate) auto_duration: bool,
    /// Manual duration override (only used when auto_duration is false).
    pub(crate) duration_s: f32,
    /// Seconds to hold after last keyframe (only for auto duration).
    pub(crate) hold_s: f32,
    /// Output file path (relative or absolute).
    pub(crate) output_path: String,
}

impl Default for ExportDialogState {
    fn default() -> Self {
        Self {
            format: ExportFormat::Video,
            width: 1280,
            height: 720,
            time_s: 0.0,
            fps: 30,
            auto_duration: true,
            duration_s: 5.0,
            hold_s: 1.0,
            output_path: String::new(),
        }
    }
}

// ─── Export Status ──────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub(crate) enum ExportStatus {
    Idle,
    Running,
    Complete { path: PathBuf },
    Failed(String),
}

// ─── Public API ─────────────────────────────────────────────────────────────

impl GuiShell {
    pub(crate) fn export_dialog_ui(&mut self, ui: &mut egui::Ui) {
        let screen_rect = ui.ctx().viewport_rect();

        // Dark semi-transparent backdrop
        ui.painter().rect_filled(
            screen_rect,
            0.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, 120),
        );

        let is_running = matches!(self.export_status, ExportStatus::Running);

        // Backdrop click → close (only when not running)
        let backdrop_id = ui.id().with("export_backdrop");
        let backdrop_response = ui.interact(screen_rect, backdrop_id, egui::Sense::click());
        if backdrop_response.clicked() && !is_running {
            self.export_dialog_open = false;
        }

        // Close on Escape (only when not running)
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !is_running {
            self.export_dialog_open = false;
        }

        // ── Centered dialog ──
        let dialog_w = 440.0;
        let dialog_h = if is_running { 200.0 } else { 440.0 };
        let dialog_rect = egui::Rect::from_center_size(
            screen_rect.center(),
            Vec2::new(dialog_w, dialog_h),
        );

        // Dialog background
        ui.painter().rect_filled(dialog_rect, RADIUS_XL, BG_BASE);
        ui.painter().rect_stroke(
            dialog_rect,
            RADIUS_XL,
            Stroke::new(1.0, BORDER),
            egui::StrokeKind::Inside,
        );

        // Lock cursor inside dialog — prevents underlying widgets from changing it
        let dialog_block = ui.interact(
            dialog_rect,
            ui.id().with("export_dialog_block"),
            egui::Sense::hover(),
        );
        if dialog_block.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }

        if is_running {
            self.render_export_progress_overlay(ui, dialog_rect);
        } else {
            let content_rect = dialog_rect.shrink(SPACE_XL);
            let mut cursor_y = content_rect.top();

            // ── Title row ──
            ui.painter().text(
                egui::pos2(content_rect.left(), cursor_y + 14.0),
                egui::Align2::LEFT_CENTER,
                "Export",
                egui::FontId::new(FONT_SIZE_XL, egui::FontFamily::Proportional),
                TEXT_PRIMARY,
            );

            // Close button
            let close_size = Vec2::new(ROW_L, ROW_L);
            let close_rect = egui::Rect::from_min_size(
                egui::pos2(content_rect.right() - close_size.x, cursor_y),
                close_size,
            );
            let close_resp = ui.interact(close_rect, ui.id().with("export_close"), egui::Sense::click());
            let close_color = if close_resp.hovered() { TEXT_PRIMARY } else { TEXT_MUTED };
            ui.painter().text(
                close_rect.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::X,
                egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
                close_color,
            );
            if close_resp.clicked() {
                self.export_dialog_open = false;
            }

            cursor_y += 44.0;

            // Divider
            ui.painter().line_segment(
                [
                    egui::pos2(content_rect.left(), cursor_y),
                    egui::pos2(content_rect.right(), cursor_y),
                ],
                Stroke::new(1.0, BORDER),
            );
            cursor_y += SPACE_L;

            // ── Format tabs ──
            let tabs = [
                (ExportFormat::Image, egui_phosphor::regular::IMAGE, "Image"),
                (ExportFormat::Video, egui_phosphor::regular::FILM_STRIP, "Video"),
                (ExportFormat::Gif, egui_phosphor::regular::GIF, "GIF"),
            ];
            let tab_rect = egui::Rect::from_min_size(
                egui::pos2(content_rect.left(), cursor_y),
                Vec2::new(content_rect.width(), 30.0),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(tab_rect), |ui| {
                if let Some(new_fmt) = pill_tab_bar(ui, self.export_state.format, &tabs) {
                    self.export_state.format = new_fmt;
                    if self.export_state.output_path.is_empty() {
                        self.update_default_export_filename();
                    }
                }
            });
            cursor_y += 38.0;

            // ── Settings content ──
            let settings_rect = egui::Rect::from_min_size(
                egui::pos2(content_rect.left(), cursor_y),
                Vec2::new(content_rect.width(), content_rect.bottom() - cursor_y - 54.0),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(settings_rect), |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, SPACE_M);
                self.render_export_settings(ui);
            });

            // ── Status / action bar ──
            cursor_y = content_rect.bottom() - 44.0;
            let action_rect = egui::Rect::from_min_size(
                egui::pos2(content_rect.left(), cursor_y),
                Vec2::new(content_rect.width(), 40.0),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(action_rect), |ui| {
                self.render_export_action_bar(ui);
            });
        }
    }

    // ─── Progress Overlay ─────────────────────────────────────────────────────

    fn render_export_progress_overlay(
        &mut self,
        ui: &mut egui::Ui,
        dialog_rect: egui::Rect,
    ) {
        let content_rect = dialog_rect.shrink(SPACE_XL);
        let center_y = content_rect.center().y;
        let spinner_center = egui::pos2(content_rect.center().x, center_y - 30.0);

        // Animated spinner ring
        let time = ui.ctx().input(|i| i.time);
        let n_dots = 8;
        let radius = 14.0;
        let base_alpha = AMBER.a();
        for i in 0..n_dots {
            let angle = (i as f32 / n_dots as f32) * std::f32::consts::TAU - (time * 3.0) as f32;
            let pos = spinner_center
                + Vec2::new(angle.cos() * radius, angle.sin() * radius);
            let fade = ((i as f32) / (n_dots as f32 - 1.0)).clamp(0.0, 1.0);
            let alpha = (fade * base_alpha as f32) as u8;
            ui.painter().circle_filled(
                pos,
                2.5,
                Color32::from_rgba_premultiplied(AMBER.r(), AMBER.g(), AMBER.b(), alpha),
            );
        }

        // Status text
        ui.painter().text(
            egui::pos2(content_rect.center().x, center_y + 10.0),
            egui::Align2::CENTER_CENTER,
            "Exporting…",
            egui::FontId::new(FONT_SIZE_L, egui::FontFamily::Proportional),
            TEXT_PRIMARY,
        );

        // Subtitle
        let format_label = match self.export_state.format {
            ExportFormat::Image => "Rendering single frame",
            ExportFormat::Video => "Rendering video frames",
            ExportFormat::Gif => "Rendering GIF frames",
        };
        ui.painter().text(
            egui::pos2(content_rect.center().x, center_y + 32.0),
            egui::Align2::CENTER_CENTER,
            format_label,
            egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            TEXT_MUTED,
        );

        // Cancel button
        let btn_size = Vec2::new(100.0, 32.0);
        let btn_rect = egui::Rect::from_center_size(
            egui::pos2(content_rect.center().x, content_rect.bottom() - 20.0),
            btn_size,
        );
        let btn_resp = ui.interact(btn_rect, ui.id().with("export_cancel"), egui::Sense::click());
        let btn_bg = if btn_resp.hovered() { BG_HOVER } else { BG_WIDGET };
        ui.painter().rect_filled(btn_rect, RADIUS_M, btn_bg);
        ui.painter().rect_stroke(btn_rect, RADIUS_M, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
        ui.painter().text(
            btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Cancel",
            egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
            if btn_resp.hovered() { TEXT_PRIMARY } else { TEXT_SECONDARY },
        );
        if btn_resp.clicked() {
            self.export_dialog_open = false;
            // Note: the export thread continues in background;
            // we just stop showing the dialog. Future work: add
            // cancellation token to renderer.
        }
    }

    // ─── Settings Form ────────────────────────────────────────────────────────

    fn render_export_settings(&mut self, ui: &mut egui::Ui) {
        // Resolution row with label aligned left
        ui.horizontal(|ui| {
            ui.label(RichText::new("Resolution").size(FONT_SIZE_S).color(TEXT_SECONDARY));
            ui.add_space(SPACE_L);

            let mut w = self.export_state.width as f32;
            ui.add(egui::DragValue::new(&mut w).speed(10.0).range(1.0..=8192.0).prefix("W: "));
            self.export_state.width = w as u32;

            ui.add_space(SPACE_S);

            let mut h = self.export_state.height as f32;
            ui.add(egui::DragValue::new(&mut h).speed(10.0).range(1.0..=8192.0).prefix("H: "));
            self.export_state.height = h as u32;

            ui.add_space(SPACE_S);

            // Use scene dimensions button
            let scene_w = self.document.scene_dimensions.width;
            let scene_h = self.document.scene_dimensions.height;
            if scene_w > 0 && scene_h > 0 {
                let resp = ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} Scene", egui_phosphor::regular::ARROWS_IN))
                            .size(FONT_SIZE_S)
                            .color(ACCENT_BLUE),
                    )
                    .selectable(false),
                );
                if resp.interact(egui::Sense::click()).clicked() {
                    self.export_state.width = scene_w;
                    self.export_state.height = scene_h;
                }
            }
        });

        ui.add_space(SPACE_XS);

        // Format-specific settings
        match self.export_state.format {
            ExportFormat::Image => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Time").size(FONT_SIZE_S).color(TEXT_SECONDARY));
                    ui.add_space(SPACE_L + 18.0); // align with resolution inputs
                    let mut t = self.export_state.time_s;
                    let max_time = self.preview.duration_s as f32;
                    ui.add(
                        egui::DragValue::new(&mut t)
                            .speed(0.1)
                            .range(0.0..=max_time)
                            .suffix(" s"),
                    );
                    self.export_state.time_s = t;

                    ui.add_space(SPACE_S);

                    let current_time = self.preview.current_time_s as f32;
                    let resp = ui.add(
                        egui::Label::new(
                            RichText::new(format!("{} Current", egui_phosphor::regular::CLOCK))
                                .size(FONT_SIZE_S)
                                .color(ACCENT_BLUE),
                        )
                        .selectable(false),
                    );
                    if resp.interact(egui::Sense::click()).clicked() {
                        self.export_state.time_s = current_time;
                    }
                });
            }
            ExportFormat::Video | ExportFormat::Gif => {
                // FPS
                ui.horizontal(|ui| {
                    ui.label(RichText::new("FPS").size(FONT_SIZE_S).color(TEXT_SECONDARY));
                    ui.add_space(SPACE_L + 24.0);
                    let mut fps = self.export_state.fps as f32;
                    ui.add(
                        egui::DragValue::new(&mut fps)
                            .speed(1.0)
                            .range(1.0..=120.0)
                            .suffix(" fps"),
                    );
                    self.export_state.fps = fps as u32;
                });

                // Duration mode
                let auto_prev = self.export_state.auto_duration;
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Duration").size(FONT_SIZE_S).color(TEXT_SECONDARY));
                    ui.add_space(SPACE_L + 2.0);

                    ui.checkbox(
                        &mut self.export_state.auto_duration,
                        RichText::new("Auto").size(FONT_SIZE_S),
                    );

                    if self.export_state.auto_duration {
                        ui.add_space(SPACE_S);
                        ui.label(RichText::new("Hold:").size(FONT_SIZE_S).color(TEXT_SECONDARY));
                        let mut hold = self.export_state.hold_s;
                        ui.add(
                            egui::DragValue::new(&mut hold)
                                .speed(0.1)
                                .range(0.0..=10.0)
                                .suffix(" s"),
                        );
                        self.export_state.hold_s = hold;
                    } else {
                        ui.add_space(SPACE_S);
                        let mut dur = self.export_state.duration_s;
                        ui.add(
                            egui::DragValue::new(&mut dur)
                                .speed(0.5)
                                .range(0.1..=3600.0)
                                .suffix(" s"),
                        );
                        self.export_state.duration_s = dur;
                    }
                });

                if !auto_prev && self.export_state.auto_duration {
                    // Just switched to auto
                } else if auto_prev && !self.export_state.auto_duration {
                    let auto_dur = self.resolve_auto_duration();
                    self.export_state.duration_s = auto_dur;
                }

                if self.export_state.auto_duration {
                    let auto_dur = self.resolve_auto_duration();
                    ui.label(
                        RichText::new(format!("Effective duration: {:.2}s", auto_dur))
                            .size(FONT_SIZE_XS)
                            .color(TEXT_MUTED),
                    );
                }
            }
        }

        ui.add_space(SPACE_S);

        // Output path
        ui.horizontal(|ui| {
            ui.label(RichText::new("Output").size(FONT_SIZE_S).color(TEXT_SECONDARY));
            ui.add_space(SPACE_L + 14.0);

            let path_width = ui.available_width();
            ui.add_sized(
                Vec2::new(path_width, ROW_L),
                egui::TextEdit::singleline(&mut self.export_state.output_path)
                    .hint_text("output filename…"),
            );
        });

        // Default filename hint
        if self.export_state.output_path.is_empty() {
            let default = self.suggest_export_filename();
            ui.label(
                RichText::new(format!("Default: {}", default.display()))
                    .size(FONT_SIZE_XS)
                    .color(TEXT_MUTED),
            );
        }
    }

    // ─── Action Bar ───────────────────────────────────────────────────────────

    fn render_export_action_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            // Status message (left side)
            match &self.export_status {
                ExportStatus::Idle => {}
                ExportStatus::Complete { path } => {
                    let path_str = path.display().to_string();
                    let label = if path_str.len() > 35 {
                        format!("{}…{}", &path_str[..15], &path_str[path_str.len()-15..])
                    } else {
                        path_str
                    };
                    ui.label(
                        RichText::new(format!("{} {}", egui_phosphor::regular::CHECK, label))
                            .size(FONT_SIZE_S)
                            .color(GREEN),
                    );
                }
                ExportStatus::Failed(err) => {
                    let truncated = if err.len() > 40 {
                        format!("{}…", &err[..37])
                    } else {
                        err.clone()
                    };
                    ui.label(
                        RichText::new(format!("{} {}", egui_phosphor::regular::WARNING, truncated))
                            .size(FONT_SIZE_S)
                            .color(RED),
                    );
                }
                ExportStatus::Running => {}
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Export button
                let btn_text = match self.export_state.format {
                    ExportFormat::Image => "Export Image",
                    ExportFormat::Video => "Export Video",
                    ExportFormat::Gif => "Export GIF",
                };
                let btn_size = Vec2::new(120.0, 32.0);
                let (btn_rect, btn_resp) = ui.allocate_exact_size(btn_size, egui::Sense::click());

                let btn_bg = if btn_resp.hovered() {
                    Color32::from_rgb(220, 170, 60)
                } else {
                    AMBER
                };

                ui.painter().rect_filled(btn_rect, RADIUS_M, btn_bg);
                ui.painter().text(
                    btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    btn_text,
                    egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
                    BG_BASE,
                );

                if btn_resp.clicked() {
                    self.start_export();
                }
            });
        });
    }

    // ─── Helpers ──────────────────────────────────────────────────────────────

    fn resolve_auto_duration(&self) -> f32 {
        if let Some(timeline) = &self.document.timeline {
            let d = timeline.duration_seconds() as f32 + self.export_state.hold_s.max(0.0);
            d.max(0.5)
        } else {
            self.export_state.duration_s.max(0.5)
        }
    }

    fn suggest_export_filename(&self) -> PathBuf {
        let ext = match self.export_state.format {
            ExportFormat::Image => "png",
            ExportFormat::Video => "mp4",
            ExportFormat::Gif => "gif",
        };
        let stem = self
            .document
            .file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("animatix");
        let workspace = self
            .document
            .file_path
            .parent()
            .unwrap_or(std::path::Path::new("."));
        workspace.join(format!("{}_export.{ext}", stem))
    }

    pub(crate) fn update_default_export_filename(&mut self) {
        let path = self.suggest_export_filename();
        self.export_state.output_path = path.to_string_lossy().to_string();
    }

    fn start_export(&mut self) {
        let timeline = match self.document.timeline.clone() {
            Some(t) => t,
            None => {
                self.export_status = ExportStatus::Failed("No timeline to export".into());
                return;
            }
        };

        let state = self.export_state.clone();
        let output_path = if state.output_path.is_empty() {
            self.suggest_export_filename()
        } else {
            PathBuf::from(&state.output_path)
        };

        let debug = animatix::timeline::DebugRenderOptions {
            draw_bounds: self.debug_bounds,
        };

        self.export_status = ExportStatus::Running;

        let result_path = output_path.clone();
        let handle = std::thread::spawn(move || {
            let result = match state.format {
                ExportFormat::Image => animatix::renderer::render_image_timeline_with_debug(
                    timeline,
                    state.width,
                    state.height,
                    state.time_s,
                    &output_path,
                    debug,
                ),
                ExportFormat::Video => {
                    let duration = if state.auto_duration {
                        let d = timeline.duration_seconds() as f32 + state.hold_s.max(0.0);
                        d.max(0.5)
                    } else {
                        state.duration_s
                    };
                    animatix::renderer::render_video_timeline_with_debug(
                        timeline, state.width, state.height, state.fps, duration, &output_path, debug,
                    )
                }
                ExportFormat::Gif => {
                    let duration = if state.auto_duration {
                        let d = timeline.duration_seconds() as f32 + state.hold_s.max(0.0);
                        d.max(0.5)
                    } else {
                        state.duration_s
                    };
                    animatix::renderer::render_gif_timeline_with_debug(
                        timeline, state.width, state.height, state.fps, duration, &output_path, debug,
                    )
                }
            };
            (result, result_path)
        });
        self.export_thread = Some(handle);
    }

    /// Call this every frame to check if an export thread finished.
    pub(crate) fn poll_export_status(&mut self) {
        if let Some(handle) = self.export_thread.take() {
            if handle.is_finished() {
                match handle.join() {
                    Ok((Ok(()), path)) => {
                        self.export_status = ExportStatus::Complete { path };
                    }
                    Ok((Err(e), _)) => {
                        self.export_status = ExportStatus::Failed(e.to_string());
                    }
                    Err(_) => {
                        self.export_status = ExportStatus::Failed("Export thread panicked".into());
                    }
                }
            } else {
                self.export_thread = Some(handle);
            }
        }
    }
}
