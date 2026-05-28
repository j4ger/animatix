use super::*;
use crate::app::commands::{Command, Effect};

impl GuiShell {
    /// Handle a single command, returning any collected side effects.
    ///
    /// State mutations happen inline; side effects (toasts, status messages,
    /// editor scroll/highlight, etc.) are returned as a `Vec<Effect>` and
    /// applied by `apply_effects` after the handler returns.
    pub(crate) fn handle_command(&mut self, command: Command) -> Vec<Effect> {
        use crate::app::commands::Command;

        match command {
            Command::OpenFile(path) => {
                self.open_document(path);
                vec![]
            }
            Command::ToggleExpandDir(path) => {
                if self.workspace_store.expanded_dirs.contains(&path) {
                    self.workspace_store.expanded_dirs.remove(&path);
                } else {
                    self.workspace_store.expanded_dirs.insert(path.clone());
                }
                self.workspace_store.file_tree = build_file_tree(
                    &self.workspace_store.workspace_root,
                    &self.document_store.document.file_path,
                    &self.workspace_store.expanded_dirs,
                );
                vec![]
            }
            Command::ShowInspector => {
                self.open_workspace_tab(WorkspaceTab::Inspector);
                vec![]
            }
            Command::OpenExportDialog => {
                self.export_store.export_dialog_open = true;
                if self.export_store.export_state.output_path.is_empty() {
                    self.update_default_export_filename();
                }
                vec![]
            }
            Command::ToggleDiagnosticsPanel => {
                self.ui_store.view.diagnostics_panel_visible =
                    !self.ui_store.view.diagnostics_panel_visible;
                vec![]
            }
            Command::Save => {
                if let Err(e) = self.save() {
                    tracing::warn!("Save failed: {}", e);
                    vec![Effect::Toast(crate::app::components::toast::Toast::error(
                        format!("Save failed: {}", e),
                    ))]
                } else {
                    vec![
                        Effect::Status(format!("Saved {}", self.document_store.document.file_path.display())),
                        Effect::Toast(crate::app::components::toast::Toast::success(
                            format!("Saved {}", self.document_store.document.file_path.display()),
                        )),
                    ]
                }
            }
            Command::Reload => {
                if let Err(e) = self.reload() {
                    tracing::warn!("Reload failed: {}", e);
                    vec![Effect::Toast(crate::app::components::toast::Toast::error(
                        format!("Reload failed: {}", e),
                    ))]
                } else {
                    vec![]
                }
            }
            Command::Rebuild => {
                self.document_store.invalidate_cache();
                if let Err(e) = self.rebuild() {
                    tracing::warn!("Rebuild command failed: {}", e);
                }
                vec![]
            }
            Command::ScrubTo(next_time) => {
                self.preview_store.preview.playback.current_time_s = next_time;
                self.preview_store.preview.playback.clamp_time();
                self.preview_store.preview.playback.is_playing = false;
                self.preview_store.preview_dirty = true;
                self.sync_active_scene_from_time();
                let mut effects: Vec<Effect> = vec![];
                if self.ui_store.editor_sync_enabled {
                    if let Some(line) =
                        self.document_store.document.find_keyframe_line_at(next_time)
                    {
                        self.document_store.editor.scroll_to_line(line);
                        self.document_store.editor.set_highlighted_line(Some(line));
                        effects.push(Effect::EditorScroll(line));
                        effects.push(Effect::EditorHighlight(line));
                    }
                }
                effects
            }
            Command::TogglePlayback => {
                self.preview_store.preview.playback.toggle_playback();
                self.preview_store.preview_dirty = true;
                vec![Effect::Repaint]
            }
            Command::ToggleEditorSync => {
                self.ui_store.editor_sync_enabled = !self.ui_store.editor_sync_enabled;
                vec![if self.ui_store.editor_sync_enabled {
                    Effect::Status("Editor sync ON".to_string())
                } else {
                    Effect::Status("Editor sync OFF".to_string())
                }]
            }
            Command::ToggleKeyframeMode => {
                // Global keyframe mode toggle removed in GUI redesign Phase 1.
                // Per-property diamond controls will replace this in Phase 2.
                vec![]
            }
            Command::EditorChanged => {
                self.document_store
                    .document
                    .set_source_text(self.document_store.editor.text().to_string());
                self.preview_store.pending_rebuild_at =
                    Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
                self.preview_store.preview.error = None;
                self.document_store.document.diagnostics.clear();
                vec![
                    Effect::Status("Editing source • rebuild scheduled".to_string()),
                    Effect::RebuildScheduled,
                ]
            }
            Command::RequestRepaint => {
                self.preview_store.preview_dirty = true;
                vec![Effect::Repaint]
            }
            Command::PrevKeyframe => {
                let keyframes = timeline_keyframe_times_s(
                    if self.document_store.document.composition.is_some() {
                        None
                    } else {
                        self.document_store.document.active_timeline()
                    },
                    self.document_store.document.composition.as_ref(),
                    self.document_store.document.active_scene.as_deref(),
                );
                self.preview_store.preview.playback.go_to_previous_keyframe(&keyframes);
                self.preview_store.preview_dirty = true;
                let status = format!(
                    "Previous keyframe • t = {:.2}s / {:.2}s",
                    self.preview_store.preview.playback.current_time_s,
                    self.preview_store.preview.playback.duration_s
                );
                let mut effects = vec![Effect::Status(status)];
                if self.ui_store.editor_sync_enabled {
                    if let Some(line) = self.document_store.document.find_keyframe_line_at(
                        self.preview_store.preview.playback.current_time_s,
                    ) {
                        self.document_store.editor.scroll_to_line(line);
                        self.document_store.editor.set_highlighted_line(Some(line));
                        effects.push(Effect::EditorScroll(line));
                        effects.push(Effect::EditorHighlight(line));
                    }
                }
                effects
            }
            Command::NextKeyframe => {
                let keyframes = timeline_keyframe_times_s(
                    if self.document_store.document.composition.is_some() {
                        None
                    } else {
                        self.document_store.document.active_timeline()
                    },
                    self.document_store.document.composition.as_ref(),
                    self.document_store.document.active_scene.as_deref(),
                );
                self.preview_store.preview.playback.go_to_next_keyframe(&keyframes);
                self.preview_store.preview_dirty = true;
                let status = format!(
                    "Next keyframe • t = {:.2}s / {:.2}s",
                    self.preview_store.preview.playback.current_time_s,
                    self.preview_store.preview.playback.duration_s
                );
                let mut effects = vec![Effect::Status(status)];
                if self.ui_store.editor_sync_enabled {
                    if let Some(line) = self.document_store.document.find_keyframe_line_at(
                        self.preview_store.preview.playback.current_time_s,
                    ) {
                        self.document_store.editor.scroll_to_line(line);
                        self.document_store.editor.set_highlighted_line(Some(line));
                        effects.push(Effect::EditorScroll(line));
                        effects.push(Effect::EditorHighlight(line));
                    }
                }
                effects
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
                            self.preview_store.preview.playback.current_time_s = *start;
                            self.preview_store.preview.playback.clamp_time();
                            self.preview_store.preview.playback.is_playing = false;
                            self.preview_store.preview_dirty = true;
                            return vec![Effect::Status(format!(
                                "Scene {} • t = {:.2}s / {:.2}s",
                                target_name,
                                self.preview_store.preview.playback.current_time_s,
                                self.preview_store.preview.playback.duration_s
                            ))];
                        }
                    }
                }
                vec![]
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
                            self.preview_store.preview.playback.current_time_s = target_time;
                            self.preview_store.preview.playback.clamp_time();
                            self.preview_store.preview.playback.is_playing = false;
                            self.preview_store.preview_dirty = true;
                            return vec![Effect::Status(format!(
                                "Scene {} • t = {:.2}s / {:.2}s",
                                scene,
                                self.preview_store.preview.playback.current_time_s,
                                self.preview_store.preview.playback.duration_s
                            ))];
                        }
                    }
                }
                vec![]
            }
            Command::DeleteScene(scene) => {
                if let Some(ref mut stmts) = self.document_store.document.raw_statements {
                    let edit = crate::source_edit::SourceEdit::DeleteScene { name: scene.clone() };
                    if crate::source_edit::apply_edit(stmts, edit) {
                        let new_source = animatix::to_source::stmts_to_source(stmts);
                        self.document_store.document.source_text = new_source.clone();
                        self.document_store.editor.replace_text(new_source);
                        self.document_store.document.is_dirty = true;
                        self.document_store.document.source_index =
                            Some(animatix::source_index::SourceIndex::build(stmts));
                        self.preview_store.pending_rebuild_at =
                            Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
                        if self.document_store.document.active_scene.as_ref() == Some(&scene) {
                            self.document_store.document.active_scene = None;
                        }
                        return vec![Effect::Status(format!("Deleted scene {}", scene))];
                    }
                }
                vec![]
            }
            Command::AddScene => {
                let existing: std::collections::HashSet<String> =
                    self.document_store.document.scene_names().into_iter().collect();
                if let Some(ref mut stmts) = self.document_store.document.raw_statements {
                    let mut i = 1;
                    let new_name = loop {
                        let candidate = format!("Scene{}", i);
                        if !existing.contains(&candidate) {
                            break candidate;
                        }
                        i += 1;
                    };
                    let edit = crate::source_edit::SourceEdit::AddScene { name: new_name.clone() };
                    if crate::source_edit::apply_edit(stmts, edit) {
                        let new_source = animatix::to_source::stmts_to_source(stmts);
                        self.document_store.document.source_text = new_source.clone();
                        self.document_store.editor.replace_text(new_source);
                        self.document_store.document.is_dirty = true;
                        self.document_store.document.source_index =
                            Some(animatix::source_index::SourceIndex::build(stmts));
                        self.preview_store.pending_rebuild_at =
                            Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
                        return vec![Effect::Status(format!("Added scene {}", new_name))];
                    }
                }
                vec![]
            }
            Command::RenameScene { old_name, new_name } => {
                if old_name != new_name && !new_name.is_empty() {
                    if let Some(ref mut stmts) = self.document_store.document.raw_statements {
                        let edit = crate::source_edit::SourceEdit::RenameScene {
                            old_name,
                            new_name: new_name.clone(),
                        };
                        if crate::source_edit::apply_edit(stmts, edit) {
                            let new_source = animatix::to_source::stmts_to_source(stmts);
                            self.document_store.document.source_text = new_source.clone();
                            self.document_store.editor.replace_text(new_source);
                            self.document_store.document.is_dirty = true;
                            self.document_store.document.source_index =
                                Some(animatix::source_index::SourceIndex::build(stmts));
                            self.preview_store.pending_rebuild_at =
                                Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
                            return vec![Effect::Status(format!("Renamed scene to {}", new_name))];
                        }
                    }
                }
                vec![]
            }
            Command::ReorderScenes(new_order) => {
                if let Some(ref mut stmts) = self.document_store.document.raw_statements {
                    let edit = crate::source_edit::SourceEdit::ReorderScenes {
                        new_order: new_order.clone(),
                    };
                    if crate::source_edit::apply_edit(stmts, edit) {
                        let new_source = animatix::to_source::stmts_to_source(stmts);
                        self.document_store.document.source_text = new_source.clone();
                        self.document_store.editor.replace_text(new_source);
                        self.document_store.document.is_dirty = true;
                        self.document_store.document.source_index =
                            Some(animatix::source_index::SourceIndex::build(stmts));
                        self.preview_store.pending_rebuild_at =
                            Some(Instant::now() + Duration::from_millis(self.ui_store.rebuild_debounce_ms));
                        return vec![Effect::Status("Reordered scenes".to_string())];
                    }
                }
                vec![]
            }
            Command::CreateActor { ty, label, position } => {
                self.handle_create_actor(&ty, &label, position);
                vec![]
            }
            Command::DuplicateActor(original_label) => {
                self.handle_duplicate_actor(&original_label);
                vec![]
            }
            Command::DeleteSelectedActors => {
                self.handle_delete_selected_actors();
                vec![]
            }
            Command::PasteActors => {
                self.paste_actors();
                vec![]
            }
            Command::SetTransition { from_scene, transition } => {
                self.handle_set_transition(&from_scene, transition);
                vec![]
            }
            Command::SetPlayTarget { from_scene, target } => {
                self.handle_set_play_target(&from_scene, target);
                vec![]
            }
            Command::RenameActor { old_label, new_label } => {
                self.handle_rename_actor(&old_label, &new_label);
                vec![]
            }
            Command::SetKeyframeEasing {
                actor,
                property,
                time_s,
                easing,
            } => {
                self.handle_set_keyframe_easing(&actor, &property, time_s, easing);
                vec![]
            }
            Command::DeleteKeyframe {
                actor,
                property,
                time_s,
            } => {
                self.handle_delete_keyframe(&actor, &property, time_s);
                vec![]
            }
            Command::MoveKeyframe {
                actor,
                property,
                old_time_s,
                new_time_s,
            } => {
                // Phase 4b: placeholder — will emit source edit when document handler is wired in
                tracing::info!("MoveKeyframe: {actor}.{property} {old_time_s}s → {new_time_s}s");
                vec![]
            }
            Command::InspectorInputDragStarted => {
                self.ui_store.interaction.inspector_input_drag_active = true;
                vec![]
            }
            Command::ReparentActor { actor, new_parent } => {
                self.handle_reparent_actor(&actor, new_parent);
                vec![]
            }
            Command::ExtractScene {
                actor_labels,
                new_scene_name,
            } => {
                self.handle_extract_scene(actor_labels, new_scene_name);
                vec![]
            }
            Command::MoveToScene {
                actor_labels,
                target_scene,
            } => {
                self.handle_move_to_scene(actor_labels, target_scene);
                vec![]
            }
            Command::PropertyEdit(edit) => {
                self.handle_property_edit(edit);
                vec![]
            }
            Command::DragEnded => {
                self.ui_store.interaction.drag_state = DragState::None;
                self.ui_store.interaction.drag_snapshot_taken = false;
                vec![]
            }
            Command::InspectorInputDragEnded => {
                self.ui_store.interaction.inspector_input_drag_active = false;
                self.ui_store.interaction.drag_snapshot_taken = false;
                vec![]
            }
            Command::Undo => {
                self.undo();
                vec![]
            }
            Command::Redo => {
                self.redo();
                vec![]
            }
            Command::ScrollToLine(line) => {
                self.document_store.editor.focus_diagnostic(line, 0);
                vec![]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::{Command, Effect};
    use crate::app::persistence::default_tree;
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Create a unique temp directory for test isolation.
    fn temp_project_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "animatix_gui_test_{}_{}_{}",
            name,
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Build a minimal GuiShell for testing command handlers.
    fn make_test_shell() -> GuiShell {
        let dir = temp_project_dir("command_handlers");
        let path = dir.join("test.amx");
        fs::write(&path, "box: Rect, size: (100, 100)\n").unwrap();

        let document = DocumentSession::load(path.clone()).expect("load test document");
        let editor = EditorBuffer::new(&path, document.source_text.clone());

        let workspace_root = dir;
        let expanded_dirs = HashSet::from([workspace_root.clone()]);
        let file_tree = build_file_tree(&workspace_root, &path, &expanded_dirs);

        let preview = PreviewPaneState::new(
            document.duration_s.max(0.1),
            document.scene_dimensions,
        );

        let tree = default_tree();

        GuiShell {
            document_store: DocumentStore::new(document, editor),
            workspace_store: WorkspaceStore::new(
                workspace_root,
                expanded_dirs,
                file_tree,
                PathBuf::from(".test_persistence.ron"),
                None,
            ),
            preview_store: PreviewStore::new(preview),
            ui_store: UiStore::new(tree),
            export_store: ExportStore::new(),
        }
    }

    // ── TogglePlayback ───────────────────────────────────────────────

    #[test]
    fn toggle_playback_returns_repaint_effect() {
        let mut shell = make_test_shell();
        let effects = shell.handle_command(Command::TogglePlayback);
        assert_eq!(effects.len(), 1, "expected exactly 1 effect");
        assert!(
            matches!(&effects[0], Effect::Repaint),
            "expected Repaint effect"
        );
    }

    #[test]
    fn toggle_playback_toggles_playing_flag() {
        let mut shell = make_test_shell();
        assert!(!shell.preview_store.preview.playback.is_playing);
        shell.handle_command(Command::TogglePlayback);
        assert!(shell.preview_store.preview.playback.is_playing);
        shell.handle_command(Command::TogglePlayback);
        assert!(!shell.preview_store.preview.playback.is_playing);
    }

    #[test]
    fn toggle_playback_resets_time_when_at_end() {
        let mut shell = make_test_shell();
        shell.preview_store.preview.playback.current_time_s =
            shell.preview_store.preview.playback.duration_s;
        shell.handle_command(Command::TogglePlayback);
        assert_eq!(shell.preview_store.preview.playback.current_time_s, 0.0);
        assert!(shell.preview_store.preview.playback.is_playing);
    }

    // ── ScrubTo ──────────────────────────────────────────────────────

    #[test]
    fn scrub_to_updates_current_time_and_stops_playback() {
        let mut shell = make_test_shell();
        // Document has no keyframes → duration is 0.1s min, so use a time within range
        let target = shell.preview_store.preview.playback.duration_s * 0.5;
        let clamped = target.max(0.0).min(shell.preview_store.preview.playback.duration_s.max(0.1));
        shell.preview_store.preview.playback.is_playing = true;

        let _effects = shell.handle_command(Command::ScrubTo(target));

        assert_eq!(shell.preview_store.preview.playback.current_time_s, clamped);
        assert!(!shell.preview_store.preview.playback.is_playing);
        assert!(shell.preview_store.preview_dirty);
    }

    #[test]
    fn scrub_to_clamps_negative_time() {
        let mut shell = make_test_shell();
        shell.handle_command(Command::ScrubTo(-5.0));
        assert_eq!(shell.preview_store.preview.playback.current_time_s, 0.0);
    }

    #[test]
    fn scrub_to_clamps_overshoot_time() {
        let mut shell = make_test_shell();
        shell.handle_command(Command::ScrubTo(999.0));
        let max = shell.preview_store.preview.playback.duration_s.max(0.1);
        assert_eq!(shell.preview_store.preview.playback.current_time_s, max);
    }

    // ── ToggleEditorSync ─────────────────────────────────────────────

    #[test]
    fn toggle_editor_sync_turns_off_when_on() {
        let mut shell = make_test_shell();
        shell.ui_store.editor_sync_enabled = true;
        let effects = shell.handle_command(Command::ToggleEditorSync);
        assert!(!shell.ui_store.editor_sync_enabled);
        assert_eq!(effects.len(), 1);
        assert!(
            matches!(&effects[0], Effect::Status(msg) if msg == "Editor sync OFF"),
            "expected Status('Editor sync OFF'), got {:?}",
            effects[0]
        );
    }

    #[test]
    fn toggle_editor_sync_turns_on_when_off() {
        let mut shell = make_test_shell();
        shell.ui_store.editor_sync_enabled = false;
        let effects = shell.handle_command(Command::ToggleEditorSync);
        assert!(shell.ui_store.editor_sync_enabled);
        assert_eq!(effects.len(), 1);
        assert!(
            matches!(&effects[0], Effect::Status(msg) if msg == "Editor sync ON"),
            "expected Status('Editor sync ON'), got {:?}",
            effects[0]
        );
    }

    // ── RequestRepaint ───────────────────────────────────────────────

    #[test]
    fn request_repaint_returns_repaint_effect() {
        let mut shell = make_test_shell();
        let effects = shell.handle_command(Command::RequestRepaint);
        assert_eq!(effects.len(), 1, "expected exactly 1 effect");
        assert!(
            matches!(&effects[0], Effect::Repaint),
            "expected Repaint effect"
        );
    }

    #[test]
    fn request_repaint_sets_preview_dirty() {
        let mut shell = make_test_shell();
        shell.preview_store.preview_dirty = false;
        shell.handle_command(Command::RequestRepaint);
        assert!(shell.preview_store.preview_dirty);
    }

    // ── Save ─────────────────────────────────────────────────────────

    #[test]
    fn save_returns_status_and_toast_effects_on_success() {
        let mut shell = make_test_shell();
        let effects = shell.handle_command(Command::Save);
        assert_eq!(effects.len(), 2, "expected 2 effects from Save");
        assert!(
            matches!(&effects[0], Effect::Status(msg) if msg.starts_with("Saved ")),
            "expected Effect::Status starting with 'Saved ', got {:?}",
            effects[0]
        );
        assert!(
            matches!(&effects[1], Effect::Toast(_)),
            "expected Effect::Toast as second effect, got {:?}",
            effects[1]
        );
    }

    #[test]
    fn save_persists_file_to_disk() {
        let mut shell = make_test_shell();
        let path = shell.document_store.document.file_path.clone();
        let before = std::fs::read_to_string(&path).unwrap();
        shell.document_store
            .document
            .set_source_text("modified content".to_string());
        shell.document_store
            .editor
            .replace_text("modified content".to_string());

        shell.handle_command(Command::Save);

        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            after, "modified content",
            "file content should match editor state after save"
        );
        assert!(
            !shell.document_store.document.is_dirty,
            "document should not be dirty after save"
        );
        assert_eq!(
            shell.document_store.document.source_text,
            "modified content",
            "source_text should reflect saved content"
        );
        assert_ne!(before, after, "file should have changed after save");
    }

    // ── EditorChanged ────────────────────────────────────────────────

    #[test]
    fn editor_changed_returns_status_and_rebuild_scheduled() {
        let mut shell = make_test_shell();
        let effects = shell.handle_command(Command::EditorChanged);
        assert_eq!(effects.len(), 2);
        assert!(
            matches!(&effects[0], Effect::Status(msg) if msg.contains("Editing source")),
            "expected Status about editing source, got {:?}",
            effects[0]
        );
        assert!(
            matches!(&effects[1], Effect::RebuildScheduled),
            "expected RebuildScheduled, got {:?}",
            effects[1]
        );
    }

    // ── ToggleKeyframeMode ───────────────────────────────────────────

    #[test]
    fn toggle_keyframe_mode_returns_empty() {
        let mut shell = make_test_shell();
        let effects = shell.handle_command(Command::ToggleKeyframeMode);
        assert!(effects.is_empty());
    }

    // ── ShowInspector ────────────────────────────────────────────────

    #[test]
    fn show_inspector_returns_empty() {
        let mut shell = make_test_shell();
        let effects = shell.handle_command(Command::ShowInspector);
        assert!(effects.is_empty());
    }

    // ── ToggleDiagnosticsPanel ───────────────────────────────────────

    #[test]
    fn toggle_diagnostics_toggles_visibility_flag() {
        let mut shell = make_test_shell();
        shell.ui_store.view.diagnostics_panel_visible = false;
        let effects = shell.handle_command(Command::ToggleDiagnosticsPanel);
        assert!(shell.ui_store.view.diagnostics_panel_visible);
        assert!(effects.is_empty());
    }

    // ── DragEnded ────────────────────────────────────────────────────

    #[test]
    fn drag_ended_resets_drag_state() {
        let mut shell = make_test_shell();
        shell.ui_store.interaction.drag_snapshot_taken = true;

        let effects = shell.handle_command(Command::DragEnded);

        assert!(
            matches!(shell.ui_store.interaction.drag_state, super::preview::DragState::None),
            "expected DragState::None after DragEnded"
        );
        assert!(!shell.ui_store.interaction.drag_snapshot_taken);
        assert!(effects.is_empty());
    }
}