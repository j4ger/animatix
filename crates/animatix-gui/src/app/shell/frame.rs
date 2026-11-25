use crate::app::*;

impl GuiShell {
    pub(crate) fn is_playing(&self) -> bool {
        self.preview_store.is_playing()
    }

    pub(crate) fn has_pending_rebuild(&self) -> bool {
        self.preview_store.has_pending_rebuild()
    }

    pub(crate) fn prepare_frame(&mut self) {
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.preview_store.last_frame_at);
        self.preview_store.last_frame_at = now;

        // Check for hot reload
        self.check_hot_reload(now);

        // Crash-recovery autosave: write the live editor text to the sidecar
        // when the document is dirty and the interval has elapsed.
        self.autosave_tick(now);

        // Poll plugin manifests/libraries for changes and reload atomically.
        if self.plugin_manager.poll() {
            self.apply_plugin_reload();
        }

        // Poll background export status
        self.export_store.poll_export_status();

        if self.preview_store.preview.playback.is_playing {
            self.preview_store.preview.playback.tick(delta);
            self.preview_store.preview_dirty = true;

            if self.ui_store.editor_sync_enabled
                && let Some(line) = self
                    .document_store
                    .source
                    .document
                    .find_keyframe_line_at(self.preview_store.preview.playback.current_time_s())
                && self.document_store.source.editor.highlighted_line != Some(line)
            {
                self.document_store.source.editor.scroll_to_line(line);
                self.document_store.source.editor.set_highlighted_line(Some(line));
            }
        }

        self.sync_active_scene_from_time();

        if let Some(deadline) = self.preview_store.pending_rebuild_at
            && now >= deadline
        {
            self.preview_store.pending_rebuild_at = None;
            self.preview_store.preview.error = None;
            if let Some(token) = crate::app::handlers::file::handle_rebuild_submit(
                &mut self.rebuild_worker,
                &mut self.document_store,
                &mut self.preview_store,
            ) {
                self.preview_store.in_flight_rebuild = Some(token);
            }
        }

        // Poll for completed rebuild responses. Accept only the exact token we
        // submitted: a response left over from a previous document must not be
        // applied after an open resets the in-flight state.
        for response in self.rebuild_worker.poll() {
            if self.preview_store.in_flight_rebuild == Some(response.token) {
                let elapsed_ms = response.elapsed_ms as f64;
                let effects = crate::app::handlers::file::handle_rebuild_response(
                    &mut self.document_store,
                    &mut self.preview_store,
                    &mut self.ui_store,
                    response,
                );
                self.preview_store.performance_metrics.record_rebuild(elapsed_ms);
                self.apply_effects(effects);
                self.preview_store.in_flight_rebuild = None;
            }
        }

        // Poll for completed background loads.
        for response in self.load_worker.poll() {
            if self.pending_open != Some(response.token) {
                continue;
            }
            self.pending_open = None;
            let effects = crate::app::handlers::file::handle_open_file_response(
                &mut self.document_store,
                &mut self.workspace_store,
                &mut self.preview_store,
                &mut self.ui_store,
                &mut self.plugin_manager,
                response,
            );
            self.apply_effects(effects);
            // The newly opened file may have a newer crash-recovery sidecar;
            // surface it before autosave can overwrite it.
            self.detect_recovery_prompt();
        }
    }

    /// Log raw IME/text input events with the current editor focus state so
    /// native input-method failures can be diagnosed without deep egui logs.
    pub(crate) fn log_input_debug(&self, ctx: &egui::Context) {
        if !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }
        let events: Vec<egui::Event> = ctx.input(|i| {
            i.events
                .iter()
                .filter(|event| matches!(event, egui::Event::Ime(_) | egui::Event::Text(_)))
                .cloned()
                .collect()
        });
        if !events.is_empty() {
            tracing::debug!(
                events = ?events,
                text_edit_focused = ctx.text_edit_focused(),
                "IME/text input"
            );
        }
    }

    pub(crate) fn ui(&mut self, ui: &mut egui::Ui, preview_texture_id: Option<egui::TextureId>) {
        let theme = eparts::theme(ui);
        // Track the window size so layout presets and reset can size themselves.
        let screen = ui.ctx().content_rect();
        self.ui_store.view.layout_size = (screen.width(), screen.height());
        let mut commands: ActionQueue = ActionQueue::default();
        commands.append(&mut self.ui_store.pending_actions);
        self.external_commands.drain_into(&mut commands);

        // Settings is a temporary route, not a dock layout or a modal over
        // the workbench. Keep the document, cursor and transport stores intact.
        if self.ui_store.view.settings_open && !self.ui_store.view.command_palette_open {
            self.handle_actions(commands);
            self.settings_page_ui(ui);
            if self.ui_store.unsaved_changes.is_open {
                self.unsaved_changes_dialog_ui(ui);
            }
            if self.ui_store.recovery_prompt.is_open {
                self.recovery_prompt_ui(ui);
            }
            self.ui_store.toasts.show(ui, Instant::now());
            return;
        }

        // Global keyboard shortcuts are now handled via ShortcutRegistry
        // in runtime.rs::handle_keyboard_shortcuts. Only shell-local
        // modal Escapes remain inline below.

        // The document and workspace controls belong to the open workbench.
        if !self.ui_store.view.welcome_open {
            egui::Panel::top("toolbar")
                .frame(egui::Frame::NONE)
                .resizable(false)
                .show(ui, |ui| self.toolbar_ui(ui, &mut commands));
        }

        let diagnostics = self.document_store.combined_diagnostics();

        // Diagnostics panel (collapsible)
        if self.ui_store.view.diagnostics_panel_visible {
            self.diagnostics_panel_ui(ui, &diagnostics);
        }

        // The diagnostics button in the footer anchors its peek overlay.
        let diagnostics_chip_rect = self.status_bar_ui(ui, &diagnostics, &theme);

        if self.ui_store.view.diagnostics_peek_open {
            match diagnostics_chip_rect {
                Some(chip_rect) => self.diagnostics_peek_ui(ui, chip_rect, &diagnostics),
                // The chip is not drawn while the welcome screen owns the
                // workspace; a stranded overlay would outlive its anchor.
                None => self.ui_store.view.diagnostics_peek_open = false,
            }
        }

        if self.pending_open.is_none() && !self.ui_store.view.welcome_open {
            self.narration_takes.update_voiceover_history(&self.document_store.source);
        }

        // Keep the dock’s panel background continuous across resized tiles.
        let workspace_rect = egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme.palette.surface.panel))
            .show(ui, |ui| {
                if self.ui_store.view.welcome_open {
                    let mut welcome_cmds = ActionQueue::default();
                    self.welcome_screen_ui(ui, &mut welcome_cmds);
                    for cmd in welcome_cmds {
                        let effects = self.handle_action(cmd);
                        self.apply_effects(effects);
                    }
                } else {
                    self.workspace_ui(ui, preview_texture_id, &mut commands);
                }
            })
            .response
            .rect;

        // Update cursor time from editor position (bi-directional sync)
        self.ui_store.cursor_time_s =
            self.document_store.source.editor.cursor_line.and_then(|line| {
                self.document_store.source.document.timeline_index.time_s_for_line(line)
            });

        self.handle_actions(commands);

        // Compact-mode overlay drawers (icon-rail sidebar and detail region).
        // Rendered below the modals and skipped while one is open, so a modal
        // always owns the screen (and the Escape key). Anchored to the central
        // workspace rect so they do not cover the toolbar or status bar.
        if !self.ui_store.view.welcome_open && !self.modal_open() {
            let mut drawer_cmds = ActionQueue::default();
            self.compact_sidebar_drawer_ui(ui, workspace_rect, &mut drawer_cmds);
            self.compact_detail_drawer_ui(ui, workspace_rect, &mut drawer_cmds);
            self.handle_actions(drawer_cmds);
        }

        self.narration_takes_ui(ui.ctx());

        // Workspace switcher dialog overlay
        if self.ui_store.view.workspace_switcher_open {
            self.workspace_switcher_ui(ui);
        }

        // Export dialog overlay
        if self.export_store.export_dialog_open {
            self.export_dialog_ui(ui);
        }

        // Shortcut cheat sheet overlay
        if self.ui_store.view.shortcuts_open {
            self.shortcut_cheat_sheet_ui(ui);
        }

        // Plugin status overlay
        if self.ui_store.view.plugin_status_open {
            self.plugin_status_ui(ui);
        }

        // Command palette overlay
        if self.ui_store.view.command_palette_open {
            self.command_palette_ui(ui);
        }

        // Find / Replace overlay
        if self.ui_store.view.find_replace_open {
            self.find_replace_ui(ui);
        }

        // Unsaved changes dialog
        if self.ui_store.unsaved_changes.is_open {
            self.unsaved_changes_dialog_ui(ui);
        }

        // Crash-recovery prompt (startup, when a newer sidecar exists)
        if self.ui_store.recovery_prompt.is_open {
            self.recovery_prompt_ui(ui);
        }

        // Toast notifications
        let now = Instant::now();
        self.ui_store.toasts.show(ui, now);
    }

    /// Collapsible diagnostics panel (or a success banner when clean).
    fn diagnostics_panel_ui(&mut self, ui: &mut egui::Ui, diagnostics: &[Diagnostic]) {
        egui::Panel::bottom("diagnostics_panel")
            .resizable(true)
            .default_size(180.0)
            .min_size(80.0)
            .max_size(400.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if diagnostics.is_empty() {
                    ui.add_space(SPACE_4);
                    ui.add(Alert::new(
                        format!(
                            "No diagnostics — all clear {}",
                            egui_phosphor::regular::CHECK_CIRCLE
                        ),
                        AlertLevel::Success,
                    ));
                } else {
                    let has_error = diagnostics.iter().any(|d| d.is_error());
                    if let Some(message) = diagnostics_banner_message(diagnostics) {
                        ui.add_space(SPACE_2);
                        ui.add(Alert::new(
                            message,
                            if has_error {
                                AlertLevel::Error
                            } else {
                                AlertLevel::Warning
                            },
                        ));
                        ui.add_space(SPACE_2);
                    }
                    if let Some(target) = components::diagnostics::diagnostics_list(
                        ui,
                        diagnostics,
                        &mut self.ui_store.view.diagnostics_panel_visible,
                    ) {
                        self.ui_store
                            .pending_actions
                            .push_back(Command::ScrollToLine(target.line, target.column).into());
                    }
                }
            });
    }

    /// Return the diagnostics button rect to anchor the peek overlay.
    fn status_bar_ui(
        &mut self,
        ui: &mut egui::Ui,
        diagnostics: &[Diagnostic],
        theme: &eparts::Theme,
    ) -> Option<egui::Rect> {
        let sp = crate::app::design_tokens::spatial::spatial(ui);
        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme.palette.surface.panel)
                    .inner_margin(egui::Margin::symmetric(sp.base.space_5 as i8, 0)),
            )
            .exact_size(sp.base.row_l + sp.base.space_1)
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    if !self.ui_store.view.welcome_open {
                        self.workbench_status_ui(ui);
                    }
                    let errors =
                        diagnostics.iter().filter(|diagnostic| diagnostic.is_error()).count();
                    let warnings = diagnostics.len() - errors;
                    let status = &self.preview_store.preview.status;
                    let compact = ui.available_width() < 310.0;
                    if !status.is_empty() && !compact {
                        ui.separator();
                        let is_error =
                            self.preview_store.preview.status_severity == StatusSeverity::Error;
                        let color = if is_error {
                            theme.palette.text.primary
                        } else {
                            theme.palette.text.secondary
                        };
                        let label = egui::Label::new(
                            egui::RichText::new(status.as_str())
                                .font(TextRole::BodyS.font_id())
                                .color(color),
                        )
                        .truncate();
                        let width = (ui.available_width() - 170.0).max(0.0);
                        if width > 0.0 {
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 0.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.add(label).on_hover_text(
                                        self.preview_store
                                            .preview
                                            .error
                                            .as_deref()
                                            .unwrap_or(status),
                                    )
                                },
                            );
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.status_diagnostics_ui(ui, errors, warnings, theme, compact)
                    })
                    .inner
                })
                .inner
            })
            .inner
    }

    fn status_diagnostics_ui(
        &mut self,
        ui: &mut egui::Ui,
        errors: usize,
        warnings: usize,
        theme: &eparts::Theme,
        compact: bool,
    ) -> Option<egui::Rect> {
        use egui_phosphor::regular as icons;

        let (icon, color) = if errors > 0 {
            (icons::X_CIRCLE, theme.palette.status.error)
        } else if warnings > 0 {
            (icons::WARNING, theme.palette.status.warning)
        } else {
            (icons::CHECK_CIRCLE, theme.palette.status.success)
        };
        let label = if errors + warnings == 0 {
            "No diagnostics".to_owned()
        } else {
            format!("{errors} errors · {warnings} warnings")
        };
        let button = if compact || errors + warnings == 0 {
            Button::icon(icon)
        } else {
            Button::ghost(&label).with_icon(icon)
        };
        let chip = ui.add(
            button
                .icon_color(color)
                .hover_icon_color(color)
                .with_tooltip(format!("{label} · click to peek, double-click to pin"))
                .active(
                    self.ui_store.view.diagnostics_panel_visible
                        || self.ui_store.view.diagnostics_peek_open,
                ),
        );
        if chip.double_clicked() {
            self.ui_store.view.diagnostics_peek_open = false;
            self.ui_store.view.diagnostics_panel_visible =
                !self.ui_store.view.diagnostics_panel_visible;
        } else if chip.clicked() {
            self.ui_store.view.diagnostics_peek_open = !self.ui_store.view.diagnostics_peek_open;
        }
        Some(chip.rect)
    }

    pub(crate) fn workspace_ui(
        &mut self,
        ui: &mut egui::Ui,
        preview_texture_id: Option<egui::TextureId>,
        commands: &mut ActionQueue,
    ) {
        // ── Compact (narrow-window) downgrade ──
        // Compute the decision from the live width and reconcile the dock only
        // when it flips. On entry the detail column is hidden (it renders as an
        // overlay drawer); on exit the pane visibility captured on entry is
        // restored so explicit user choices are not overridden.
        let avail = ui.available_size();
        let compact = crate::app::persistence::compact_for_width(avail.x);
        let was_compact = self.ui_store.view.compact;
        if compact != was_compact {
            let transition = crate::app::persistence::reconcile_compact(
                &mut self.ui_store.view.tree,
                was_compact,
                compact,
                &mut self.ui_store.view.compact_restore,
            );
            if transition != crate::app::persistence::CompactTransition::Unchanged {
                tracing::debug!(
                    compact,
                    width = avail.x,
                    ?transition,
                    "workspace compact-mode changed"
                );
            }
            // Overlay drawers are compact-only, and any open drawer is stale
            // after a breakpoint crossing.
            self.ui_store.set_compact(compact);
        }

        let preset = self.ui_store.view.layout_preset;
        if compact {
            // The sidebar is a fixed-width icon rail; ignore the preset's
            // sidebar pixel floor and give the rest to the preview.
            crate::app::persistence::enforce_compact_layout(
                &mut self.ui_store.view.tree,
                preset,
                avail.x,
                avail.y,
            );
        } else {
            // Keep region sizes inside the preset's pixel bounds before layout,
            // so a window smaller than the build-time reference does not scale
            // panels below their floors.
            crate::app::persistence::enforce_layout_bounds(
                &mut self.ui_store.view.tree,
                preset,
                avail.x,
                avail.y,
            );
        }

        // Refresh find-match decorations once per frame from the shared find
        // state, before the Code pane renders. The renderer overlays them on
        // the cached highlight jobs, so query/option changes take effect
        // without invalidating the highlight cache.
        self.document_store.source.editor.set_find_state(
            &self.ui_store.find_query,
            self.ui_store.find_case_sensitive,
            self.ui_store.find_whole_word,
            self.ui_store.find_regex,
            self.ui_store.find_last_match,
        );

        let tree = &mut self.ui_store.view.tree;
        let mut behavior = panels::behavior::WorkspaceBehavior {
            voiceover_history: &self.narration_takes.voiceover_history,
            document_store: &mut self.document_store,
            workspace_store: &mut self.workspace_store,
            preview_store: &mut self.preview_store,
            commands,
            preview_texture_id,
            shortcut_registry: &self.shortcut_registry,
            collapsed_actors: &mut self.ui_store.view.collapsed_actors,
            expanded_properties: &mut self.ui_store.view.expanded_properties,
            selected_actors: &mut self.ui_store.selection.selected_actors,
            selected_keyframes: &mut self.ui_store.selection.selected_keyframes,
            hit_regions: &self.ui_store.selection.hit_regions,
            selection: &mut self.ui_store.selection.selection,
            sidebar_tab: &mut self.ui_store.sidebar_tab,
            compact,
            compact_drawer: &mut self.ui_store.view.compact_drawer,
            debug_layout: self.ui_store.view.debug_layout,
            debug_spacing: self.ui_store.view.debug_spacing,
        };
        tree.ui(&mut behavior, ui);
    }
}
