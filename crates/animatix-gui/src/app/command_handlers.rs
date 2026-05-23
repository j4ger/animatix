use super::*;

impl GuiShell {
    pub(crate) fn handle_command(&mut self, command: Command) {
        use crate::app::commands::Command;

        match command {
            Command::OpenFile(path) => self.open_document(path),
            Command::ToggleExpandDir(path) => {
                if self.workspace_store.expanded_dirs.contains(&path) {
                    self.workspace_store.expanded_dirs.remove(&path);
                } else {
                    self.workspace_store.expanded_dirs.insert(path.clone());
                }
                self.workspace_store.file_tree = build_file_tree(&self.workspace_store.workspace_root, &self.document_store.document.file_path, &self.workspace_store.expanded_dirs);
            }
            Command::ShowInspector => self.open_workspace_tab(WorkspaceTab::Inspector),
            Command::OpenExportDialog => {
                self.export_store.export_dialog_open = true;
                if self.export_store.export_state.output_path.is_empty() {
                    self.update_default_export_filename();
                }
            }
            Command::ToggleDiagnosticsPanel => {
                self.ui_store.diagnostics_panel_visible = !self.ui_store.diagnostics_panel_visible;
            }
            Command::Save => { let _ = self.save(); }
            Command::Reload => { let _ = self.reload(); }
            Command::Rebuild => { let _ = self.rebuild(); }
            Command::ScrubTo(next_time) => {
                self.preview_store.preview.current_time_s = next_time;
                self.preview_store.preview.clamp_time();
                self.preview_store.preview.is_playing = false;
                self.preview_store.preview_dirty = true;
                self.sync_active_scene_from_time();
                if self.ui_store.editor_sync_enabled {
                    if let Some(line) = self.document_store.document.find_keyframe_line_at(next_time) {
                        self.document_store.editor.scroll_to_line(line);
                        self.document_store.editor.set_highlighted_line(Some(line));
                    }
                }
            }
            Command::TogglePlayback => {
                self.preview_store.preview.toggle_playback();
                self.preview_store.preview_dirty = true;
            }
            Command::ToggleEditorSync => {
                self.ui_store.editor_sync_enabled = !self.ui_store.editor_sync_enabled;
                self.preview_store.preview.status = if self.ui_store.editor_sync_enabled {
                    "Editor sync ON".to_string()
                } else {
                    "Editor sync OFF".to_string()
                };
            }
            Command::ToggleKeyframeMode => {
                self.ui_store.keyframe_mode = !self.ui_store.keyframe_mode;
                self.preview_store.preview.status = if self.ui_store.keyframe_mode {
                    "Keyframe mode ON — edits create timestamps".to_string()
                } else {
                    "Keyframe mode OFF — edits overwrite defaults".to_string()
                };
            }
            Command::EditorChanged => {
                self.document_store
                    .document
                    .set_source_text(self.document_store.editor.text().to_string());
                self.preview_store.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
                self.preview_store.preview.status = "Editing source • rebuild scheduled".to_string();
                self.preview_store.preview.error = None;
                self.document_store.document.diagnostics.clear();
            }
            Command::RequestRepaint => self.preview_store.preview_dirty = true,
            Command::PrevKeyframe => {
                let keyframes = timeline_keyframe_times_s(
                    if self.document_store.document.composition.is_some() { None } else { self.document_store.document.active_timeline() },
                    self.document_store.document.composition.as_ref(),
                    self.document_store.document.active_scene.as_deref(),
                );
                self.preview_store.preview.go_to_previous_keyframe(&keyframes);
                self.preview_store.preview.status = format!(
                    "Previous keyframe • t = {:.2}s / {:.2}s",
                    self.preview_store.preview.current_time_s, self.preview_store.preview.duration_s
                );
                self.preview_store.preview_dirty = true;
                if self.ui_store.editor_sync_enabled {
                    if let Some(line) = self.document_store.document.find_keyframe_line_at(self.preview_store.preview.current_time_s) {
                        self.document_store.editor.scroll_to_line(line);
                        self.document_store.editor.set_highlighted_line(Some(line));
                    }
                }
            }
            Command::NextKeyframe => {
                let keyframes = timeline_keyframe_times_s(
                    if self.document_store.document.composition.is_some() { None } else { self.document_store.document.active_timeline() },
                    self.document_store.document.composition.as_ref(),
                    self.document_store.document.active_scene.as_deref(),
                );
                self.preview_store.preview.go_to_next_keyframe(&keyframes);
                self.preview_store.preview.status = format!(
                    "Next keyframe • t = {:.2}s / {:.2}s",
                    self.preview_store.preview.current_time_s, self.preview_store.preview.duration_s
                );
                self.preview_store.preview_dirty = true;
                if self.ui_store.editor_sync_enabled {
                    if let Some(line) = self.document_store.document.find_keyframe_line_at(self.preview_store.preview.current_time_s) {
                        self.document_store.editor.scroll_to_line(line);
                        self.document_store.editor.set_highlighted_line(Some(line));
                    }
                }
            }
            Command::PrevScene | Command::NextScene => {
                if let Some(composition) = self.document_store.document.composition.as_ref() {
                    let current_idx = self.document_store.document.active_scene.as_deref()
                        .and_then(|name| composition.declaration_order.iter().position(|n| n == name))
                        .unwrap_or(0);
                    let target_idx = if matches!(command, Command::PrevScene) {
                        current_idx.saturating_sub(1)
                    } else {
                        (current_idx + 1).min(composition.declaration_order.len().saturating_sub(1))
                    };
                    if let Some(target_name) = composition.declaration_order.get(target_idx) {
                        self.document_store.document.active_scene = Some(target_name.clone());
                        if let Some(start) = composition.scene_start_times.get(target_name) {
                            self.preview_store.preview.current_time_s = *start;
                            self.preview_store.preview.clamp_time();
                            self.preview_store.preview.is_playing = false;
                            self.preview_store.preview_dirty = true;
                            self.preview_store.preview.status = format!(
                                "Scene {} • t = {:.2}s / {:.2}s",
                                target_name, self.preview_store.preview.current_time_s, self.preview_store.preview.duration_s
                            );
                        }
                    }
                }
            }
            Command::SelectScene(scene) => {
                if let Some(composition) = self.document_store.document.composition.as_ref() {
                    if composition.scenes.contains_key(&scene) {
                        self.document_store.document.active_scene = Some(scene.clone());
                        if let Some(start) = composition.scene_start_times.get(&scene) {
                            let mut target_time = *start;
                            for edge in composition.edges.values() {
                                if edge.to_scene == scene {
                                    target_time += edge.transition.duration_ms as f64 / 1000.0;
                                    break;
                                }
                            }
                            self.preview_store.preview.current_time_s = target_time;
                            self.preview_store.preview.clamp_time();
                            self.preview_store.preview.is_playing = false;
                            self.preview_store.preview_dirty = true;
                            self.preview_store.preview.status = format!(
                                "Scene {} • t = {:.2}s / {:.2}s",
                                scene, self.preview_store.preview.current_time_s, self.preview_store.preview.duration_s
                            );
                        }
                    }
                }
            }
            Command::DeleteScene(scene) => {
                if let Some(ref mut stmts) = self.document_store.document.raw_statements {
                    let edit = crate::source_edit::SourceEdit::DeleteScene { name: scene.clone() };
                    if crate::source_edit::apply_edit(stmts, edit) {
                        let new_source = animatix::to_source::stmts_to_source(stmts);
                        self.document_store.document.source_text = new_source.clone();
                        self.document_store.editor.replace_text(new_source);
                        self.document_store.document.is_dirty = true;
                        self.document_store.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
                        self.preview_store.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
                        self.preview_store.preview.status = format!("Deleted scene {}", scene);
                        if self.document_store.document.active_scene.as_ref() == Some(&scene) {
                            self.document_store.document.active_scene = None;
                        }
                    }
                }
            }
            Command::AddScene => {
                let existing: std::collections::HashSet<String> =
                    self.document_store.document.scene_names().into_iter().collect();
                if let Some(ref mut stmts) = self.document_store.document.raw_statements {
                    let mut i = 1;
                    let new_name = loop {
                        let candidate = format!("Scene{}", i);
                        if !existing.contains(&candidate) { break candidate; }
                        i += 1;
                    };
                    let edit = crate::source_edit::SourceEdit::AddScene { name: new_name.clone() };
                    if crate::source_edit::apply_edit(stmts, edit) {
                        let new_source = animatix::to_source::stmts_to_source(stmts);
                        self.document_store.document.source_text = new_source.clone();
                        self.document_store.editor.replace_text(new_source);
                        self.document_store.document.is_dirty = true;
                        self.document_store.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
                        self.preview_store.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
                        self.preview_store.preview.status = format!("Added scene {}", new_name);
                    }
                }
            }
            Command::RenameScene { old_name, new_name } => {
                if old_name != new_name && !new_name.is_empty() {
                    if let Some(ref mut stmts) = self.document_store.document.raw_statements {
                        let edit = crate::source_edit::SourceEdit::RenameScene { old_name, new_name: new_name.clone() };
                        if crate::source_edit::apply_edit(stmts, edit) {
                            let new_source = animatix::to_source::stmts_to_source(stmts);
                            self.document_store.document.source_text = new_source.clone();
                            self.document_store.editor.replace_text(new_source);
                            self.document_store.document.is_dirty = true;
                            self.document_store.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
                            self.preview_store.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
                            self.preview_store.preview.status = format!("Renamed scene to {}", new_name);
                        }
                    }
                }
            }
            Command::ReorderScenes(new_order) => {
                if let Some(ref mut stmts) = self.document_store.document.raw_statements {
                    let edit = crate::source_edit::SourceEdit::ReorderScenes { new_order: new_order.clone() };
                    if crate::source_edit::apply_edit(stmts, edit) {
                        let new_source = animatix::to_source::stmts_to_source(stmts);
                        self.document_store.document.source_text = new_source.clone();
                        self.document_store.editor.replace_text(new_source);
                        self.document_store.document.is_dirty = true;
                        self.document_store.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
                        self.preview_store.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
                        self.preview_store.preview.status = "Reordered scenes".to_string();
                    }
                }
            }
            Command::CreateActor { ty, label, position } => {
                self.handle_create_actor(&ty, &label, position);
            }
            Command::DuplicateActor(original_label) => {
                self.handle_duplicate_actor(&original_label);
            }
            Command::DeleteSelectedActors => {
                self.handle_delete_selected_actors();
            }
            Command::PasteActors => {
                self.paste_actors();
            }
            Command::SetTransition { from_scene, transition } => {
                self.handle_set_transition(&from_scene, transition);
            }
            Command::SetPlayTarget { from_scene, target } => {
                self.handle_set_play_target(&from_scene, target);
            }
            Command::RenameActor { old_label, new_label } => {
                self.handle_rename_actor(&old_label, &new_label);
            }
            Command::SetKeyframeEasing { actor, property, time_s, easing } => {
                self.handle_set_keyframe_easing(&actor, &property, time_s, easing);
            }
            Command::DeleteKeyframe { actor, property, time_s } => {
                self.handle_delete_keyframe(&actor, &property, time_s);
            }
            Command::InspectorInputDragStarted => {
                self.ui_store.inspector_input_drag_active = true;
            }
            Command::OpenTransitionEditor(scene) => {
                self.preview_store.panel_state.open_transition_editor = Some(scene);
            }
            Command::ReparentActor { actor, new_parent } => {
                self.handle_reparent_actor(&actor, new_parent);
            }
            Command::ExtractScene { actor_labels, new_scene_name } => {
                self.handle_extract_scene(actor_labels, new_scene_name);
            }
            Command::MoveToScene { actor_labels, target_scene } => {
                self.handle_move_to_scene(actor_labels, target_scene);
            }
            Command::PropertyEdit(edit) => {
                self.handle_property_edit(edit);
            }
            Command::DragEnded => {
                self.ui_store.drag_state = DragState::None;
                self.ui_store.drag_snapshot_taken = false;
            }
            Command::InspectorInputDragEnded => {
                self.ui_store.inspector_input_drag_active = false;
                self.ui_store.drag_snapshot_taken = false;
            }
            Command::Undo => self.undo(),
            Command::Redo => self.redo(),
            Command::ScrollToLine(line) => {
                self.document_store.editor.focus_diagnostic(line, 0);
            }
        }
    }
}