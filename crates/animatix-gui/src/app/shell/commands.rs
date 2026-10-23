use super::*;
use crate::app::*;

impl GuiShell {
    /// True when any modal/overlay other than the compact drawers is open.
    pub(crate) fn modal_open(&self) -> bool {
        self.ui_store.view.settings_open
            || self.ui_store.view.workspace_switcher_open
            || self.export_store.export_dialog_open
            || self.insertion_palette.open
            || self.ui_store.view.shortcuts_open
            || self.ui_store.view.plugin_status_open
            || self.ui_store.view.command_palette_open
            || self.ui_store.view.find_replace_open
            || self.ui_store.unsaved_changes.is_open
            || self.ui_store.recovery_prompt.is_open
    }

    /// Return a cloneable sender for commands submitted outside egui callbacks.
    ///
    /// Reserved for integration tests and future remote control; the queue is
    /// owned by the shell and drained every frame.
    #[allow(dead_code)] // Exposed for integration tests and future remote-control wiring.
    pub(crate) fn external_command_sender(&self) -> CommandSender {
        self.external_commands.sender()
    }

    pub(crate) fn handle_actions(&mut self, actions: ActionQueue) {
        for action in actions {
            let effects = self.handle_action(action);
            self.apply_effects(effects);
        }
    }

    /// Apply a collection of side effects produced by a command handler.
    ///
    /// Effects are applied *after* all state mutations for the command have been
    /// performed, ensuring that side-effect code (UI toasts, status text, editor
    /// sync, etc.) runs in a consistent state.
    pub(crate) fn apply_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Toast(toast) => {
                    self.ui_store.toasts.push(toast);
                },
                Effect::Status(status) => {
                    self.preview_store.preview.set_status_info(status);
                },
                Effect::Repaint => {
                    self.preview_store.preview_dirty = true;
                },
                Effect::EditorScroll(line) => {
                    self.document_store.source.editor.scroll_to_line(line);
                },
                Effect::EditorHighlight(line) => {
                    self.document_store.source.editor.set_highlighted_line(Some(line));
                },
                Effect::RebuildScheduled => {
                    // The status has already been set; the pending rebuild
                    // timer is set directly in the command handler.
                },
            }
        }
    }

    /// Force-clear any active error state (parse or render).
    pub(crate) fn clear_any_error(&mut self, status: String) {
        self.document_store.history.render_diagnostics.clear();
        self.document_store.history.runtime_diagnostics.clear();
        self.preview_store.preview.error = None;
        self.preview_store.preview.status = status;
    }

    pub(crate) fn save_persistence(&self) {
        if let Some(parent) = self.workspace_store.persistence_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                tracing::warn!("Failed to create persistence directory: {}", e);
            }
        }
        let persistence = WorkspacePersistence {
            tree: self.ui_store.view.tree.clone(),
            window_size: Some(self.window_size),
            window_maximized: Some(self.window_maximized),
            settings: Some(SettingsPersistence {
                rebuild_debounce_ms: self.ui_store.rebuild_debounce_ms,
                scrub_step_s: self.ui_store.scrub_step_s,
                nudge_step_px: self.ui_store.nudge_step_px,
                nudge_step_shift_px: self.ui_store.nudge_step_shift_px,
                rotation_snap_degrees: self.ui_store.rotation_snap_degrees,
                snap_fps: self.ui_store.snap_fps,
                keyframe_merge_window_s: self.ui_store.keyframe_merge_window_s,
                undo_limit: self.document_store.history.undo_limit,
                grid_size: self.preview_store.preview.overlay.grid_size,
                app_theme: match self.ui_store.view.app_theme {
                    eparts::AppThemeChoice::Light => "light",
                    eparts::AppThemeChoice::Dark => "dark",
                    eparts::AppThemeChoice::Auto => "auto",
                }
                .to_string(),
                reduce_motion: self.ui_store.view.reduce_motion,
                density: match self.ui_store.view.density {
                    eparts::Density::Compact => "compact",
                    eparts::Density::Default => "default",
                }
                .to_string(),
                theme_dir: self.ui_store.view.theme_dir.clone(),
                theme_name: self.ui_store.view.theme_name.clone(),
                shortcuts: self.ui_store.shortcut_overrides.clone(),
                plugin_paths: self.plugin_manager.explicit_plugin_paths(),
            }),
        };
        if let Ok(serialized) =
            ron::ser::to_string_pretty(&persistence, ron::ser::PrettyConfig::default())
        {
            if let Err(e) = fs::write(&self.workspace_store.persistence_path, serialized) {
                tracing::warn!("Failed to write persistence file: {}", e);
            }
        }
    }

    /// Take a snapshot of the current source text for undo/redo.
    /// Call this BEFORE making a change to the source.
    pub(crate) fn snapshot(&mut self, label: UndoLabel) {
        let ui_before = self.ui_store.snapshot_with_preview(&self.preview_store);
        self.document_store.snapshot(label, ui_before);
    }

    pub(crate) fn sync_active_scene_from_time(&mut self) {
        if let Some(composition) = self.document_store.source.document.composition.as_ref() {
            let previous_scene = self.document_store.source.document.active_scene.clone();
            let (scene, _, _) =
                composition.evaluate(self.preview_store.preview.playback.current_time_s());
            let scene = (!scene.is_empty()).then_some(scene);
            if scene != previous_scene {
                // Keyframe selections are scene-qualified; automatic playback
                // crossing a scene boundary must not replay old-scene entries.
                self.ui_store.selection.selected_keyframes.clear();
            }
            self.document_store.source.document.active_scene = scene;
        }
    }

    pub(crate) fn set_status(&mut self, status: String, error: Option<String>) {
        if error.is_some() {
            self.preview_store.preview.set_status_error(status);
        } else {
            self.preview_store.preview.set_status_info(status);
        }
        self.preview_store.preview.error = error;
    }

    /// Apply a plugin reload to the current document and schedule a rebuild.
    pub(crate) fn apply_plugin_reload(&mut self) {
        let context = self.plugin_manager.context();
        let manifest = self.plugin_manager.manifest();
        let epoch = self.plugin_manager.epoch();
        self.document_store
            .source
            .document
            .set_extension_context(context, manifest, epoch);
        self.preview_store.pending_rebuild_at = Some(
            std::time::Instant::now()
                + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
        );
        let issues = self.plugin_manager.snapshot().issues;
        if !issues.is_empty() {
            let message = issues
                .iter()
                .map(|issue| {
                    issue
                        .path
                        .as_ref()
                        .map(|path| format!("{}: {}", path.display(), issue.message))
                        .unwrap_or_else(|| issue.message.clone())
                })
                .collect::<Vec<_>>()
                .join("; ");
            self.ui_store
                .toasts
                .push(Toast::warning(format!("Plugin reload issue: {message}")));
            self.preview_store
                .preview
                .set_status_info("Plugin reload completed with issues");
        } else {
            self.preview_store.preview.set_status_info("Plugin reload completed");
        }
    }

    pub(crate) fn set_render_error(&mut self, error: String) {
        self.document_store.history.render_diagnostics = vec![Diagnostic::error(
            DiagnosticCode::RenderFailure,
            DiagnosticPhase::Render,
            error.clone(),
        )];
        self.preview_store.preview_dirty = false;
        self.set_status(format!("Render failed • {error}"), Some(error));
    }

    #[cfg(test)]
    pub(crate) fn clear_render_error(&mut self, status: String) {
        let active_render_error = self
            .document_store
            .history
            .render_diagnostics
            .first()
            .map(|diagnostic| diagnostic.message.clone());
        self.document_store.history.render_diagnostics.clear();

        if let Some(render_error) = active_render_error
            && self.preview_store.preview.error.as_deref() == Some(render_error.as_str())
            && self.preview_store.preview.status == format!("Render failed • {render_error}")
        {
            self.preview_store.preview.error = None;
            self.preview_store.preview.status = status;
        }
    }

    /// Copy currently selected actor labels into the clipboard buffer.
    pub(crate) fn copy_selected_actors(&mut self) {
        let count = self.ui_store.selection.selected_actors.len();
        self.ui_store.clipboard.clipboard_actors =
            self.ui_store.selection.selected_actors.iter().cloned().collect();
        self.preview_store.preview.status = format!("Copied {} actor(s)", count);
    }
}
