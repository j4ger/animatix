use super::*;

impl FocusContext {
    pub fn can_handle(&self, scope: ShortcutScope) -> bool {
        match scope {
            ShortcutScope::Global => true,
            ShortcutScope::TextSafe => !self.inline_edit_active && !self.unsaved_dialog_open,
            ShortcutScope::Canvas => {
                !self.text_edit_focused
                    && !self.inline_edit_active
                    && !self.command_palette_open
                    && !self.find_replace_open
                    && !self.unsaved_dialog_open
            },
        }
    }
}

impl KeyboardAction {
    /// Legacy bindings remain parseable, but cannot activate spatial edits.
    pub(crate) fn allowed_in_workbench(&self) -> bool {
        !matches!(
            self,
            Self::DuplicateSelection
                | Self::DeleteSelection
                | Self::GroupSelection
                | Self::UngroupSelection
                | Self::SetMoveTool
                | Self::SetScaleTool
                | Self::SetRotateTool
                | Self::SetVertexTool
                | Self::SetPivotTool
                | Self::OpenInsertionPalette
                | Self::CopySelection
                | Self::PasteClipboard
        )
    }
}
