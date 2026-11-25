use super::*;
use crate::app::*;

impl GuiShell {
    /// True when any modal/overlay other than the compact drawers is open.
    pub(crate) fn modal_open(&self) -> bool {
        self.ui_store.view.settings_open
            || self.ui_store.view.workspace_switcher_open
            || self.export_store.export_dialog_open
            || self.ui_store.view.shortcuts_open
            || self.ui_store.view.plugin_status_open
            || self.ui_store.view.command_palette_open
            || self.ui_store.view.find_replace_open
            || self.ui_store.unsaved_changes.is_open
            || self.ui_store.recovery_prompt.is_open
    }

    /// Return a cloneable sender for commands submitted outside egui callbacks.
    ///
    /// Used by the demo-script driver in `runtime/run.rs`; the queue is owned
    /// by the shell and drained every frame.
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
                Effect::Notice(level, message) => {
                    self.ui_store.toasts.push(Toast::new(
                        message.display(self.ui_store.view.language).into_owned(),
                        level,
                    ));
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
            }
        }
    }

    /// Force-clear any active error state (parse or render).
    pub(crate) fn clear_any_error(&mut self) {
        self.document_store.history.render_diagnostics.clear();
        self.document_store.history.runtime_diagnostics.clear();
        self.preview_store.preview.error = None;
        self.preview_store.preview.status.clear();
    }

    pub(crate) fn save_persistence(&self) {
        if let Some(parent) = self.workspace_store.persistence_path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            tracing::warn!("Failed to create persistence directory: {}", e);
        }
        let mut tree = self.ui_store.view.tree.clone();
        if let Some(restore) = self.ui_store.view.compact_restore {
            crate::app::persistence::restore_compact_tree(&mut tree, restore);
        }
        let persistence = WorkspacePersistence {
            tree,
            layout_version: crate::app::persistence::WORKSPACE_LAYOUT_VERSION,
            layout_preset: Some(self.ui_store.view.layout_preset),
            window_size: Some(self.window_size),
            window_maximized: Some(self.window_maximized),
            settings: Some(SettingsPersistence {
                interface_language: self.ui_store.view.language.key().to_owned(),
                rebuild_debounce_ms: self.ui_store.rebuild_debounce_ms,
                scrub_step_s: self.ui_store.scrub_step_s,
                snap_fps: self.ui_store.snap_fps,
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
            && let Err(e) = fs::write(&self.workspace_store.persistence_path, serialized)
        {
            tracing::warn!("Failed to write persistence file: {}", e);
        }
    }

    pub(crate) fn sync_active_scene_from_time(&mut self) {
        if let Some(composition) = self.document_store.source.document.composition.as_ref() {
            let previous_scene = self.document_store.source.document.active_scene.clone();
            let scene = crate::app::document::review_scene_at_time(
                composition,
                self.preview_store.preview.playback.current_time_s(),
            );
            if scene != previous_scene {
                // Keyframe selections are scene-qualified; automatic playback
                // crossing a scene boundary must not replay old-scene entries.
                self.ui_store.selection.selected_keyframes.clear();
            }
            self.document_store.source.document.active_scene = scene;
        }
    }

    pub(crate) fn set_status(
        &mut self,
        status: impl Into<crate::app::i18n::StatusText>,
        error: Option<String>,
    ) {
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
            self.ui_store.toasts.push(Toast::warning(
                crate::app::i18n::StatusText::message(
                    "gui-file-plugin-reload",
                    [("detail", message)],
                )
                .display(self.ui_store.view.language)
                .into_owned(),
            ));
            self.preview_store
                .preview
                .set_status_info(crate::app::i18n::StatusText::message(
                    "gui-status-plugins-reloaded-issues",
                    [],
                ));
        } else {
            self.preview_store
                .preview
                .set_status_info(crate::app::i18n::StatusText::message(
                    "gui-status-plugins-reloaded",
                    [],
                ));
        }
    }

    pub(crate) fn set_render_error(&mut self, error: String) {
        self.document_store.history.render_diagnostics = vec![Diagnostic::error(
            DiagnosticCode::RenderFailure,
            DiagnosticPhase::Render,
            error.clone(),
        )];
        self.preview_store.preview_dirty = false;
        self.set_status(
            crate::app::i18n::StatusText::message(
                "gui-status-render-failed",
                [("detail", error.clone())],
            ),
            Some(error),
        );
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
            && self.preview_store.preview.status.canonical()
                == format!("Render failed • {render_error}")
        {
            self.preview_store.preview.error = None;
            self.preview_store.preview.status = status.into();
        }
    }
}
