//! Editor panel: source code editor with diagnostics and scrub-to commands.

use crate::app::commands::{ActionQueue, Command, ShellAction};
use crate::app::panels::panel_frame;
use crate::editor::EditorBuffer;
use animatix_syntax::diagnostics::Diagnostic;

pub(crate) struct EditorContext<'a> {
    pub editor: &'a mut EditorBuffer,
    pub diagnostics: &'a [Diagnostic],
    pub source_dirty: &'a mut String,
    pub commands: &'a mut ActionQueue,
    pub is_playing: bool,
}

pub(crate) fn editor_ui(ctx: &mut EditorContext<'_>, ui: &mut egui::Ui) {
    panel_frame().show(ui, |ui| {
        ctx.editor.set_diagnostics(ctx.diagnostics);
        let response = ctx.editor.show(ui);
        if response.changed() || ctx.editor.text() != ctx.source_dirty.as_str() {
            *ctx.source_dirty = ctx.editor.text().to_string();
            ctx.commands.push_back(ShellAction::Command(Command::EditorChanged));
        }
        if let Some(time_s) = ctx.editor.pending_scrub_to_time.take() {
            ctx.commands.push_back(ShellAction::Command(Command::ScrubTo(time_s)));
            if !ctx.is_playing {
                ctx.commands.push_back(ShellAction::Command(Command::TogglePlayback));
            }
        }
    });
}