use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::*;
use crate::app::components::toast::Toast;

impl GuiShell {
    pub(crate) fn handle_command(&mut self, command: Command) -> Vec<Effect> {
        match command {
            Command::OpenFile(path) => self.command_open_file(path),
            Command::Save => self.command_save(),
            Command::Reload => self.command_reload(),
            Command::ReloadPlugins => self.command_reload_plugins(),
            Command::Rebuild => self.command_rebuild(),
            Command::ToggleExpandDir(path) => self.command_toggle_expand_dir(path),
            Command::SwitchWorkspace(path) => self.command_switch_workspace(path),
            Command::TogglePlayback => self.command_toggle_playback(),
            Command::ScrubTo(time) => self.command_scrub_to(time),
            Command::PrevKeyframe => self.command_prev_keyframe(),
            Command::NextKeyframe => self.command_next_keyframe(),
            Command::FrameStepForward => self.command_frame_step_forward(),
            Command::FrameStepBackward => self.command_frame_step_backward(),
            Command::StepNext => self.command_step_next(),
            Command::StepPrevious => self.command_step_previous(),
            Command::TogglePresentationMode => self.command_toggle_presentation_mode(),
            Command::ToggleEditorSync => self.command_toggle_editor_sync(),
            Command::EditorChanged => self.command_editor_changed(),
            Command::ApplySourcePatch(ref patch) => {
                self.command_apply_source_patch(&command, patch)
            },
            Command::SelectScene(scene) => self.command_select_scene(scene),
            Command::Undo => self.command_undo(),
            Command::Redo => self.command_redo(),
            Command::ScrollToLine(line, column) => self.command_scroll_to_line(line, column),
            Command::OpenParagraphTakes {
                epoch,
                ref scene,
                ref paragraph,
            } => {
                if epoch == self.document_store.source.epoch() {
                    self.narration_takes.open_paragraph(
                        &self.document_store.source,
                        scene.clone(),
                        paragraph,
                    );
                }
                vec![]
            },
            Command::RevealNarration {
                epoch,
                scene,
                marker,
            } => self.command_reveal_narration(epoch, scene.as_deref(), &marker),
            Command::ReviewNarrationCue {
                epoch,
                scene,
                line,
                marker,
            } => self.command_review_narration_cue(epoch, scene.as_deref(), line, &marker),
            Command::ZoomToSelection => self.command_zoom_to_selection(),
            Command::ZoomToAll => self.command_zoom_to_all(),
            // Find/replace applies its edit directly in the modal handler.
            Command::FindReplaceAll => vec![],
        }
    }

    /// Open a document after resolving any unsaved source changes.
    fn command_open_file(&mut self, path: PathBuf) -> Vec<Effect> {
        if self.document_store.source.is_dirty() {
            self.ui_store.unsaved_changes.open(
                format!("Save changes before opening \"{}\"?", path.display()),
                Command::OpenFile(path).into(),
            );
            return vec![];
        }
        match file::submit_open_file(&mut self.load_worker, &mut self.plugin_manager, path.clone())
        {
            Ok(token) => {
                self.pending_open = Some(token);
                // The new document will have its own snapshot state; a
                // rebuild response for the old document must not land.
                self.preview_store.pending_rebuild_at = None;
                self.preview_store.quiet_rebuild = None;
                self.preview_store.in_flight_rebuild = None;
                self.preview_store.rebuild_in_progress = false;
                self.preview_store.preview.error = None;
                self.preview_store.preview.status = format!("Opening {}…", path.display());
                self.preview_store.preview_dirty = true;
            },
            Err(err) => {
                self.pending_open = None;
                self.preview_store.preview.set_status_error(format!(
                    "Open failed • {} • {}",
                    path.display(),
                    err
                ));
            },
        }
        vec![]
    }

    fn command_toggle_expand_dir(&mut self, path: PathBuf) -> Vec<Effect> {
        file::handle_toggle_expand_dir(&mut self.workspace_store, &self.document_store, path)
    }

    /// Workspace switch flow, including the dirty-document confirmation.
    fn command_switch_workspace(&mut self, path: PathBuf) -> Vec<Effect> {
        if self.document_store.source.is_dirty() {
            self.ui_store.unsaved_changes.open(
                format!("Save changes before switching workspace to \"{}\"?", path.display()),
                Command::SwitchWorkspace(path).into(),
            );
            return vec![];
        }
        file::handle_switch_workspace(&mut self.workspace_store, &self.document_store, path)
    }

    fn command_save(&mut self) -> Vec<Effect> {
        if self.recovery_prompt_pending() {
            return vec![Effect::Toast(self.recovery_prompt_save_blocked())];
        }
        self.narration_takes.checkpoint(&self.document_store.source, false);
        let effects = file::handle_save(&mut self.document_store, &mut self.preview_store);
        if !self.document_store.source.is_dirty() {
            self.narration_takes.checkpoint(&self.document_store.source, true);
        }
        effects
    }

    fn command_reload(&mut self) -> Vec<Effect> {
        if self.recovery_prompt_pending() {
            return vec![Effect::Toast(
                crate::app::components::toast::Toast::warning(
                    "Choose Recover or Discard before reloading",
                ),
            )];
        }
        if self.document_store.source.is_dirty() {
            self.ui_store
                .unsaved_changes
                .open("Save changes before reloading?".to_string(), Command::Reload.into());
            return vec![];
        }
        file::handle_reload(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            &mut self.workspace_store,
        )
    }

    fn command_rebuild(&mut self) -> Vec<Effect> {
        // A manual rebuild supersedes any debounced rebuild and runs the
        // same off-thread worker as ordinary source edits, so the UI
        // thread never blocks on parse/build.
        self.preview_store.pending_rebuild_at = None;
        self.preview_store.preview.error = None;
        if let Some(token) = file::handle_rebuild_submit(
            &mut self.rebuild_worker,
            &mut self.document_store,
            &mut self.preview_store,
        ) {
            self.preview_store.in_flight_rebuild = Some(token);
        } else {
            self.preview_store.in_flight_rebuild = None;
        }
        vec![]
    }

    fn command_reload_plugins(&mut self) -> Vec<Effect> {
        if self.plugin_manager.reload() {
            self.apply_plugin_reload();
        } else {
            self.preview_store.preview.set_status_info("Plugins already up to date");
        }
        vec![]
    }

    fn command_toggle_playback(&mut self) -> Vec<Effect> {
        playback::handle_toggle_playback(&mut self.preview_store)
    }

    fn command_scrub_to(&mut self, next_time: f64) -> Vec<Effect> {
        playback::handle_scrub_to(
            &mut self.document_store,
            &mut self.preview_store,
            &self.ui_store,
            next_time,
        )
    }

    fn command_prev_keyframe(&mut self) -> Vec<Effect> {
        playback::handle_prev_keyframe(
            &self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_next_keyframe(&mut self) -> Vec<Effect> {
        playback::handle_next_keyframe(
            &self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_frame_step_forward(&mut self) -> Vec<Effect> {
        playback::handle_frame_step_forward(
            &mut self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_frame_step_backward(&mut self) -> Vec<Effect> {
        playback::handle_frame_step_backward(
            &mut self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_step_next(&mut self) -> Vec<Effect> {
        playback::handle_step_next(
            &mut self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_step_previous(&mut self) -> Vec<Effect> {
        playback::handle_step_previous(
            &mut self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_toggle_presentation_mode(&mut self) -> Vec<Effect> {
        playback::handle_toggle_presentation_mode(&mut self.preview_store)
    }

    fn command_toggle_editor_sync(&mut self) -> Vec<Effect> {
        playback::handle_toggle_editor_sync(&mut self.ui_store)
    }

    fn command_editor_changed(&mut self) -> Vec<Effect> {
        playback::handle_editor_changed(
            &mut self.document_store,
            &mut self.preview_store,
            &self.ui_store,
        )
    }

    fn command_apply_source_patch(
        &mut self,
        command: &Command,
        patch: &crate::editor::source_patch::SourcePatch,
    ) -> Vec<Effect> {
        let Some(text) = patch.apply(&self.document_store.source) else {
            return vec![Effect::Toast(Toast::warning(
                "Source changed. Open the edit again to use the current revision.",
            ))];
        };
        let snapshot = self.ui_store.snapshot_with_preview(&self.preview_store);
        self.document_store.snapshot(command.clone(), snapshot.clone());
        self.document_store.replace_text_with_ui(text, snapshot);
        self.preview_store.pending_rebuild_at =
            Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
        self.preview_store.preview.error = None;
        vec![Effect::Status(format!(
            "{} • rebuild scheduled",
            patch.label()
        ))]
    }

    fn command_select_scene(&mut self, scene: String) -> Vec<Effect> {
        self.ui_store.selection.selection.clear_tapped_place();
        self.ui_store.selection.selected_keyframes.clear();
        scene::handle_select_scene(
            &mut self.document_store,
            &mut self.preview_store,
            &mut self.ui_store,
            scene,
        )
    }

    fn command_undo(&mut self) -> Vec<Effect> {
        ui::handle_undo(&mut self.document_store, &mut self.preview_store, &mut self.ui_store)
    }

    fn command_redo(&mut self) -> Vec<Effect> {
        ui::handle_redo(&mut self.document_store, &mut self.preview_store, &mut self.ui_store)
    }

    fn command_scroll_to_line(&mut self, line: usize, column: usize) -> Vec<Effect> {
        self.reveal_source_editor();
        self.document_store
            .source
            .editor
            .set_source_view(crate::editor::narration::SourceView::Full);
        ui::handle_scroll_to_line(&mut self.document_store, line, column)
    }

    fn reveal_source_editor(&mut self) {
        self.ui_store.view.preview_fullscreen = false;
        crate::app::persistence::show_editor(&mut self.ui_store.view.tree);
        if self.ui_store.view.compact_drawer == Some(crate::app::panels::CompactDrawer::Detail) {
            self.ui_store.view.compact_drawer = None;
        }
    }

    fn command_reveal_narration(
        &mut self,
        epoch: crate::app::document::version::SourceEpoch,
        scene: Option<&str>,
        marker: &str,
    ) -> Vec<Effect> {
        let source = &mut self.document_store.source;
        if source.epoch() != epoch || source.editor.text() != source.text() {
            return vec![Effect::Toast(Toast::warning(
                "Source changed. Refresh the narration cue.",
            ))];
        }
        if source.editor.reveal_narration(scene, marker) {
            let effects = crate::app::handlers::ui::handle_apply_layout(
                &mut self.ui_store,
                crate::app::LayoutPreset::Code,
            );
            self.reveal_source_editor();
            effects
        } else {
            vec![Effect::Toast(Toast::warning(
                "This narration phrase has no current source match.",
            ))]
        }
    }

    fn command_review_narration_cue(
        &mut self,
        epoch: crate::app::document::version::SourceEpoch,
        scene: Option<&str>,
        line: usize,
        marker: &str,
    ) -> Vec<Effect> {
        let source = &self.document_store.source;
        let document = &source.document;
        if source.epoch() != epoch
            || source.editor.text() != source.text()
            || self.preview_store.has_pending_rebuild()
            || document.last_rebuild_error.is_some()
        {
            return vec![Effect::Toast(Toast::warning(
                "Rebuild the current source to review this cue.",
            ))];
        }
        let resolve = |statements: &[animatix_syntax::ast::Stmt]| {
            crate::app::panels::timeline_panel::review_markers(
                statements,
                document.timeline.as_ref(),
                document.composition.as_ref(),
                &document.diagnostics,
            )
            .iter()
            .find(|target| {
                target.scene.as_deref() == scene
                    && target
                        .expression
                        .split_once('@')
                        .is_some_and(|(_, tail)| tail.split_whitespace().next() == Some(marker))
                    && (target.line == Some(line)
                        || animatix_syntax::narration::is_implicit_anchor(marker))
            })
            .and_then(|target| target.time_s)
        };
        let time = document.raw_statements.as_deref().and_then(resolve);
        let mut effects = ui::handle_show_timeline(&mut self.ui_store);
        if let Some(time) = time {
            effects.extend(self.command_scrub_to(time).into_iter().filter(|effect| {
                !matches!(effect, Effect::EditorScroll(_) | Effect::EditorHighlight(_))
            }));
        } else {
            effects.push(Effect::Toast(Toast::warning("This narration cue is unresolved.")));
        }
        effects
    }

    fn command_zoom_to_selection(&mut self) -> Vec<Effect> {
        ui::handle_zoom_to_selection(&mut self.preview_store, &self.ui_store, &self.document_store)
    }

    fn command_zoom_to_all(&mut self) -> Vec<Effect> {
        ui::handle_zoom_to_all(&mut self.preview_store, &self.document_store)
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use crate::app::commands::{Command, Effect, ShellAction};
    use crate::app::document::{recovery_path, write_recovery};
    use crate::app::test_support::TestTempDir;

    use super::*;

    #[test]
    fn reload_keeps_pending_recovery_until_user_decides() {
        let dir = TestTempDir::new("reload-pending-recovery");
        let source_path = dir.path().join("scene.amx");
        let disk_text = "#0s\nbox: Rect, size: (100, 100)\n";
        let recovery_text = "#0s\nbox: Rect, size: (200, 200)\n";
        std::fs::write(&source_path, disk_text).expect("write source");

        // Make recovery strictly newer even on filesystems with coarse mtime precision.
        std::fs::File::options()
            .write(true)
            .open(&source_path)
            .expect("open source")
            .set_modified(SystemTime::now() - Duration::from_secs(600))
            .expect("backdate source");
        write_recovery(&source_path, recovery_text).expect("write recovery sidecar");

        let mut shell = GuiShell::load(source_path.clone(), false);
        let sidecar = recovery_path(&source_path);
        assert!(shell.recovery_prompt_pending());
        assert!(!shell.document_store.source.is_dirty());
        assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), recovery_text);

        let effects = shell.handle_action(ShellAction::Command(Command::Reload));

        assert!(matches!(
            effects.first(),
            Some(Effect::Toast(toast)) if toast.message.contains("before reloading")
        ));
        assert!(shell.recovery_prompt_pending());
        assert_eq!(shell.document_store.source.text(), disk_text);
        assert_eq!(
            std::fs::read_to_string(&sidecar).unwrap(),
            recovery_text,
            "reload must preserve the only recovery copy until Recover or Discard"
        );
    }
}
