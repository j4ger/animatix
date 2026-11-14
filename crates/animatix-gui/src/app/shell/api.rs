use super::*;

impl GuiShell {
    /// Handle a single action, returning any collected side effects.
    pub(crate) fn handle_action(&mut self, action: ShellAction) -> Vec<Effect> {
        if !action.allowed_in_workbench() {
            // Stale persisted UI and externally queued commands obey the same
            // rule as visible controls. Do not snapshot or mutate the document.
            self.ui_store.interaction.pending_drag_source_edits.clear();
            self.ui_store.interaction.reset_drag_state();
            return vec![Effect::Status(
                "Read-only review — edit source code to change the scene".into(),
            )];
        }
        match action {
            ShellAction::Command(cmd) => self.handle_command(cmd),
            ShellAction::View(view) => self.handle_view_action(view),
            ShellAction::Drag(drag) => self.handle_drag_event(drag),
        }
    }

    fn handle_view_action(&mut self, view: ViewAction) -> Vec<Effect> {
        match view {
            ViewAction::ShowInspector => ui::handle_show_inspector(&mut self.ui_store),
            ViewAction::ShowCode => ui::handle_show_code(&mut self.ui_store),
            ViewAction::ShowCurves => ui::handle_show_curves(&mut self.ui_store),
            ViewAction::ShowTimeline => ui::handle_show_timeline(&mut self.ui_store),
            ViewAction::ApplyLayout(preset) => ui::handle_apply_layout(&mut self.ui_store, preset),
            ViewAction::ResetLayout => ui::handle_reset_layout(&mut self.ui_store),
            ViewAction::OpenFileDialog => {
                match rfd::FileDialog::new().add_filter("Animatix", &["amx"]).pick_file() {
                    Some(path) => self.handle_command(Command::OpenFile(path)),
                    None => vec![],
                }
            },
            ViewAction::NewFile => {
                let path = crate::app::document::default_file_path();
                match std::fs::write(&path, "#0s\n") {
                    Ok(()) => self.handle_command(Command::OpenFile(path)),
                    Err(err) => vec![Effect::Toast(crate::app::components::toast::Toast::error(
                        format!("Failed to create scene: {err}"),
                    ))],
                }
            },
            ViewAction::SaveAsDialog => {
                // Saving under a new name would clear the current document's
                // sidecar, so require the recovery decision first.
                if self.recovery_prompt_pending() {
                    return vec![Effect::Toast(self.recovery_prompt_save_blocked())];
                }
                let default_name = self
                    .document_store
                    .source
                    .file_path()
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("untitled.amx")
                    .to_string();
                match rfd::FileDialog::new()
                    .add_filter("Animatix", &["amx"])
                    .set_file_name(&default_name)
                    .save_file()
                {
                    Some(path) => file::handle_save_as(
                        &mut self.document_store,
                        &mut self.workspace_store,
                        &mut self.preview_store,
                        &mut self.plugin_manager,
                        path,
                    ),
                    None => vec![],
                }
            },
            ViewAction::OpenSettings => {
                self.ui_store.view.settings_open = true;
                vec![]
            },
            ViewAction::OpenShortcuts => {
                self.ui_store.view.shortcuts_open = true;
                vec![]
            },
            ViewAction::OpenInsertionPalette => {
                use crate::app::shell::insertion_palette::PaletteMode;
                self.insertion_palette.open(PaletteMode::Universal);
                vec![]
            },
            ViewAction::OpenExportDialog => {
                if self.document_store.source.document.last_rebuild_error.is_some()
                    || self.document_store.showing_last_good()
                    || self.document_store.snapshot_is_stale()
                    || self.preview_store.has_pending_rebuild()
                {
                    vec![Effect::Status(
                        "Export unavailable: wait for a successful build of the current source"
                            .into(),
                    )]
                } else {
                    ui::handle_open_export_dialog(&mut self.export_store, &self.document_store)
                }
            },
            ViewAction::OpenCommandPalette => {
                self.ui_store.view.command_palette_open = true;
                self.ui_store.command_palette_selected = 0;
                vec![]
            },
            ViewAction::OpenPluginStatus => {
                self.ui_store.view.plugin_status_open = true;
                vec![]
            },
            ViewAction::OpenFindReplace => {
                self.ui_store.view.find_replace_open = true;
                self.ui_store.find_last_match = None;
                vec![]
            },
            ViewAction::DeselectActors => {
                self.ui_store.selection.selected_actors.clear();
                self.ui_store.selection.selection.clear_tapped_place();
                vec![]
            },
        }
    }

    fn handle_drag_event(&mut self, drag: DragEvent) -> Vec<Effect> {
        let effects = match drag {
            DragEvent::DragEnded => ui::handle_drag_ended(&mut self.ui_store),
            DragEvent::InspectorInputDragStarted => {
                ui::handle_inspector_input_drag_started(&mut self.ui_store)
            },
            DragEvent::InspectorInputDragEnded => {
                ui::handle_inspector_input_drag_ended(&mut self.ui_store)
            },
        };
        // Flush any deferred source edits once the drag interaction ends.
        if matches!(drag, DragEvent::DragEnded | DragEvent::InspectorInputDragEnded) {
            self.flush_pending_drag_edits();
        }
        effects
    }
}
