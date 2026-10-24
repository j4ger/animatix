use super::helpers::*;
use super::*;

impl AnimatixApp {
    pub(super) fn new(
        cc: &eframe::CreationContext<'_>,
        initial_path: PathBuf,
        show_welcome: bool,
        perf_log: Option<crate::app::perf_log::PerfLogSink>,
    ) -> Result<Self, String> {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .ok_or_else(|| "eframe wgpu render state not available".to_string())?;

        let device = &render_state.device;
        let queue = &render_state.queue;

        // Set up theme: detect OS appearance, resolve the persisted choice, apply.
        // (Resolved after `shell` is loaded below, since the choice is persisted there.)
        let system_is_dark = detect_system_dark();

        // Register egui defaults, Phosphor icons, and a system CJK fallback.
        crate::fonts::install_fonts(&cc.egui_ctx);

        let preview_surface = PreviewSurface::new(device, queue)
            .map_err(|e| format!("Preview surface init failed: {e}"))?;
        let shell = GuiShell::load(initial_path, show_welcome);

        let choice = shell.ui_store.view.app_theme;
        let theme = choice.resolve(system_is_dark);
        eparts::set_theme(&cc.egui_ctx, theme);
        install_theme(&cc.egui_ctx, &theme, choice.is_dark(system_is_dark));

        let reduce_motion = shell.ui_store.view.reduce_motion;
        let initial_density = shell.ui_store.view.density;

        #[cfg(feature = "theme-json")]
        let (theme_registry, theme_registry_watcher, theme_error) =
            if let Some(dir) = shell.ui_store.view.theme_dir.clone() {
                let registry = ThemeRegistry::from_directory(&dir);
                let watcher = ThemeRegistryWatcher::new(&dir);
                (
                    registry.as_ref().ok().cloned(),
                    watcher.ok(),
                    registry.err().map(|e| e.to_string()),
                )
            } else {
                (None, None, None)
            };

        let audio_engine = match AudioEngine::new() {
            Ok(engine) => Some(engine),
            Err(e) => {
                tracing::warn!("Audio playback not available: {e}");
                None
            },
        };

        // Dev-only: start with a named layout preset so screenshots can cover
        // each preset without driving the UI.
        #[cfg(feature = "dev-screenshots")]
        let shell = {
            let mut shell = shell;
            let preset = match std::env::var("ANIMATIX_LAYOUT_PRESET")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "code" => Some(crate::app::LayoutPreset::Code),
                "inspect" => Some(crate::app::LayoutPreset::Inspect),
                "focus" => Some(crate::app::LayoutPreset::Focus),
                "animate" => Some(crate::app::LayoutPreset::Animate),
                _ => None,
            };
            if let Some(preset) = preset {
                let _ = crate::app::handlers::ui::handle_apply_layout(&mut shell.ui_store, preset);
            }
            shell
        };

        Ok(Self {
            shell,
            preview_surface,
            preview_texture_id: None,
            audio_engine,
            system_is_dark,
            last_theme_probe: Some(std::time::Instant::now()),
            applied_theme_signature: Some((choice, system_is_dark)),
            applied_motion_preference: Some(reduce_motion),
            applied_density: Some(initial_density),
            #[cfg(feature = "theme-json")]
            theme_registry,
            #[cfg(feature = "theme-json")]
            theme_registry_watcher,
            #[cfg(feature = "theme-json")]
            theme_error,
            perf_log,
            #[cfg(feature = "dev-screenshots")]
            screenshot: ScreenshotSession::from_env(),
        })
    }

    #[cfg(feature = "theme-json")]
    pub(super) fn update_json_theme(&mut self, ctx: &egui::Context) {
        let desired = self.shell.ui_store.view.theme_dir.clone();
        let current_dir =
            self.theme_registry_watcher.as_ref().map(|watcher| watcher.directory().clone());
        if current_dir != desired {
            self.theme_registry = None;
            self.theme_registry_watcher = None;
            self.theme_error = None;
            self.shell.ui_store.view.theme_names.clear();
            self.shell.ui_store.view.theme_error = None;
            if let Some(dir) = desired {
                match ThemeRegistry::from_directory(&dir) {
                    Ok(registry) => {
                        self.theme_registry = Some(registry);
                        self.shell.ui_store.view.theme_names = self
                            .theme_registry
                            .as_ref()
                            .map(ThemeRegistry::names)
                            .unwrap_or_default();
                    },
                    Err(error) => {
                        self.theme_error = Some(error.to_string());
                        self.shell.ui_store.view.theme_error = self.theme_error.clone();
                    },
                }
                match ThemeRegistryWatcher::new(&dir) {
                    Ok(watcher) => self.theme_registry_watcher = Some(watcher),
                    Err(error) => {
                        self.theme_error = Some(error);
                        self.shell.ui_store.view.theme_error = self.theme_error.clone();
                    },
                }
            }
        }

        if let Some(watcher) = &mut self.theme_registry_watcher {
            match watcher.update(std::time::Instant::now()) {
                ThemeRegistryWatcherEvent::Reloaded(registry) => {
                    let names = registry.names();
                    self.theme_registry = Some(registry);
                    self.theme_error = None;
                    self.shell.ui_store.view.theme_names = names;
                    self.shell.ui_store.view.theme_error = None;
                },
                ThemeRegistryWatcherEvent::Error(error) => {
                    self.theme_error = Some(error.clone());
                    self.shell.ui_store.view.theme_error = Some(error);
                },
                ThemeRegistryWatcherEvent::NoChange => {},
            }
        }

        if let (Some(registry), Some(name)) =
            (&self.theme_registry, &self.shell.ui_store.view.theme_name)
        {
            let dark = self.shell.ui_store.view.app_theme.is_dark(self.system_is_dark);
            if registry.install(ctx, name, dark) {
                let theme = eparts::theme_from_ctx(ctx);
                install_theme(ctx, &theme, dark);
                self.applied_theme_signature =
                    Some((self.shell.ui_store.view.app_theme, self.system_is_dark));
            }
        }
    }

    pub(super) fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        use crate::app::interaction::keyboard::{FocusContext, KeyboardAction};
        use crate::app::preview::{DragState, ToolMode};
        use crate::app::shell::insertion_palette::PaletteMode;

        let has_selection = !self.shell.ui_store.selection.selected_actors.is_empty();
        let focus = FocusContext {
            text_edit_focused: ctx.text_edit_focused(),
            has_selection,
            drag_active: self.shell.ui_store.interaction.is_dragging(),
            inline_edit_active: self.shell.preview_store.preview.inline_edit.is_some(),
            command_palette_open: self.shell.ui_store.view.command_palette_open,
            find_replace_open: self.shell.ui_store.view.find_replace_open,
            unsaved_dialog_open: self.shell.ui_store.unsaved_changes.is_open,
            tool_mode: self.shell.ui_store.view.tool_mode,
        };

        if let Some(action) = self.shell.shortcut_registry.check(ctx, &focus) {
            match action {
                KeyboardAction::Undo => {
                    self.shell.ui_store.pending_actions.push_back(DocumentCommand::Undo.into());
                },
                KeyboardAction::Redo => {
                    self.shell.ui_store.pending_actions.push_back(DocumentCommand::Redo.into());
                },
                KeyboardAction::Save => {
                    self.shell.ui_store.pending_actions.push_back(DocumentCommand::Save.into());
                },
                KeyboardAction::Reload => {
                    self.shell.ui_store.pending_actions.push_back(DocumentCommand::Reload.into());
                },
                KeyboardAction::Rebuild => {
                    self.shell.ui_store.pending_actions.push_back(DocumentCommand::Rebuild.into());
                },
                KeyboardAction::TogglePlayback => {
                    if self.shell.preview_store.preview.playback.presentation_mode {
                        self.shell
                            .ui_store
                            .pending_actions
                            .push_back(PlaybackCommand::StepNext.into());
                    } else {
                        self.shell
                            .ui_store
                            .pending_actions
                            .push_back(PlaybackCommand::TogglePlayback.into());
                    }
                },
                KeyboardAction::StepNext => {
                    self.shell.ui_store.pending_actions.push_back(PlaybackCommand::StepNext.into());
                },
                KeyboardAction::StepPrevious => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(PlaybackCommand::StepPrevious.into());
                },
                KeyboardAction::TogglePresentationMode => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(PlaybackCommand::TogglePresentationMode.into());
                },
                KeyboardAction::PrevKeyframe => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(PlaybackCommand::PrevKeyframe.into());
                },
                KeyboardAction::NextKeyframe => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(PlaybackCommand::NextKeyframe.into());
                },
                KeyboardAction::FrameStepForward => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(PlaybackCommand::FrameStepForward.into());
                },
                KeyboardAction::FrameStepBackward => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(PlaybackCommand::FrameStepBackward.into());
                },
                KeyboardAction::DuplicateSelection => {
                    for label in self.shell.ui_store.selection.selected_actors.clone().into_iter() {
                        self.shell
                            .ui_store
                            .pending_actions
                            .push_back(ActorCommand::DuplicateActor(label).into());
                    }
                },
                KeyboardAction::DeleteSelection => {
                    // When the timeline panel is focused, delete selected keyframes
                    // instead of selected actors. The Canvas scope already guards
                    // against firing while text fields/inspector inputs are active.
                    if self.shell.ui_store.view.timeline_focused
                        && !self.shell.ui_store.selection.selected_keyframes.is_empty()
                    {
                        for keyframe in self.shell.ui_store.selection.selected_keyframes.clone() {
                            let crate::app::document::timeline_diff::KeyframeId {
                                scene,
                                actor,
                                property,
                                time_ms,
                            } = keyframe;
                            self.shell.ui_store.pending_actions.push_back(
                                KeyframeCommand::DeleteKeyframe {
                                    scene,
                                    actor,
                                    property,
                                    time_s: time_ms as f64 / 1000.0,
                                }
                                .into(),
                            );
                        }
                        self.shell.ui_store.selection.selected_keyframes.clear();
                    } else {
                        self.shell
                            .ui_store
                            .pending_actions
                            .push_back(ActorCommand::DeleteSelectedActors.into());
                    }
                },
                KeyboardAction::GroupSelection => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(ActorCommand::GroupSelectedActors.into());
                },
                KeyboardAction::UngroupSelection => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(ActorCommand::UngroupSelectedActors.into());
                },
                KeyboardAction::ZoomToSelection => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(ViewCommand::ZoomToSelection.into());
                },
                KeyboardAction::ZoomToAll => {
                    self.shell.ui_store.pending_actions.push_back(ViewCommand::ZoomToAll.into());
                },
                KeyboardAction::OpenCommandPalette => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(ShellAction::View(ViewAction::OpenCommandPalette));
                },
                KeyboardAction::OpenFindReplace => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(ShellAction::View(ViewAction::OpenFindReplace));
                },
                KeyboardAction::SetSelectTool => {
                    self.shell.ui_store.view.tool_mode = ToolMode::Select;
                    self.shell.preview_store.preview.status = "Tool: Select".to_string();
                },
                KeyboardAction::SetMoveTool => {
                    self.shell.ui_store.view.tool_mode = ToolMode::Move;
                    self.shell.preview_store.preview.status = "Tool: Move".to_string();
                },
                KeyboardAction::SetScaleTool => {
                    self.shell.ui_store.view.tool_mode = ToolMode::Scale;
                    self.shell.preview_store.preview.status = "Tool: Scale".to_string();
                },
                KeyboardAction::SetRotateTool => {
                    self.shell.ui_store.view.tool_mode = ToolMode::Rotate;
                    self.shell.preview_store.preview.status = "Tool: Rotate".to_string();
                },
                KeyboardAction::SetVertexTool => {
                    self.shell.ui_store.view.tool_mode = ToolMode::Vertex;
                    self.shell.preview_store.preview.status = "Tool: Vertex".to_string();
                },
                KeyboardAction::SetPivotTool => {
                    self.shell.ui_store.view.tool_mode = ToolMode::Pivot;
                    self.shell.preview_store.preview.status = "Tool: Pivot".to_string();
                },
                KeyboardAction::Escape => {
                    if self.shell.preview_store.preview.playback.presentation_mode {
                        self.shell
                            .ui_store
                            .pending_actions
                            .push_back(PlaybackCommand::TogglePresentationMode.into());
                    } else if !matches!(self.shell.ui_store.interaction.drag_state, DragState::None)
                    {
                        self.shell.ui_store.interaction.drag_state = DragState::None;
                        self.shell.preview_store.preview.status = "Drag cancelled".to_string();
                    } else if self.shell.ui_store.view.tool_mode != ToolMode::Select {
                        self.shell.ui_store.view.tool_mode = ToolMode::Select;
                        self.shell.preview_store.preview.status = "Tool: Select".to_string();
                    }
                },
                KeyboardAction::EditSync => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(PlaybackCommand::ToggleEditorSync.into());
                },
                KeyboardAction::OpenInsertionPalette => {
                    self.shell.insertion_palette.open(PaletteMode::Universal);
                },
                KeyboardAction::CopySelection => {
                    if has_selection {
                        self.shell.copy_selected_actors();
                    }
                },
                KeyboardAction::PasteClipboard => {
                    if !self.shell.ui_store.clipboard.clipboard_actors.is_empty() {
                        self.shell
                            .ui_store
                            .pending_actions
                            .push_back(ActorCommand::PasteActors.into());
                    }
                },
                KeyboardAction::SelectScene(index) => {
                    let scene_names = self.shell.document_store.source.document.scene_names();
                    if (*index as usize) < scene_names.len() {
                        self.shell.ui_store.pending_actions.push_back(
                            SceneCommand::SelectScene(scene_names[*index as usize].clone()).into(),
                        );
                    }
                },
                KeyboardAction::NudgeSelected { dx, dy } => {
                    let scrub_step_s = self.shell.ui_store.scrub_step_s;
                    if self.shell.preview_store.preview.playback.presentation_mode {
                        if *dx > 0.0 {
                            self.shell
                                .ui_store
                                .pending_actions
                                .push_back(PlaybackCommand::StepNext.into());
                        } else if *dx < 0.0 {
                            self.shell
                                .ui_store
                                .pending_actions
                                .push_back(PlaybackCommand::StepPrevious.into());
                        }
                    } else if has_selection {
                        let shift_pressed = ctx.input(|i| i.modifiers.shift);
                        let nudge_step = if shift_pressed {
                            self.shell.ui_store.nudge_step_shift_px
                        } else if self.shell.preview_store.preview.overlay.show_grid {
                            self.shell.preview_store.preview.overlay.grid_size
                        } else {
                            self.shell.ui_store.nudge_step_px
                        };
                        let dx = *dx * nudge_step;
                        let dy = *dy * nudge_step;

                        let time_ms = (self.shell.preview_store.preview.playback.current_time_s()
                            * 1000.0) as u64;
                        let keyframe_mode = self.shell.ui_store.keyframe_mode;
                        let selected: Vec<String> =
                            self.shell.ui_store.selection.selected_actors.iter().cloned().collect();
                        let nudge_data: Vec<(String, [f32; 2])> = self
                            .shell
                            .document_store
                            .source
                            .document
                            .active_timeline()
                            .map(|t| {
                                selected
                                    .iter()
                                    .filter_map(|actor| {
                                        t.get_entity_id(actor).and_then(|id| {
                                            t.world().geometry(id).position.as_ref().map(|p| {
                                                let pos = p.evaluate(time_ms);
                                                (actor.clone(), [pos[0] + dx, pos[1] + dy])
                                            })
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        for (actor, new_pos) in &nudge_data {
                            self.shell.handle_property_edit(crate::app::panels::PropertyEdit {
                                time_s: None,
                                actor: actor.clone(),
                                property: "position".into(),
                                value: crate::app::panels::PropertyValue::Vec2(*new_pos),
                                create_keyframe: keyframe_mode,
                            });
                        }
                    } else {
                        // No selection: left/right arrows become frame stepping
                        if *dx < 0.0 {
                            let new_time =
                                self.shell.preview_store.preview.playback.current_time_s()
                                    - scrub_step_s;
                            self.shell
                                .ui_store
                                .pending_actions
                                .push_back(PlaybackCommand::ScrubTo(new_time).into());
                        } else if *dx > 0.0 {
                            let new_time =
                                self.shell.preview_store.preview.playback.current_time_s()
                                    + scrub_step_s;
                            self.shell
                                .ui_store
                                .pending_actions
                                .push_back(PlaybackCommand::ScrubTo(new_time).into());
                        }
                        // Up/down arrows without selection are intentionally ignored
                    }
                },
                KeyboardAction::ToggleInspector => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(ShellAction::View(ViewAction::ShowInspector));
                },
                KeyboardAction::ToggleCode => {
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(ShellAction::View(ViewAction::ShowCode));
                },
            }
        }
    }

    pub(super) fn sync_preview_surface(&mut self, frame: &mut eframe::Frame) -> Result<(), String> {
        // Capture last-good fallback before any mutable borrows
        let fallback = if self.shell.document_store.source.document.timeline.is_none()
            && self.shell.document_store.source.document.composition.is_none()
        {
            self.shell.document_store.last_good_snapshot()
        } else {
            None
        };

        let dimensions = fallback
            .as_ref()
            .map(|s| s.scene_dimensions)
            .unwrap_or(self.shell.document_store.source.document.scene_dimensions);
        let render_state = frame
            .wgpu_render_state()
            .ok_or_else(|| "wgpu render state not available".to_string())?;
        let device = &render_state.device;
        let queue = &render_state.queue;

        self.preview_surface.set_dimensions(device, dimensions);

        if self.shell.preview_store.preview_dirty {
            let render_start = std::time::Instant::now();
            let debug = animatix::timeline::DebugRenderOptions {
                draw_bounds: self.shell.ui_store.view.debug_bounds,
                compute_hit_regions: true,
                draw_layout_debug: self.shell.ui_store.view.debug_layout,
                draw_spacing: self.shell.ui_store.view.debug_spacing,
            };

            let render_result = if let Some(ref snapshot) = fallback {
                match &snapshot.target {
                    crate::app::document::snapshot::BuildTargetSnapshot::Composition(c) => {
                        self.preview_surface.render_composition(
                            device,
                            queue,
                            c,
                            self.shell.preview_store.preview.playback.current_time_s(),
                            debug,
                        )
                    },
                    crate::app::document::snapshot::BuildTargetSnapshot::Timeline(t) => {
                        self.preview_surface.render(
                            device,
                            queue,
                            t,
                            self.shell.preview_store.preview.playback.current_time_s(),
                            debug,
                        )
                    },
                    _ => Ok(()),
                }
            } else if let Some(composition) =
                self.shell.document_store.source.document.composition.as_ref()
            {
                self.preview_surface.render_composition(
                    device,
                    queue,
                    composition,
                    self.shell.preview_store.preview.playback.current_time_s(),
                    debug,
                )
            } else if let Some(timeline) =
                self.shell.document_store.source.document.active_timeline()
            {
                self.preview_surface.render(
                    device,
                    queue,
                    timeline,
                    self.shell.preview_store.preview.playback.current_time_s(),
                    debug,
                )
            } else {
                Ok(())
            };

            if let Err(error) = render_result {
                self.shell.set_render_error(error);
                return Ok(());
            }

            // Register or update the texture with egui
            if let Some(sample_view) = self.preview_surface.sample_view() {
                let mut renderer = render_state.renderer.write();
                let texture_id = match self.preview_texture_id {
                    Some(id) => {
                        renderer.update_egui_texture_from_wgpu_texture(
                            device,
                            sample_view,
                            wgpu::FilterMode::Linear,
                            id,
                        );
                        id
                    },
                    None => renderer.register_native_texture(
                        device,
                        sample_view,
                        wgpu::FilterMode::Linear,
                    ),
                };
                self.preview_texture_id = Some(texture_id);
            }

            self.shell.preview_store.preview_dirty = false;

            // Collect runtime diagnostics from the evaluated timeline(s)
            // Skip when using fallback — fallback snapshot is immutable, not re-evaluated.
            let mut runtime_diagnostics = Vec::new();
            if fallback.is_none() {
                if let Some(composition) =
                    self.shell.document_store.source.document.composition.as_ref()
                {
                    for scene in composition.scenes.values() {
                        runtime_diagnostics.extend(scene.timeline.runtime_diagnostics());
                    }
                } else if let Some(timeline) =
                    self.shell.document_store.source.document.active_timeline()
                {
                    runtime_diagnostics.extend(timeline.runtime_diagnostics());
                }
            }
            self.shell.document_store.history.runtime_diagnostics = runtime_diagnostics;

            // Transfer hit regions from the preview surface (moved, not cloned)
            // into the document store cache.  A small clone is still needed for
            // ui_store because Behavior borrows document_store mutably and cannot
            // alias &cached_hit_regions simultaneously.
            let fresh_hit_regions = self.preview_surface.take_hit_regions();
            self.shell.document_store.source.cached_actor_bounds = fresh_hit_regions
                .iter()
                .map(|(label, bounds)| (label.clone(), *bounds))
                .collect();
            self.shell.document_store.source.cached_hit_regions = fresh_hit_regions;
            self.shell.ui_store.selection.hit_regions =
                self.shell.document_store.source.cached_hit_regions.clone();
            self.shell.clear_any_error(live_preview_status(
                &self.shell.preview_store.preview,
                self.shell.document_store.source.document.active_scene.as_deref(),
            ));

            // Record render time
            let render_elapsed = render_start.elapsed().as_secs_f64() * 1000.0;
            self.shell.preview_store.performance_metrics.record_render(render_elapsed);
        } else if self.preview_texture_id.is_none()
            && self.preview_surface.dimensions().width > 0
            && self.preview_surface.dimensions().height > 0
        {
            // Register texture if not yet registered
            if let Some(sample_view) = self.preview_surface.sample_view() {
                let mut renderer = render_state.renderer.write();
                let texture_id =
                    renderer.register_native_texture(device, sample_view, wgpu::FilterMode::Linear);
                self.preview_texture_id = Some(texture_id);
            }
            self.shell.clear_any_error(live_preview_status(
                &self.shell.preview_store.preview,
                self.shell.document_store.source.document.active_scene.as_deref(),
            ));
        }

        Ok(())
    }
}
