use std::path::PathBuf;
use std::sync::Arc;

use egui::{Color32, RichText, Stroke, Vec2};
use eparts::{Button, Checkbox, NumberField, ProgressBar, TextField};

use crate::app::GuiShell;
use crate::app::components::{TabBar, Tag, layout};
use crate::app::design_tokens::spatial::{RADIUS_M, RADIUS_XL, STROKE_WIDTH};
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
    /// Video encoder selection, shared with CLI export presets.
    pub(crate) video_codec: animatix_render::encode::VideoCodec,
    /// libx264 speed preset, shared with CLI export presets.
    pub(crate) h264_preset: animatix_render::encode::H264Preset,
}

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
            width: 1280,
            height: 720,
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
        ui.painter().rect_filled(screen_rect, 0.0, theme.palette.overlay.backdrop);

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

        // Dialog elevation shadow & background
        ui.painter().add(theme.elevation.overlay.as_shape(dialog_rect, RADIUS_XL));
        ui.painter().rect_filled(dialog_rect, RADIUS_XL, theme.palette.surface.base);
        ui.painter().rect_stroke(
            dialog_rect,
            RADIUS_XL,
            Stroke::new(STROKE_WIDTH, theme.palette.border.default),
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
                    ui.add(Button::icon(egui_phosphor::regular::X).with_tooltip("Close (Esc)"))
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
            let labels: Vec<&str> = formats
                .iter()
                .map(|format| match format {
                    ExportFormat::Image => "Image",
                    ExportFormat::Video => "Video",
                    ExportFormat::Gif => "GIF",
                    ExportFormat::WebM => "WebM",
                    ExportFormat::Mov => "MOV",
                    ExportFormat::WebP => "WebP",
                })
                .collect();
            let mut selected = formats
                .iter()
                .position(|format| *format == self.export_store.export_state.format)
                .unwrap_or(0);
            let tab_rect = egui::Rect::from_min_size(
                egui::pos2(content_rect.left(), cursor_y),
                Vec2::new(content_rect.width(), sp.base.component.pill_tab_height),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(tab_rect), |ui| {
                TabBar::new("export_format_tabs", &mut selected, &labels).show(ui);
            });
            let previous_format = self.export_store.export_state.format;
            if let Some(new_fmt) = formats.get(selected).copied() {
                self.export_store.export_state.format = new_fmt;
                if self.export_store.export_state.output_path.is_empty()
                    && previous_format != new_fmt
                {
                    self.update_default_export_filename();
                }
            }
            cursor_y += sp.base.component.pill_tab_height + sp.base.space_3;

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
}

mod actions;
mod progress;
mod settings;
#[cfg(test)]
mod tests;
