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

            if self.ui_store.editor_sync_enabled {
                if let Some(line) = self
                    .document_store
                    .source
                    .document
                    .find_keyframe_line_at(self.preview_store.preview.playback.current_time_s())
                {
                    if self.document_store.source.editor.highlighted_line != Some(line) {
                        self.document_store.source.editor.scroll_to_line(line);
                        self.document_store.source.editor.set_highlighted_line(Some(line));
                    }
                }
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

        // Poll for completed rebuild responses
        for response in self.rebuild_worker.poll() {
            // Only accept the highest-token response (newest)
            if self.preview_store.in_flight_rebuild.is_none_or(|token| response.token == token) {
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
                focused_cell = self.document_store.source.editor.focused_cell(),
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

        // Global keyboard shortcuts are now handled via ShortcutRegistry
        // in runtime.rs::handle_keyboard_shortcuts. Only shell-local
        // modal Escapes remain inline below.

        // Compact toolbar — hidden during onboarding so no grid/zoom controls clutter
        // the welcome screen.
        if !self.ui_store.view.welcome_open {
            egui::Panel::top("toolbar")
                .resizable(false)
                .show_inside(ui, |ui| self.toolbar_ui(ui, &mut commands));
        }

        let diagnostics = self.document_store.combined_diagnostics();

        // Diagnostics panel (collapsible)
        if self.ui_store.view.diagnostics_panel_visible {
            egui::Panel::bottom("diagnostics_panel")
                .resizable(true)
                .default_size(180.0)
                .min_size(80.0)
                .max_size(400.0)
                .show_inside(ui, |ui| {
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
                        if let Some(message) = diagnostics_banner_message(&diagnostics) {
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
                            &diagnostics,
                            &mut self.ui_store.view.diagnostics_panel_visible,
                        ) {
                            self.ui_store.pending_actions.push_back(
                                ViewCommand::ScrollToLine(target.line, target.column).into(),
                            );
                        }
                    }
                });
        }

        // Status bar — thin bar at the bottom showing preview status and scene dimensions
        // Filled by the diagnostics chip below; the peek overlay anchors to it.
        let mut diagnostics_chip_rect: Option<egui::Rect> = None;

        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme.palette.surface.panel)
                    .inner_margin(egui::Margin::symmetric(8, 2)),
            )
            .resizable(false)
            .min_size(20.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let status = &self.preview_store.preview.status;
                    if !status.is_empty() {
                        let is_error =
                            self.preview_store.preview.status_severity == StatusSeverity::Error;
                        if is_error {
                            // Red accent pill + warning icon for errors
                            let (bg_rect, _) = ui
                                .allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            ui.painter().rect_filled(
                                bg_rect,
                                RADIUS_S,
                                theme.palette.status.diagnostic_error.linear_multiply(0.3),
                            );
                            ui.painter().text(
                                egui::pos2(bg_rect.center().x, bg_rect.center().y),
                                egui::Align2::CENTER_CENTER,
                                egui_phosphor::regular::WARNING,
                                TextRole::Micro.font_id(),
                                theme.palette.status.diagnostic_error,
                            );
                            ui.add_space(SPACE_2);
                        }
                        let color = if is_error {
                            theme.palette.status.diagnostic_error
                        } else {
                            theme.palette.text.muted
                        };
                        let label = ui.label(
                            egui::RichText::new(status.as_str())
                                .size(TextRole::Micro.size())
                                .color(color),
                        );
                        if is_error && self.preview_store.preview.error.is_some() {
                            text_tooltip(
                                ui,
                                ui.id().with("status_error_tooltip"),
                                &label,
                                self.preview_store.preview.error.as_deref().unwrap_or(""),
                            );
                        }
                    }
                    // Right side: diagnostics count + scene dimensions
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let dims = &self.document_store.source.document.scene_dimensions;
                        ui.label(
                            egui::RichText::new(format!("{}×{}", dims.width, dims.height))
                                .size(TextRole::Micro.size())
                                .color(theme.palette.text.muted),
                        );

                        // Diagnostics chip: always-visible error/warning counts that
                        // toggle the panel, so an open issue is never invisible.
                        let errors = diagnostics.iter().filter(|d| d.is_error()).count();
                        let warnings = diagnostics.len() - errors;
                        let (chip_icon, chip_color, chip_label) = if errors > 0 {
                            (
                                egui_phosphor::regular::X_CIRCLE,
                                theme.palette.status.error,
                                format!("{errors} error(s)"),
                            )
                        } else if warnings > 0 {
                            (
                                egui_phosphor::regular::WARNING,
                                theme.palette.status.warning,
                                format!("{warnings} warning(s)"),
                            )
                        } else {
                            (
                                egui_phosphor::regular::CHECK_CIRCLE,
                                theme.palette.status.success,
                                "No problems".to_string(),
                            )
                        };
                        let chip = ui.add(
                            Button::ghost("")
                                .with_icon(chip_icon)
                                .icon_color(chip_color)
                                .hover_icon_color(chip_color)
                                .active(
                                    self.ui_store.view.diagnostics_panel_visible
                                        || self.ui_store.view.diagnostics_peek_open,
                                ),
                        );
                        text_tooltip(
                            ui,
                            chip.id.with("diag_chip_tip"),
                            &chip,
                            &format!(
                                "{chip_label} — click to peek, double-click to pin the \
                                 diagnostics panel"
                            ),
                        );
                        diagnostics_chip_rect = Some(chip.rect);
                        if chip.double_clicked() {
                            self.ui_store.view.diagnostics_peek_open = false;
                            self.ui_store.view.diagnostics_panel_visible =
                                !self.ui_store.view.diagnostics_panel_visible;
                        } else if chip.clicked() {
                            self.ui_store.view.diagnostics_peek_open =
                                !self.ui_store.view.diagnostics_peek_open;
                        }
                    });
                });
            });

        if self.ui_store.view.diagnostics_peek_open {
            match diagnostics_chip_rect {
                Some(chip_rect) => self.diagnostics_peek_ui(ui, chip_rect, &diagnostics),
                // The chip is not drawn while the welcome screen owns the
                // workspace; a stranded overlay would outlive its anchor.
                None => self.ui_store.view.diagnostics_peek_open = false,
            }
        }

        // Central workspace — edge-to-edge tiles, no outer margin
        // When welcome screen is open, show it instead of the workspace.
        let workspace_rect = egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin::ZERO))
            .show_inside(ui, |ui| {
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

        // Safety net: the preview panel clears the Library drag payload on
        // release, but it does not render on the welcome screen (or if the
        // preview pane is ever hidden). Drop any leftover payload once the
        // pointer is up so a stale drag cannot drop on a later frame.
        if ui.input(|i| i.pointer.any_released()) {
            panels::clear_library_drag(ui.ctx());
        }

        // Settings modal overlay (rendered on top of everything)
        if self.ui_store.view.settings_open {
            self.settings_dialog_ui(ui);
        }

        // Workspace switcher dialog overlay
        if self.ui_store.view.workspace_switcher_open {
            self.workspace_switcher_ui(ui);
        }

        // Export dialog overlay
        if self.export_store.export_dialog_open {
            self.export_dialog_ui(ui);
        }

        // Insertion palette overlay
        self.insertion_palette_ui(ui);

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
            document_store: &mut self.document_store,
            workspace_store: &mut self.workspace_store,
            preview_store: &mut self.preview_store,
            commands,
            preview_texture_id,
            collapsed_actors: &mut self.ui_store.view.collapsed_actors,
            expanded_properties: &mut self.ui_store.view.expanded_properties,
            selected_actors: &mut self.ui_store.selection.selected_actors,
            hit_regions: &self.ui_store.selection.hit_regions,
            drag_state: &mut self.ui_store.interaction.drag_state,
            selection: &mut self.ui_store.selection.selection,
            pivot_offsets: &mut self.ui_store.pivot_offsets,
            tool_mode: &mut self.ui_store.view.tool_mode,
            sidebar_tab: &mut self.ui_store.sidebar_tab,
            compact,
            compact_drawer: &mut self.ui_store.view.compact_drawer,
            property_view_mode: &mut self.ui_store.property_view_mode,
            keyframe_view_mode: &mut self.ui_store.keyframe_view_mode,
            keyframe_mode: self.ui_store.keyframe_mode,
            rotation_snap_degrees: self.ui_store.rotation_snap_degrees,
            snap_fps: self.ui_store.snap_fps,
            debug_layout: self.ui_store.view.debug_layout,
            debug_spacing: self.ui_store.view.debug_spacing,
            timeline_focused: &mut self.ui_store.view.timeline_focused,
            selected_keyframes: &mut self.ui_store.selection.selected_keyframes,
        };
        tree.ui(&mut behavior, ui);
    }
}
