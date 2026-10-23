use super::*;
use crate::app::*;

impl GuiShell {
    pub(crate) fn check_hot_reload(&mut self, app_time: Instant) {
        if let Some(ref mut reloader) = self.workspace_store.hot_reloader {
            match reloader.update(app_time) {
                ReloadStatus::ShouldReload { path: _ } => {
                    // LiveDocument: editor is the source of truth. If the editor
                    // has unsaved changes, route through the unsaved-changes dialog so
                    // the user can Save (which overwrites the on-disk edit) or Discard
                    // (which loads the external change). Guard with is_open to avoid
                    // re-prompting every frame while the watcher keeps firing.
                    if self.document_store.source.document.is_dirty {
                        if !self.ui_store.unsaved_changes.is_open {
                            self.ui_store.unsaved_changes.open(
                                "File changed on disk. Reload and discard your unsaved edits?",
                                DocumentCommand::Reload.into(),
                            );
                        }
                        return;
                    }
                    if let Err(err) = self.document_store.source.document.reload_from_disk() {
                        self.preview_store.preview.error = Some(err.to_string());
                        self.preview_store.preview.set_status_error("Hot reload failed");
                    } else {
                        self.document_store.source.invalidate_cache();
                        self.document_store.source.editor.set_document(
                            &self.document_store.source.document.file_path,
                            self.document_store.source.document.source_text.clone(),
                        );
                        self.workspace_store.last_reload_time = Some(app_time);
                        self.preview_store.preview.status = "File reloaded; rebuilding".to_string();
                        self.preview_store.preview.error = None;
                        self.preview_store.pending_rebuild_at = Some(
                            std::time::Instant::now()
                                + std::time::Duration::from_millis(
                                    self.ui_store.rebuild_debounce_ms,
                                ),
                        );
                    }
                },
                ReloadStatus::NoChange => {},
            }
        }
    }

    pub(crate) fn load(initial_path: PathBuf, show_welcome: bool) -> Self {
        let workspace_root = workspace_root_for(&initial_path);
        let persistence_path = persistence_path();
        let persistence = load_workspace_persistence(&persistence_path);
        let plugin_paths = persistence
            .as_ref()
            .and_then(|p| p.settings.as_ref())
            .map(|settings| settings.plugin_paths.clone())
            .unwrap_or_default();
        let plugin_manager = DocumentPluginManager::new_with_explicit_paths(
            initial_path.clone(),
            workspace_root.clone(),
            plugin_paths,
        );
        let (mut document, status, error, is_welcome) = if show_welcome {
            // No recent file persisted — show welcome screen
            let doc = DocumentSession::from_error(initial_path.clone());
            (doc, None, None, true)
        } else {
            let context = plugin_manager.context();
            let manifest = plugin_manager.manifest();
            match DocumentSession::load_with_extension(
                initial_path.clone(),
                context,
                manifest,
                Some(workspace_root.clone()),
            ) {
                Ok(document) => {
                    let error = document.last_rebuild_error.clone();
                    (document, None, error, false)
                },
                Err(error) => {
                    // Persisted file missing/deleted — fall back to welcome
                    let doc = DocumentSession::from_error(initial_path.clone());
                    (doc, None, Some(error.to_string()), true)
                },
            }
        };

        document.set_plugin_epoch(plugin_manager.epoch());

        let expanded_dirs = HashSet::from([workspace_root.clone()]);
        let file_tree = build_file_tree(&workspace_root, &document.file_path, &expanded_dirs);
        let tree = persistence.as_ref().map(|p| p.tree.clone()).unwrap_or_else(default_tree);
        let window_size =
            persistence.as_ref().and_then(|p| p.window_size).unwrap_or([1440.0, 960.0]);
        let window_maximized =
            persistence.as_ref().and_then(|p| p.window_maximized).unwrap_or(false);
        let (hot_reloader, hot_reload_error) = match HotReloader::new(&document.file_path) {
            Ok(reloader) => (Some(reloader), None),
            Err(err) => (None, Some(err)),
        };
        let duration_s = document.duration_s.max(0.1);
        let mut preview = PreviewPaneState::new(duration_s, document.scene_dimensions);
        if let Some(status) = status {
            preview.status = status;
        } else if has_source_load_failure(&document.diagnostics) {
            preview.set_status_error(format!(
                "Opened {} • parse/load error • {}",
                document.file_path.display(),
                diagnostics_phase_summary(&document.diagnostics)
            ));
        }
        preview.error = error.clone();

        let editor = EditorBuffer::new(&document.file_path, document.source_text.clone());

        let mut ui_store = UiStore::new(tree);
        ui_store.recent_files = crate::app::persistence::load_recent_files();
        ui_store.view.welcome_open = is_welcome;

        // Apply persisted settings
        if let Some(s) = persistence.as_ref().and_then(|p| p.settings.as_ref()) {
            ui_store.rebuild_debounce_ms = s.rebuild_debounce_ms;
            ui_store.scrub_step_s = s.scrub_step_s;
            ui_store.nudge_step_px = s.nudge_step_px;
            ui_store.nudge_step_shift_px = s.nudge_step_shift_px;
            ui_store.rotation_snap_degrees = s.rotation_snap_degrees;
            ui_store.snap_fps = s.snap_fps;
            ui_store.keyframe_merge_window_s = s.keyframe_merge_window_s;
            preview.overlay.grid_size = s.grid_size;
            ui_store.view.app_theme = match s.app_theme.as_str() {
                "light" => eparts::AppThemeChoice::Light,
                "dark" => eparts::AppThemeChoice::Dark,
                _ => eparts::AppThemeChoice::Auto,
            };
            ui_store.view.reduce_motion = s.reduce_motion;
            ui_store.view.density = match s.density.as_str() {
                "compact" => eparts::Density::Compact,
                _ => eparts::Density::Default,
            };
            ui_store.view.theme_dir = s.theme_dir.clone();
            ui_store.view.theme_name = s.theme_name.clone();
            ui_store.shortcut_overrides = s.shortcuts.clone();
        }

        // Autosave preferences live in `app_state.ron` (not the workspace layout
        // file), defaulting on when absent so older profiles gain recovery.
        let autosave_prefs = crate::app::persistence::load_autosave_prefs();
        ui_store.view.autosave = crate::app::stores::ui_store::AutosaveState::from_prefs(
            autosave_prefs.enabled,
            autosave_prefs.interval_s,
        );

        let shortcut_registry = match ShortcutRegistry::with_overrides(&ui_store.shortcut_overrides)
        {
            Ok(registry) => registry,
            Err(error) => {
                tracing::warn!(
                    "Failed to apply persisted shortcuts, dropping invalid overrides: {error}"
                );
                ui_store.shortcut_overrides.clear();
                ShortcutRegistry::new()
            },
        };

        let mut shell = Self {
            document_store: DocumentStore::new(document, editor),
            workspace_store: WorkspaceStore::new(
                workspace_root,
                expanded_dirs,
                file_tree,
                persistence_path,
                hot_reloader,
                hot_reload_error,
            ),
            preview_store: PreviewStore::new(preview),
            ui_store,
            export_store: ExportStore::new(),
            rebuild_worker: RebuildWorker::start(),
            plugin_manager,
            external_commands: CommandQueue::new(),
            insertion_palette: InsertionPalette::default(),
            shortcut_registry,
            window_size,
            window_maximized,
        };
        if let Some(s) = persistence.as_ref().and_then(|p| p.settings.as_ref()) {
            shell.document_store.history.undo_limit = s.undo_limit;
        }

        let plugin_issues = shell.plugin_manager.snapshot().issues;
        if !plugin_issues.is_empty() {
            let message = plugin_issues
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
            shell
                .ui_store
                .toasts
                .push(Toast::warning(format!("Plugin load issue: {message}")));
        }

        if !is_welcome {
            shell.document_store.publish_rebuild_result(
                error.is_none()
                    && !has_source_load_failure(&shell.document_store.source.document.diagnostics),
            );
            // Offer crash recovery when a newer sidecar exists. Runs before the
            // UI loop so the prompt's `is_open` is set on the first frame.
            shell.detect_recovery_prompt();
        }
        shell
    }
}
