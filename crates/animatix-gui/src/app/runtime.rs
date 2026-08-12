use eframe::egui;

use super::*;
use crate::app::audio::AudioEngine;
use crate::app::commands::{
    ActorCommand, DocumentCommand, KeyframeCommand, PlaybackCommand, SceneCommand, ShellAction,
    ViewAction, ViewCommand,
};
use crate::app::design_tokens::spatial::spatial_from_ctx;
use crate::app::persistence::{
    clear_app_state, load_app_state, load_workspace_persistence, persistence_path, save_app_state,
};

pub fn run_gui(path: Option<PathBuf>) {
    let (initial_path, show_welcome) = match path {
        Some(p) => (p, false),
        None => match load_app_state() {
            Some(p) => (p, false),
            None => (default_file_path(), true),
        },
    };

    // Load persisted window geometry before constructing NativeOptions
    let persistence_path = persistence_path();
    let persistence = load_workspace_persistence(&persistence_path);
    let window_size = persistence.as_ref().and_then(|p| p.window_size);
    let window_maximized = persistence.as_ref().and_then(|p| p.window_maximized).unwrap_or(false);

    let viewport = egui::ViewportBuilder::default()
        .with_title("Animatix")
        .with_inner_size(
            window_size
                .map(|[w, h]| egui::Vec2::new(w, h))
                .unwrap_or(egui::vec2(INITIAL_WINDOW_SIZE.0 as f32, INITIAL_WINDOW_SIZE.1 as f32)),
        )
        .with_maximized(window_maximized);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "Animatix",
        options,
        Box::new(move |cc| {
            let app = AnimatixApp::new(cc, initial_path, show_welcome)?;
            Ok(Box::new(app))
        }),
    ) {
        tracing::error!("Failed to run Animatix GUI: {e}");
        std::process::exit(1);
    }
}

struct AnimatixApp {
    shell: GuiShell,
    preview_surface: PreviewSurface,
    preview_texture_id: Option<egui::TextureId>,
    /// Audio playback engine for preview audio synced with the timeline.
    audio_engine: Option<AudioEngine>,
    /// Cached OS dark/light detection for the `Auto` theme (None = unknown).
    system_is_dark: Option<bool>,
    /// Last time the OS theme was probed (periodic re-detect for Auto mode).
    last_theme_probe: Option<std::time::Instant>,
    /// The theme choice applied on the previous frame, to avoid redundant work.
    applied_theme_signature: Option<(eparts::AppThemeChoice, Option<bool>)>,
    /// The motion preference applied on the previous frame, to avoid redundant work.
    applied_motion_preference: Option<bool>,
    /// The density applied on the previous frame, to avoid redundant work.
    applied_density: Option<eparts::Density>,
}

/// Probe the OS light/dark appearance via the `dark-light` crate.
///
/// Cross-platform: XDG Desktop Portal on Linux, native APIs on Windows/macOS.
/// Returns `None` when the OS does not report a preference.
fn detect_system_dark() -> Option<bool> {
    match dark_light::detect() {
        Ok(dark_light::Mode::Dark) => Some(true),
        Ok(dark_light::Mode::Light) => Some(false),
        _ => None,
    }
}

impl AnimatixApp {
    fn new(
        cc: &eframe::CreationContext<'_>,
        initial_path: PathBuf,
        show_welcome: bool,
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

        // Register Phosphor icon font
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        let preview_surface = PreviewSurface::new(device, queue)
            .map_err(|e| format!("Preview surface init failed: {e}"))?;
        let shell = GuiShell::load(initial_path, show_welcome);

        let choice = shell.ui_store.view.app_theme;
        let theme = choice.resolve(system_is_dark);
        eparts::set_theme(&cc.egui_ctx, theme);
        install_theme(&cc.egui_ctx, &theme, choice.is_dark(system_is_dark));

        let reduce_motion = shell.ui_store.view.reduce_motion;
        let initial_density = shell.ui_store.view.density;

        let audio_engine = match AudioEngine::new() {
            Ok(engine) => Some(engine),
            Err(e) => {
                tracing::warn!("Audio playback not available: {e}");
                None
            },
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
        })
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        use crate::app::interaction::keyboard::{FocusContext, KeyboardAction};
        use crate::app::preview::{DragState, ToolMode};
        use crate::app::shell::insertion_palette::PaletteMode;

        let has_selection = !self.shell.ui_store.selection.selected_actors.is_empty();
        let focus = FocusContext {
            wants_keyboard: ctx.egui_wants_keyboard_input(),
            has_selection,
            drag_active: self.shell.ui_store.interaction.is_dragging(),
            inline_edit_active: false,
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
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(PlaybackCommand::TogglePlayback.into());
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
                                actor,
                                property,
                                time_ms,
                                ..
                            } = keyframe;
                            self.shell.ui_store.pending_actions.push_back(
                                KeyframeCommand::DeleteKeyframe {
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
                    if !matches!(self.shell.ui_store.interaction.drag_state, DragState::None) {
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
                    if has_selection {
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
                                        t.get_track(actor).and_then(|track| {
                                            track.geometry.position.as_ref().map(|p| {
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
            }
        }
    }

    fn sync_preview_surface(&mut self, frame: &mut eframe::Frame) -> Result<(), String> {
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

impl eframe::App for AnimatixApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        let [r, g, b, a] = visuals.panel_fill.to_normalized_gamma_f32();
        [r, g, b, a]
    }

    fn on_exit(&mut self) {
        self.shell.save_persistence();
        if self.shell.ui_store.view.welcome_open {
            clear_app_state();
        } else {
            save_app_state(&self.shell.document_store.source.document.file_path);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Intercept close when document has unsaved changes
        if self.shell.document_store.source.is_dirty()
            && !self.shell.ui_store.unsaved_changes.is_open
        {
            let close_requested = ui.input(|i| i.viewport().close_requested());
            if close_requested {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.shell.ui_store.unsaved_changes.open_for_close();
            }
        }

        // Record frame tick for performance HUD
        self.shell.preview_store.performance_metrics.record_tick();

        // Update stale flag and GPU memory estimate
        self.shell
            .preview_store
            .performance_metrics
            .set_stale(self.shell.document_store.snapshot_is_stale());

        // Prepare frame (hot reload, playback tick, pending rebuild)
        self.shell.prepare_frame();

        if let Some(err) = self.shell.workspace_store.hot_reload_error.take() {
            self.shell.ui_store.toasts.push(crate::app::components::toast::Toast::warning(
                format!("Hot reload unavailable: {err}"),
            ));
        }

        // Re-resolve the app theme. Periodically re-probe the OS appearance (cheap
        // amortized: every ~2s) so `Auto` follows runtime OS light/dark changes;
        // reapply only when the effective theme actually changes.
        {
            let now = std::time::Instant::now();
            let due = self
                .last_theme_probe
                .map(|t| now.duration_since(t).as_secs_f32() >= 2.0)
                .unwrap_or(true);
            if due {
                self.system_is_dark = detect_system_dark();
                self.last_theme_probe = Some(now);
            }
            let choice = self.shell.ui_store.view.app_theme;
            let signature = (choice, self.system_is_dark);
            if self.applied_theme_signature != Some(signature) {
                let dark = choice.is_dark(self.system_is_dark);
                let theme = choice.resolve(self.system_is_dark);
                eparts::set_theme(ui.ctx(), theme);
                install_theme(ui.ctx(), &theme, dark);
                self.applied_theme_signature = Some(signature);
            }

            // Sync motion preference (reduced-motion toggle)
            let reduce = self.shell.ui_store.view.reduce_motion;
            if self.applied_motion_preference != Some(reduce) {
                let pref = if reduce {
                    eparts::MotionPreference::Reduced
                } else {
                    eparts::MotionPreference::Full
                };
                eparts::set_motion_preference(ui.ctx(), pref);
                self.applied_motion_preference = Some(reduce);
            }

            // Sync density preference
            let d = self.shell.ui_store.view.density;
            if self.applied_density != Some(d) {
                eparts::set_density(ui.ctx(), d);
                self.applied_density = Some(d);
            }
        }

        // Handle keyboard shortcuts
        self.handle_keyboard_shortcuts(ui.ctx());

        // Sync audio with playback state
        if let Some(audio_engine) = &mut self.audio_engine {
            let playing = self.shell.preview_store.preview.playback.is_playing;
            let segments = if playing {
                self.shell.document_store.source.document.all_audio_segments()
            } else {
                Vec::new()
            };
            let time_s = self.shell.preview_store.preview.playback.current_time_s();
            let playback_speed = self.shell.preview_store.preview.playback.playback_speed;
            audio_engine.sync(&segments, time_s, playing, playback_speed);
        }

        // Sync preview surface (render vello, register texture)
        if let Err(error) = self.sync_preview_surface(frame) {
            self.shell.set_render_error(error);
        }

        // Render the UI
        self.shell.ui(ui, self.preview_texture_id);

        // Capture window geometry for persistence
        let (size, maximized) = ui.ctx().input(|i| {
            let rect = i.viewport_rect();
            let max = i.viewport().maximized.unwrap_or(false);
            ([rect.size().x, rect.size().y], max)
        });
        self.shell.window_size = size;
        self.shell.window_maximized = maximized;

        // Request repaint if needed
        if self.shell.is_playing()
            || self.shell.preview_store.preview_dirty
            || self.shell.has_pending_rebuild()
            || self.shell.preview_store.rebuild_in_progress
        {
            ui.ctx().request_repaint();
        }
    }
}

fn live_preview_status(preview: &PreviewPaneState, active_scene: Option<&str>) -> String {
    if let Some(scene) = active_scene {
        format!(
            "{} \u{2022} t = {:.2}s / {:.2}s",
            scene,
            preview.playback.current_time_s(),
            preview.playback.duration_s
        )
    } else {
        format!(
            "Live preview \u{2022} t = {:.2}s / {:.2}s",
            preview.playback.current_time_s(),
            preview.playback.duration_s
        )
    }
}

fn install_theme(ctx: &egui::Context, theme: &eparts::Theme, dark: bool) {
    let sp = spatial_from_ctx(ctx);
    let mut style = (*ctx.global_style()).clone();

    // Tighter spacing
    style.spacing.item_spacing = Vec2::new(sp.base.space_2, sp.base.space_2);
    style.spacing.button_padding = Vec2::new(sp.base.space_4, sp.base.space_2);
    style.spacing.window_margin = egui::Margin::same(sp.base.space_4 as i8);
    style.spacing.indent = sp.base.component.icon_slot_width;

    // Map the eparts Theme onto egui Visuals so stock widgets match the palette.
    style.visuals = theme.to_visuals(dark);

    // Disable selectable labels globally (we handle selection manually)
    style.interaction.selectable_labels = false;

    ctx.set_global_style(style);
}
