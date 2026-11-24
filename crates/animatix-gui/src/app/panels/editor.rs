//! Editor panel: source code editor with diagnostics and scrub-to commands.

use animatix_syntax::diagnostics::Diagnostic;
use egui::Vec2;

use crate::app::commands::{ActionQueue, Command};
use crate::app::design_tokens::spatial::{STROKE_WIDTH, spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::document::version::SourceEpoch;
use crate::app::panels::panel_frame;
use crate::editor::EditorBuffer;
use crate::editor::narration::SourceView;

pub(crate) struct EditorContext<'a> {
    pub voiceover_history: &'a crate::app::document::narration::VoiceoverHistory,
    pub editor: &'a mut EditorBuffer,
    pub diagnostics: &'a [Diagnostic],
    pub document: &'a crate::app::document::DocumentSession,
    pub source_epoch: SourceEpoch,
    pub commands: &'a mut ActionQueue,
}

fn source_projection_menu(editor: &mut EditorBuffer, ui: &mut egui::Ui) {
    let current = editor.source_view();
    let name = SourceView::ALL
        .iter()
        .find(|(view, _)| *view == current)
        .map(|(_, label)| *label)
        .unwrap_or("Full");
    let menu = ui.add(
        eparts::Button::ghost(format!("View: {name}"))
            .with_icon(egui_phosphor::regular::CARET_DOWN)
            .with_size(eparts::Size::Sm)
            .with_tooltip("Choose source view"),
    );
    egui::Popup::menu(&menu).show(|ui| {
        for (view, label) in SourceView::ALL {
            if ui.selectable_label(current == view, label).clicked() {
                editor.set_source_view(view);
                ui.close();
            }
        }
    });
}

pub(crate) fn editor_ui(ctx: &mut EditorContext<'_>, ui: &mut egui::Ui) {
    let theme = eparts::theme(ui);
    let sp = spatial(ui);

    panel_frame().show(ui, |ui| {
        // ── Toolbar Header ──
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_4, 0.0);

            // Title
            ui.label(
                egui::RichText::new(egui_phosphor::regular::CODE)
                    .size(TextRole::Body.size())
                    .color(theme.palette.accent.primary),
            );
            ui.label(
                egui::RichText::new(if ctx.editor.source_view() == SourceView::Script {
                    "Narration"
                } else {
                    "Editor"
                })
                .size(TextRole::Body.size())
                .strong()
                .color(theme.palette.text.primary),
            );

            source_projection_menu(ctx.editor, ui);

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
                if ui.add(btn).on_hover_text("Click to jump to first error").clicked()
                    && let Some(err) = ctx.diagnostics.iter().find(|d| {
                        d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Error
                    })
                {
                    let line = err.location.line.unwrap_or(1).saturating_sub(1);
                    let col = err.location.column.unwrap_or(1).saturating_sub(1);
                    ctx.editor.focus_diagnostic(line, col);
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
                if ui.add(btn).on_hover_text("Click to jump to first warning").clicked()
                    && let Some(warn) = ctx.diagnostics.iter().find(|d| {
                        d.severity == animatix_syntax::diagnostics::DiagnosticSeverity::Warning
                    })
                {
                    let line = warn.location.line.unwrap_or(1).saturating_sub(1);
                    let col = warn.location.column.unwrap_or(1).saturating_sub(1);
                    ctx.editor.focus_diagnostic(line, col);
                }
            }

            // Right-aligned action: Format source.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, 0.0);

                if ui
                    .add_enabled(
                        ctx.editor.source_view() == SourceView::Full,
                        eparts::Button::icon(egui_phosphor::regular::MAGIC_WAND)
                            .with_size(eparts::Size::Sm)
                            .with_tooltip("Format source code"),
                    )
                    .clicked()
                {
                    ctx.editor.format_source();
                }
            });
        });

        // Horizontal divider under toolbar
        ui.add_space(sp.base.space_2);
        let avail_w = ui.available_width();
        let (sep_rect, _) =
            ui.allocate_exact_size(Vec2::new(avail_w, STROKE_WIDTH), egui::Sense::hover());
        ui.painter().line_segment(
            [sep_rect.left_top(), sep_rect.right_top()],
            egui::Stroke::new(STROKE_WIDTH, theme.palette.border.subtle),
        );
        ui.add_space(sp.base.space_2);

        if ctx.editor.source_view() == SourceView::Full
            && let Some((scene, marker)) = ctx.editor.cursor_narration_marker()
            && ui
                .add(eparts::Button::ghost("Show phrase").with_size(eparts::Size::Sm))
                .clicked()
        {
            ctx.commands.push_back(
                Command::RevealNarration {
                    epoch: ctx.source_epoch,
                    scene,
                    marker,
                }
                .into(),
            );
        }

        ctx.editor.set_diagnostics(ctx.diagnostics);
        let changed = match ctx.editor.source_view() {
            SourceView::Full => ctx.editor.show(ui).changed(),
            SourceView::Script => {
                ctx.editor.show_script(
                    ui,
                    ctx.source_epoch,
                    ctx.document,
                    ctx.voiceover_history,
                    ctx.commands,
                );
                false
            },
            SourceView::Code => {
                ctx.editor.show_code_projection(ui);
                false
            },
        };
        if changed || ctx.editor.text() != ctx.document.source_text {
            ctx.commands.push_back(Command::EditorChanged.into());
        }
    });
}
