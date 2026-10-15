//! Editor panel: source code editor with diagnostics and scrub-to commands.

use animatix_syntax::diagnostics::Diagnostic;
use egui::Vec2;

use crate::app::commands::{ActionQueue, PlaybackCommand};
use crate::app::design_tokens::typography::TextRole;
use crate::app::panels::panel_frame;
use crate::editor::EditorBuffer;

pub(crate) struct EditorContext<'a> {
    pub editor: &'a mut EditorBuffer,
    pub diagnostics: &'a [Diagnostic],
    pub source_dirty: &'a mut String,
    pub commands: &'a mut ActionQueue,
    pub is_playing: bool,
}

pub(crate) fn editor_ui(ctx: &mut EditorContext<'_>, ui: &mut egui::Ui) {
    let theme = eparts::theme(ui);

    panel_frame().show(ui, |ui| {
        // ── Toolbar Header ──
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);

            // Title
            ui.label(
                egui::RichText::new(egui_phosphor::regular::CODE)
                    .size(TextRole::Body.size())
                    .color(theme.palette.accent.primary),
            );
            ui.label(
                egui::RichText::new("Editor")
                    .size(TextRole::Body.size())
                    .strong()
                    .color(theme.palette.text.primary),
            );

            // Diagnostics status pill
            let error_count = ctx
                .diagnostics
                .iter()
                .filter(|d| d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error)
                .count();
            let warning_count = ctx
                .diagnostics
                .iter()
                .filter(|d| {
                    matches!(d.severity, animatix_syntax::diagnostics::DiagnosticSeverity::Warning)
                })
                .count();

            if error_count > 0 {
                let text = if error_count == 1 {
                    "1 error".to_string()
                } else {
                    format!("{error_count} errors")
                };
                let btn = eparts::Button::ghost(text)
                    .with_icon(egui_phosphor::regular::WARNING_CIRCLE)
                    .icon_color(theme.palette.status.error)
                    .with_size(eparts::Size::Sm);
                if ui.add(btn).on_hover_text("Click to jump to first error").clicked() {
                    if let Some(err) = ctx.diagnostics.iter().find(|d| {
                        d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error
                    }) {
                        let line = err.location.line.unwrap_or(1).saturating_sub(1);
                        let col = err.location.column.unwrap_or(1).saturating_sub(1);
                        ctx.editor.focus_diagnostic(line, col);
                    }
                }
            } else if warning_count > 0 {
                let text = if warning_count == 1 {
                    "1 warning".to_string()
                } else {
                    format!("{warning_count} warnings")
                };
                let btn = eparts::Button::ghost(text)
                    .with_icon(egui_phosphor::regular::WARNING)
                    .icon_color(theme.palette.status.warning)
                    .with_size(eparts::Size::Sm);
                if ui.add(btn).on_hover_text("Click to jump to first warning").clicked() {
                    if let Some(warn) = ctx.diagnostics.iter().find(|d| {
                        d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Warning
                    }) {
                        let line = warn.location.line.unwrap_or(1).saturating_sub(1);
                        let col = warn.location.column.unwrap_or(1).saturating_sub(1);
                        ctx.editor.focus_diagnostic(line, col);
                    }
                }
            } else {
                ui.label(
                    egui::RichText::new(format!("{} Ready", egui_phosphor::regular::CHECK_CIRCLE))
                        .size(TextRole::Caption.size())
                        .color(theme.palette.status.success),
                );
            }

            // Right-aligned actions: Format, Expand all, Collapse all
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);

                if ui
                    .add(
                        eparts::Button::icon(egui_phosphor::regular::ARROWS_IN_SIMPLE)
                            .with_size(eparts::Size::Sm)
                            .with_tooltip("Collapse all cells"),
                    )
                    .clicked()
                {
                    ctx.editor.collapse_all();
                }

                if ui
                    .add(
                        eparts::Button::icon(egui_phosphor::regular::ROWS)
                            .with_size(eparts::Size::Sm)
                            .with_tooltip("Expand all cells"),
                    )
                    .clicked()
                {
                    ctx.editor.expand_all();
                }

                if ui
                    .add(
                        eparts::Button::icon(egui_phosphor::regular::MAGIC_WAND)
                            .with_size(eparts::Size::Sm)
                            .with_tooltip("Format source code"),
                    )
                    .clicked()
                    && ctx.editor.format_source()
                {
                    *ctx.source_dirty = ctx.editor.text().to_string();
                    ctx.commands.push_back(PlaybackCommand::EditorChanged.into());
                }
            });
        });

        // Horizontal divider under toolbar
        ui.add_space(4.0);
        let avail_w = ui.available_width();
        let (sep_rect, _) = ui.allocate_exact_size(Vec2::new(avail_w, 1.0), egui::Sense::hover());
        ui.painter().line_segment(
            [sep_rect.left_top(), sep_rect.right_top()],
            egui::Stroke::new(1.0, theme.palette.border.subtle),
        );
        ui.add_space(4.0);

        ctx.editor.set_diagnostics(ctx.diagnostics);
        let response = ctx.editor.show(ui);
        if response.changed() || ctx.editor.text() != ctx.source_dirty.as_str() {
            *ctx.source_dirty = ctx.editor.text().to_string();
            ctx.commands.push_back(PlaybackCommand::EditorChanged.into());
        }
        if let Some(time_s) = ctx.editor.pending_scrub_to_time.take() {
            ctx.commands.push_back(PlaybackCommand::ScrubTo(time_s).into());
            if !ctx.is_playing {
                ctx.commands.push_back(PlaybackCommand::TogglePlayback.into());
            }
        }
    });
}
