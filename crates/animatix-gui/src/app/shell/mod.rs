pub mod autosave;
pub mod command_palette;
pub mod export_dialog;
pub mod find_replace;
pub mod insertion_palette;
pub mod plugin_status;
pub mod settings;
pub mod shortcut_cheat_sheet;
pub mod toolbar;
pub mod transport;

use crate::app::GuiShell;
use crate::app::commands::{Command, DocumentCommand, DragEvent, Effect, ShellAction, ViewAction};
use crate::app::handlers::*;

impl GuiShell {
    /// Handle a single action, returning any collected side effects.
    pub(crate) fn handle_action(&mut self, action: ShellAction) -> Vec<Effect> {
        match action {
            ShellAction::Command(cmd) => self.handle_command(cmd),
            ShellAction::View(view) => self.handle_view_action(view),
            ShellAction::Drag(drag) => self.handle_drag_event(drag),
        }
    }

    fn handle_command(&mut self, command: Command) -> Vec<Effect> {
        match command {
            Command::OpenFile(path) => {
                if self.document_store.source.is_dirty() {
                    self.ui_store.unsaved_changes.open(
                        format!("Save changes before opening \"{}\"?", path.display()),
                        DocumentCommand::OpenFile(path).into(),
                    );
                    return vec![];
                }
                let effects = file::handle_open_file(
                    &mut self.document_store,
                    &mut self.workspace_store,
                    &mut self.preview_store,
                    &mut self.ui_store,
                    &mut self.plugin_manager,
                    path,
                );
                // The newly opened file may have a newer crash-recovery sidecar;
                // surface it before autosave can overwrite it.
                self.detect_recovery_prompt();
                effects
            },
            Command::ToggleExpandDir(path) => file::handle_toggle_expand_dir(
                &mut self.workspace_store,
                &self.document_store,
                path,
            ),
            Command::SwitchWorkspace(path) => {
                if self.document_store.source.is_dirty() {
                    self.ui_store.unsaved_changes.open(
                        format!(
                            "Save changes before switching workspace to \"{}\"?",
                            path.display()
                        ),
                        DocumentCommand::SwitchWorkspace(path).into(),
                    );
                    return vec![];
                }
                file::handle_switch_workspace(&mut self.workspace_store, &self.document_store, path)
            },
            Command::Save => {
                if self.recovery_prompt_pending() {
                    return vec![Effect::Toast(self.recovery_prompt_save_blocked())];
                }
                file::handle_save(&mut self.document_store, &mut self.preview_store)
            },
            Command::Reload => {
                if self.document_store.source.is_dirty() {
                    self.ui_store.unsaved_changes.open(
                        "Save changes before reloading?".to_string(),
                        DocumentCommand::Reload.into(),
                    );
                    return vec![];
                }
                file::handle_reload(
                    &mut self.document_store,
                    &mut self.preview_store,
                    &mut self.ui_store,
                    &mut self.workspace_store,
                )
            },
            Command::Rebuild => file::handle_rebuild(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
            ),
            Command::ReloadPlugins => {
                if self.plugin_manager.reload() {
                    self.apply_plugin_reload();
                } else {
                    self.preview_store.preview.set_status_info("Plugins already up to date");
                }
                vec![]
            },
            Command::ScrubTo(next_time) => playback::handle_scrub_to(
                &mut self.document_store,
                &mut self.preview_store,
                &self.ui_store,
                next_time,
            ),
            Command::TogglePlayback => playback::handle_toggle_playback(&mut self.preview_store),
            Command::ToggleEditorSync => playback::handle_toggle_editor_sync(&mut self.ui_store),
            Command::EditorChanged => playback::handle_editor_changed(
                &mut self.document_store,
                &mut self.preview_store,
                &self.ui_store,
            ),
            Command::PrevKeyframe => playback::handle_prev_keyframe(
                &self.document_store,
                &mut self.preview_store,
                &self.ui_store,
            ),
            Command::NextKeyframe => playback::handle_next_keyframe(
                &self.document_store,
                &mut self.preview_store,
                &self.ui_store,
            ),
            Command::FrameStepForward => playback::handle_frame_step_forward(
                &mut self.document_store,
                &mut self.preview_store,
                &self.ui_store,
            ),
            Command::FrameStepBackward => playback::handle_frame_step_backward(
                &mut self.document_store,
                &mut self.preview_store,
                &self.ui_store,
            ),
            Command::SelectScene(scene) => {
                self.ui_store.selection.selection.clear_tapped_place();
                self.ui_store.selection.selected_keyframes.clear();
                scene::handle_select_scene(
                    &mut self.document_store,
                    &mut self.preview_store,
                    &mut self.ui_store,
                    scene,
                )
            },
            Command::ReorderScenes(new_order) => scene::handle_reorder_scenes(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                new_order,
            ),
            Command::AddScene(scene) => scene::handle_add_scene(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                scene,
            ),
            Command::DuplicateScene(scene) => scene::handle_duplicate_scene(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                scene,
            ),
            Command::DeleteScene(scene) => scene::handle_delete_scene(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                scene,
            ),
            Command::CreateActor {
                ty,
                label,
                position,
                props,
            } => actor::handle_create_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                ty,
                label,
                position,
                props,
            ),
            Command::DuplicateActor(original_label) => actor::handle_duplicate_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                original_label,
            ),
            Command::DuplicateSelectedActors => {
                let labels: Vec<String> =
                    self.ui_store.selection.selected_actors.iter().cloned().collect();
                let mut effects = Vec::new();
                for label in labels {
                    effects.extend(actor::handle_duplicate_actor(
                        &mut self.document_store,
                        &mut self.preview_store,
                        &mut self.ui_store,
                        label,
                    ));
                }
                effects
            },
            Command::DeleteSelectedActors => actor::handle_delete_selected_actors(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
            ),
            Command::PasteActors => actor::handle_paste_actors(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
            ),
            Command::SetTransition {
                from_scene,
                transition,
            } => property::handle_set_transition(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                from_scene,
                transition,
            ),
            Command::SetPlayTarget { from_scene, target } => property::handle_set_play_target(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                from_scene,
                target,
            ),
            Command::SetSceneDuration { scene, duration_s } => property::handle_set_scene_duration(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                scene,
                duration_s,
            ),
            Command::RenameActor {
                old_label,
                new_label,
            } => actor::handle_rename_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                old_label,
                new_label,
            ),
            Command::SetKeyframeEasing {
                scene,
                actor,
                property,
                time_s,
                easing,
            } => keyframe::handle_set_keyframe_easing(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                scene,
                actor,
                property,
                time_s,
                easing,
            ),
            Command::DeleteKeyframe {
                scene,
                actor,
                property,
                time_s,
            } => keyframe::handle_delete_keyframe(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                scene,
                actor,
                property,
                time_s,
            ),
            Command::MoveKeyframe {
                scene,
                actor,
                property,
                old_time_s,
                new_time_s,
            } => keyframe::handle_move_keyframe(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                scene,
                actor,
                property,
                old_time_s,
                new_time_s,
            ),
            Command::SetKeyframeValue {
                scene,
                actor,
                property,
                time_s,
                value,
            } => keyframe::handle_set_keyframe_value(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                scene,
                actor,
                property,
                time_s,
                value,
            ),
            Command::MoveKeyframes(specs) => keyframe::handle_move_keyframes(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                specs,
            ),
            Command::DeleteKeyframes(ids) => keyframe::handle_delete_keyframes(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                ids,
            ),
            Command::SetSelectedKeyframes(keyframes) => {
                ui::handle_set_selected_keyframes(&mut self.ui_store, keyframes)
            },
            Command::ResizeAction {
                verb,
                targets,
                old_start_s,
                new_start_s,
                new_duration_s,
            } => keyframe::handle_resize_action(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                verb,
                targets,
                old_start_s,
                new_start_s,
                new_duration_s,
            ),
            Command::ReparentActor { actor, new_parent } => actor::handle_reparent_actor(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
                new_parent,
            ),
            Command::ExtractScene {
                actor_labels,
                new_scene_name,
            } => actor::handle_extract_scene(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor_labels,
                new_scene_name,
            ),
            Command::MoveToScene {
                actor_labels,
                target_scene,
            } => actor::handle_move_to_scene(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor_labels,
                target_scene,
            ),
            Command::ToggleActorVisibility(actor) => actor::handle_toggle_actor_visibility(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
            ),
            Command::ToggleActorLock(actor) => actor::handle_toggle_actor_lock(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                actor,
            ),
            Command::PropertyEdit(edit) => {
                self.handle_property_edit(edit);
                vec![]
            },
            Command::DetachCallout {
                actor,
                from,
                to,
                label_at,
            } => {
                self.handle_detach_callout(actor, from, to, label_at);
                vec![]
            },
            Command::Undo => ui::handle_undo(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
            ),
            Command::Redo => ui::handle_redo(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
            ),
            Command::ScrollToLine(line, column) => {
                ui::handle_scroll_to_line(&mut self.document_store, line, column)
            },
            Command::ZoomToSelection => ui::handle_zoom_to_selection(
                &mut self.preview_store,
                &self.ui_store,
                &self.document_store,
            ),
            Command::ZoomToAll => {
                ui::handle_zoom_to_all(&mut self.preview_store, &self.document_store)
            },
            Command::AlignActors(alignment) => actor::handle_align_actors(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                alignment,
            ),
            Command::DistributeActors(axis) => actor::handle_distribute_actors(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
                axis,
            ),
            Command::GroupSelectedActors => actor::handle_group_selected_actors(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
            ),
            Command::UngroupSelectedActors => actor::handle_ungroup_selected_actors(
                &mut self.document_store,
                &mut self.preview_store,
                &mut self.ui_store,
            ),
            // InsertionFromPalette is a snapshot marker, not dispatched through the handler.
            Command::InsertionFromPalette => vec![],

            // FindReplaceAll is handled in-place by perform_find_replace_all, not dispatched.
            Command::FindReplaceAll => vec![],
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
                let path = crate::document::default_file_path();
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
                        &mut self.preview_store,
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
                ui::handle_open_export_dialog(&mut self.export_store, &self.document_store)
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
