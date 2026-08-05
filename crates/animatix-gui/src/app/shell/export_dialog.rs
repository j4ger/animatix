use std::path::PathBuf;
use std::sync::Arc;

use egui::{Color32, RichText, Stroke, Vec2};

use crate::app::GuiShell;
use crate::app::components::layout;
use crate::app::design_tokens::spatial::{RADIUS_M, RADIUS_S, RADIUS_XL, STROKE_WIDTH};
use crate::app::design_tokens::typography::TextRole;
use crate::app::document::export_target::ExportScope;
use crate::app::utils::text::{truncate_chars, truncate_middle};

// ─── Export Configuration ───────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportFormat {
    Image,
    Video,
    Gif,
    WebM,
    Mov,
    WebP,
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
    /// Export scope: ActiveScene or WholeComposition.
    pub(crate) export_scope: ExportScope,
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
            export_scope: ExportScope::ActiveScene,
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
        let theme = eparts::theme(ui);
        let sp = crate::app::design_tokens::spatial::spatial(ui);

        let screen_rect = ui.ctx().viewport_rect();

        // Dark semi-transparent backdrop
        ui.painter().rect_filled(screen_rect, 0.0, theme.overlay.backdrop);

        let is_running = matches!(self.export_store.export_status, ExportStatus::Running);

        // Backdrop click → close (only when not running)
        let backdrop_id = ui.id().with("export_backdrop");
        let backdrop_response = ui.interact(screen_rect, backdrop_id, egui::Sense::click());
        if backdrop_response.clicked() && !is_running {
            self.export_store.export_dialog_open = false;
        }

        // Close on Escape (only when not running)
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) && !is_running {
            self.export_store.export_dialog_open = false;
        }

        // ── Responsive centered dialog ──
        let min_w = 360.0;
        let max_w = 560.0;
        let min_h = if is_running { 180.0 } else { 380.0 };
        let max_h = if is_running { 260.0 } else { 580.0 };
        let dialog_w = (screen_rect.width() * 0.45).clamp(min_w, max_w);
        let dialog_h = if is_running {
            (screen_rect.height() * 0.28).clamp(min_h, max_h)
        } else {
            (screen_rect.height() * 0.55).clamp(min_h, max_h)
        };
        let dialog_rect =
            egui::Rect::from_center_size(screen_rect.center(), Vec2::new(dialog_w, dialog_h));

        // Dialog background
        ui.painter().rect_filled(dialog_rect, RADIUS_XL, theme.surface.base);
        ui.painter().rect_stroke(
            dialog_rect,
            RADIUS_XL,
            Stroke::new(STROKE_WIDTH, theme.border.default),
            egui::StrokeKind::Inside,
        );

        // Lock cursor inside dialog — prevents underlying widgets from changing it
        let dialog_block =
            ui.interact(dialog_rect, ui.id().with("export_dialog_block"), egui::Sense::hover());
        if dialog_block.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
        }

        if is_running {
            self.render_export_progress_overlay(ui, dialog_rect);
        } else {
            let content_rect = dialog_rect.shrink(sp.base.space_5);
            let mut cursor_y = content_rect.top();

            // ── Title row ──
            ui.painter().text(
                egui::pos2(content_rect.left(), cursor_y + 14.0),
                egui::Align2::LEFT_CENTER,
                "Export",
                TextRole::Heading.font_id(),
                theme.text.primary,
            );

            // Close button
            let close_size = Vec2::new(sp.base.row_l, sp.base.row_l);
            let close_rect = egui::Rect::from_min_size(
                egui::pos2(content_rect.right() - close_size.x, cursor_y),
                close_size,
            );
            let close_resp =
                ui.interact(close_rect, ui.id().with("export_close"), egui::Sense::click());
            let close_color = if close_resp.hovered() {
                theme.text.primary
            } else {
                theme.text.muted
            };
            ui.painter().text(
                close_rect.center(),
                egui::Align2::CENTER_CENTER,
                egui_phosphor::regular::X,
                TextRole::Body.font_id(),
                close_color,
            );
            if close_resp.clicked() {
                self.export_store.export_dialog_open = false;
            }

            cursor_y += sp.base.row_l + sp.base.space_4;

            // Divider
            ui.painter().line_segment(
                [
                    egui::pos2(content_rect.left(), cursor_y),
                    egui::pos2(content_rect.right(), cursor_y),
                ],
                Stroke::new(STROKE_WIDTH, theme.border.default),
            );
            cursor_y += sp.base.space_4;

            // ── Format tabs ──
            let tabs = [
                (ExportFormat::Image, egui_phosphor::regular::IMAGE, "Image"),
                (ExportFormat::Video, egui_phosphor::regular::FILM_STRIP, "Video"),
                (ExportFormat::Gif, egui_phosphor::regular::GIF, "GIF"),
                (ExportFormat::WebM, egui_phosphor::regular::FILM_STRIP, "WebM"),
                (ExportFormat::Mov, egui_phosphor::regular::FILM_STRIP, "MOV"),
                (ExportFormat::WebP, egui_phosphor::regular::IMAGE, "WebP"),
            ];
            let tab_rect = egui::Rect::from_min_size(
                egui::pos2(content_rect.left(), cursor_y),
                Vec2::new(content_rect.width(), sp.base.row_m),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(tab_rect), |ui| {
                if let Some(new_fmt) =
                    layout::pill_tab_bar(ui, self.export_store.export_state.format, &tabs)
                {
                    self.export_store.export_state.format = new_fmt;
                    if self.export_store.export_state.output_path.is_empty() {
                        self.update_default_export_filename();
                    }
                }
            });
            cursor_y += sp.base.row_m + sp.base.space_3;

            // ── Settings content ──
            let settings_rect = egui::Rect::from_min_size(
                egui::pos2(content_rect.left(), cursor_y),
                Vec2::new(content_rect.width(), content_rect.bottom() - cursor_y - 54.0),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(settings_rect), |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, sp.base.space_3);
                self.render_export_settings(ui);
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
    }

    // ─── Progress Overlay ─────────────────────────────────────────────────────

    fn render_export_progress_overlay(&mut self, ui: &mut egui::Ui, dialog_rect: egui::Rect) {
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
        let base_alpha = theme.status.warning.a();
        for i in 0..n_dots {
            let angle = (i as f32 / n_dots as f32) * std::f32::consts::TAU - (time * 3.0) as f32;
            let pos = spinner_center + Vec2::new(angle.cos() * radius, angle.sin() * radius);
            let fade = ((i as f32) / (n_dots as f32 - 1.0)).clamp(0.0, 1.0);
            let alpha = (fade * base_alpha as f32) as u8;
            ui.painter().circle_filled(
                pos,
                2.5,
                Color32::from_rgba_premultiplied(
                    theme.status.warning.r(),
                    theme.status.warning.g(),
                    theme.status.warning.b(),
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
            theme.text.primary,
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
            theme.text.muted,
        );

        // Progress bar + frame count
        let progress = self.export_store.export_progress.load(std::sync::atomic::Ordering::Relaxed);
        let total = self.export_store.export_total_frames.max(1);
        let pct = (progress as f32 / total as f32).clamp(0.0, 1.0);

        let bar_w = content_rect.width().min(280.0);
        let bar_h = 6.0;
        let bar_y = center_y + 50.0;
        let bar_rect = egui::Rect::from_center_size(
            egui::pos2(content_rect.center().x, bar_y),
            Vec2::new(bar_w, bar_h),
        );
        // Track
        ui.painter().rect_filled(bar_rect, bar_h * 0.5, theme.surface.widget);
        // Fill
        if pct > 0.0 {
            let fill_rect = egui::Rect::from_min_size(bar_rect.min, Vec2::new(bar_w * pct, bar_h));
            ui.painter().rect_filled(fill_rect, bar_h * 0.5, theme.status.warning);
        }

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
            theme.text.muted,
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
                theme.text.muted,
            );
        }

        // Cancel button
        let btn_size = Vec2::new(100.0, sp.base.row_m);
        let btn_rect = egui::Rect::from_center_size(
            egui::pos2(content_rect.center().x, content_rect.bottom() - 20.0),
            btn_size,
        );
        let btn_resp = ui.interact(btn_rect, ui.id().with("export_cancel"), egui::Sense::click());
        let btn_bg = if btn_resp.hovered() {
            theme.surface.hover
        } else {
            theme.surface.widget
        };
        ui.painter().rect_filled(btn_rect, RADIUS_M, btn_bg);
        ui.painter().rect_stroke(
            btn_rect,
            RADIUS_M,
            Stroke::new(STROKE_WIDTH, theme.border.default),
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Cancel",
            TextRole::Body.font_id(),
            if btn_resp.hovered() {
                theme.text.primary
            } else {
                theme.text.secondary
            },
        );
        if btn_resp.clicked() {
            self.export_store
                .export_cancelled
                .store(true, std::sync::atomic::Ordering::Relaxed);
            self.export_store.export_status = ExportStatus::Idle;
            self.export_store.export_dialog_open = false;
        }
    }

    // ─── Settings Form ────────────────────────────────────────────────────────

    fn render_export_settings(&mut self, ui: &mut egui::Ui) {
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
            let width = &mut self.export_store.export_state.width;
            let height = &mut self.export_store.export_state.height;
            let time_s = &mut self.export_store.export_state.time_s;
            let fps = &mut self.export_store.export_state.fps;
            let auto_duration = &mut self.export_store.export_state.auto_duration;
            let hold_s = &mut self.export_store.export_state.hold_s;
            let duration_s = &mut self.export_store.export_state.duration_s;
            let output_path = &mut self.export_store.export_state.output_path;

            // ── Resolution row ──
            Self::settings_row(ui, "Resolution", |ui| {
                let mut w_f32 = *width as f32;
                layout::field_sized(ui, Some(78.0), |ui| {
                    ui.add(
                        egui::DragValue::new(&mut w_f32)
                            .speed(10.0)
                            .range(1.0..=8192.0)
                            .prefix("W: "),
                    );
                });
                *width = w_f32 as u32;

                ui.add_space(sp.base.space_2);

                let mut h_f32 = *height as f32;
                layout::field_sized(ui, Some(78.0), |ui| {
                    ui.add(
                        egui::DragValue::new(&mut h_f32)
                            .speed(10.0)
                            .range(1.0..=8192.0)
                            .prefix("H: "),
                    );
                });
                *height = h_f32 as u32;

                ui.add_space(sp.base.space_2);

                if scene_dims.width > 0 && scene_dims.height > 0 {
                    let resp = ui.add(
                        egui::Label::new(
                            RichText::new(format!("{} Scene", egui_phosphor::regular::ARROWS_IN))
                                .size(TextRole::BodyS.size())
                                .color(theme.accent.primary),
                        )
                        .selectable(false),
                    );
                    if resp.interact(egui::Sense::click()).clicked() {
                        *width = scene_dims.width;
                        *height = scene_dims.height;
                    }
                }
            });

            ui.add_space(sp.base.space_1);

            // ── Quality presets ──
            Self::settings_row(ui, "Presets", |ui| {
                let presets = [
                    ("720p / 30", 1280, 720, 30),
                    ("1080p / 30", 1920, 1080, 30),
                    ("1080p / 60", 1920, 1080, 60),
                    ("4K / 60", 3840, 2160, 60),
                ];
                for (label, w, h, f) in presets {
                    let resp = ui.add(
                        egui::Button::new(
                            RichText::new(label)
                                .size(TextRole::Micro.size())
                                .color(theme.text.secondary),
                        )
                        .fill(theme.surface.widget)
                        .stroke(Stroke::new(STROKE_WIDTH, theme.border.default))
                        .corner_radius(RADIUS_S)
                        .small(),
                    );
                    if resp.clicked() {
                        *width = w;
                        *height = h;
                        *fps = f;
                    }
                }
            });

            ui.add_space(sp.base.space_1);

            // ── Format-specific settings ──
            match format {
                ExportFormat::Image | ExportFormat::WebP => {
                    Self::settings_row(ui, "Time", |ui| {
                        let mut t = *time_s;
                        layout::field_sized(ui, Some(100.0), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut t)
                                    .speed(0.1)
                                    .range(0.0..=max_time)
                                    .suffix(" s"),
                            );
                        });
                        *time_s = t;

                        ui.add_space(sp.base.space_2);

                        let resp = ui.add(
                            egui::Label::new(
                                RichText::new(format!("{} Current", egui_phosphor::regular::CLOCK))
                                    .size(TextRole::BodyS.size())
                                    .color(theme.accent.primary),
                            )
                            .selectable(false),
                        );
                        if resp.interact(egui::Sense::click()).clicked() {
                            *time_s = current_time;
                        }
                    });
                },
                ExportFormat::Video
                | ExportFormat::Gif
                | ExportFormat::WebM
                | ExportFormat::Mov => {
                    // FPS
                    Self::settings_row(ui, "FPS", |ui| {
                        let mut fps_f32 = *fps as f32;
                        layout::field_sized(ui, Some(70.0), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut fps_f32)
                                    .speed(1.0)
                                    .range(1.0..=120.0)
                                    .suffix(" fps"),
                            );
                        });
                        *fps = fps_f32 as u32;
                    });

                    // Duration mode
                    let auto_prev = *auto_duration;
                    Self::settings_row(ui, "Duration", |ui| {
                        ui.checkbox(
                            auto_duration,
                            RichText::new("Auto").size(TextRole::BodyS.size()),
                        );

                        if *auto_duration {
                            ui.add_space(sp.base.space_2);
                            ui.label(
                                RichText::new("Hold:")
                                    .size(TextRole::BodyS.size())
                                    .color(theme.text.secondary),
                            );

                            let mut hold = *hold_s;
                            layout::field_sized(ui, Some(80.0), |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut hold)
                                        .speed(0.1)
                                        .range(0.0..=10.0)
                                        .suffix(" s"),
                                );
                            });
                            *hold_s = hold;
                        } else {
                            ui.add_space(sp.base.space_2);

                            let mut dur = *duration_s;
                            layout::field_sized(ui, Some(80.0), |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut dur)
                                        .speed(0.5)
                                        .range(0.1..=3600.0)
                                        .suffix(" s"),
                                );
                            });
                            *duration_s = dur;
                        }
                    });

                    if !auto_prev && *auto_duration {
                        // Just switched to auto
                    } else if auto_prev && !*auto_duration {
                        let auto_dur = if let Some(dur) = timeline_duration {
                            (dur + hold_s.max(0.0)).max(0.5)
                        } else {
                            duration_s.max(0.5)
                        };
                        *duration_s = auto_dur;
                    }

                    if *auto_duration {
                        let auto_dur = if let Some(dur) = timeline_duration {
                            (dur + hold_s.max(0.0)).max(0.5)
                        } else {
                            duration_s.max(0.5)
                        };
                        ui.label(
                            RichText::new(format!("Effective duration: {:.2}s", auto_dur))
                                .size(TextRole::Micro.size())
                                .color(theme.text.muted),
                        );
                    }
                },
            }

            ui.add_space(sp.base.space_2);

            // ── Output path ──
            Self::settings_row(ui, "Output", |ui| {
                let path_width = ui.available_width();
                layout::field_sized(ui, Some(path_width), |ui| {
                    ui.add(egui::TextEdit::singleline(output_path).hint_text("output filename…"));
                });
            });
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
                    .color(theme.text.muted),
            );
        }
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
                                .color(theme.text.secondary),
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

    fn render_export_action_bar(&mut self, ui: &mut egui::Ui) {
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
                        egui::Label::new(
                            RichText::new(format!("{} {}", egui_phosphor::regular::CHECK, label))
                                .size(TextRole::BodyS.size())
                                .color(theme.status.success),
                        )
                        .selectable(false),
                    );
                    if resp.interact(egui::Sense::click()).clicked() {
                        self.export_store.export_status = ExportStatus::Idle;
                    }
                },
                ExportStatus::Failed(err) => {
                    let truncated = truncate_chars(err, 37);
                    let resp = ui.add(
                        egui::Label::new(
                            RichText::new(format!(
                                "{} {}",
                                egui_phosphor::regular::WARNING,
                                truncated
                            ))
                            .size(TextRole::BodyS.size())
                            .color(theme.status.error),
                        )
                        .selectable(false),
                    );
                    if resp.interact(egui::Sense::click()).clicked() {
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

                let btn_bg = theme.status.warning;

                ui.painter().rect_filled(btn_rect, RADIUS_M, btn_bg);
                ui.painter().text(
                    btn_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    btn_text,
                    TextRole::Body.font_id(),
                    theme.surface.base,
                );

                if btn_resp.clicked() {
                    self.start_export();
                }
            });
        });
    }

    fn suggest_export_filename(&self) -> PathBuf {
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

    fn start_export(&mut self) {
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
            let result: Result<(), animatix::renderer::ExportError> = match state.format {
                ExportFormat::Image | ExportFormat::WebP => {
                    // Ungated image path — no FFmpeg needed.
                    if has_composition {
                        match &cloned_target {
                            crate::app::document::export_target::ExportTargetOwned::Composition(
                                comp,
                            ) => animatix::renderer::render_image_composition(
                                comp,
                                state.width,
                                state.height,
                                state.time_s,
                                &output_path,
                            ),
                            _ => Err(animatix::renderer::ExportError::Internal(format!(
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
                                animatix::renderer::render_image_timeline_with_progress(
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
                            _ => Err(animatix::renderer::ExportError::Internal(format!(
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
                            ) => animatix::renderer::render_video_composition_with_progress(
                                comp,
                                state.width,
                                state.height,
                                state.fps,
                                duration,
                                &output_path,
                                debug,
                                animatix::renderer::ExportSettings::default(),
                                progress_ref,
                                cancel_ref,
                            ),
                            _ => Err(animatix::renderer::ExportError::Internal(format!(
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
                                animatix::renderer::render_video_timeline_with_progress(
                                    timeline,
                                    state.width,
                                    state.height,
                                    state.fps,
                                    duration,
                                    &output_path,
                                    debug,
                                    animatix::renderer::ExportSettings::default(),
                                    progress_ref,
                                    cancel_ref,
                                )
                            },
                            _ => Err(animatix::renderer::ExportError::Internal(format!(
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
                            ) => animatix::renderer::render_video_composition_with_progress(
                                comp,
                                state.width,
                                state.height,
                                state.fps,
                                duration,
                                &output_path,
                                debug,
                                animatix::renderer::ExportSettings {
                                    video_codec: animatix::renderer::VideoCodec::Vp9,
                                    ..Default::default()
                                },
                                progress_ref,
                                cancel_ref,
                            ),
                            _ => Err(animatix::renderer::ExportError::Internal(format!(
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
                                animatix::renderer::render_video_timeline_with_progress(
                                    timeline,
                                    state.width,
                                    state.height,
                                    state.fps,
                                    duration,
                                    &output_path,
                                    debug,
                                    animatix::renderer::ExportSettings {
                                        video_codec: animatix::renderer::VideoCodec::Vp9,
                                        ..Default::default()
                                    },
                                    progress_ref,
                                    cancel_ref,
                                )
                            },
                            _ => Err(animatix::renderer::ExportError::Internal(format!(
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
                            ) => animatix::renderer::render_video_composition_with_progress(
                                comp,
                                state.width,
                                state.height,
                                state.fps,
                                duration,
                                &output_path,
                                debug,
                                animatix::renderer::ExportSettings::default(),
                                progress_ref,
                                cancel_ref,
                            ),
                            _ => Err(animatix::renderer::ExportError::Internal(format!(
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
                                animatix::renderer::render_video_timeline_with_progress(
                                    timeline,
                                    state.width,
                                    state.height,
                                    state.fps,
                                    duration,
                                    &output_path,
                                    debug,
                                    animatix::renderer::ExportSettings::default(),
                                    progress_ref,
                                    cancel_ref,
                                )
                            },
                            _ => Err(animatix::renderer::ExportError::Internal(format!(
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
                            ) => animatix::renderer::render_gif_composition_with_progress(
                                comp,
                                state.width,
                                state.height,
                                state.fps,
                                duration,
                                &output_path,
                                debug,
                                animatix::renderer::ExportSettings::default(),
                                progress_ref,
                                cancel_ref,
                            ),
                            _ => Err(animatix::renderer::ExportError::Internal(format!(
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
                                animatix::renderer::render_gif_timeline_with_progress(
                                    timeline,
                                    state.width,
                                    state.height,
                                    state.fps,
                                    duration,
                                    &output_path,
                                    debug,
                                    animatix::renderer::ExportSettings::default(),
                                    progress_ref,
                                    cancel_ref,
                                )
                            },
                            _ => Err(animatix::renderer::ExportError::Internal(format!(
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
                | ExportFormat::Gif => Err(animatix::renderer::ExportError::Internal(
                    "This format requires the 'video' feature (FFmpeg)".into(),
                )),
            };
            (result, result_path)
        });
        self.export_store.export_thread = Some(handle);
    }
}
