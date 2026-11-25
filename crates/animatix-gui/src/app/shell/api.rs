use super::*;

impl GuiShell {
    /// Handle a single action, returning any collected side effects.
    pub(crate) fn handle_action(&mut self, action: ShellAction) -> Vec<Effect> {
        match action {
            ShellAction::Command(cmd) => self.handle_command(cmd),
            ShellAction::View(view) => self.handle_view_action(view),
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
            ViewAction::NewProject(template) => {
                if self.document_store.source.is_dirty() {
                    self.ui_store.unsaved_changes.open(
                        "Save changes before creating a project?",
                        ShellAction::View(ViewAction::NewProject(template)),
                    );
                    return vec![];
                }
                let Some(directory) = rfd::FileDialog::new()
                    .set_title("Choose a new or empty project folder")
                    .pick_folder()
                else {
                    return vec![];
                };
                match template.create(&directory) {
                    Ok(scene) => self.handle_command(Command::OpenFile(scene)),
                    Err(err) => vec![Effect::Toast(crate::app::components::toast::Toast::error(
                        format!("Could not create a project in {}: {err}", directory.display()),
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
                    Some(path) => {
                        self.narration_takes.checkpoint(&self.document_store.source, false);
                        let effects = file::handle_save_as(
                            &mut self.document_store,
                            &mut self.workspace_store,
                            &mut self.preview_store,
                            &mut self.plugin_manager,
                            path.clone(),
                        );
                        if self.document_store.source.file_path() == path
                            && !self.document_store.source.is_dirty()
                        {
                            self.narration_takes.checkpoint(&self.document_store.source, true);
                        }
                        effects
                    },
                    None => vec![],
                }
            },
            ViewAction::OpenSettings => {
                self.open_settings_page();
                vec![]
            },
            ViewAction::OpenShortcuts => {
                self.ui_store.view.shortcuts_open = true;
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
        }
    }
}
