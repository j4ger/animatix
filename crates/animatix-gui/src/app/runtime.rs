use super::*;
use crate::app::audio::AudioEngine;
use crate::app::commands::{
    ActorCommand, Command, DocumentCommand, PlaybackCommand, SceneCommand, ShellAction, ViewAction,
    ViewCommand,
};
use crate::app::design_tokens::semantic::{accent, border, surface, text};
use crate::app::design_tokens::spatial::{self, component::ICON_SLOT_WIDTH};
use crate::app::persistence::{
    clear_app_state, load_app_state, load_workspace_persistence, persistence_path, save_app_state,
};
use eframe::egui;

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

        // Set up dark theme
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        install_theme(&cc.egui_ctx);

        // Register Phosphor icon font
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        let preview_surface = PreviewSurface::new(device, queue)
            .map_err(|e| format!("Preview surface init failed: {e}"))?;
        let shell = GuiShell::load(initial_path, show_welcome);

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
        })
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        use crate::app::interaction::keyboard::{FocusContext, KeyboardAction, ShortcutRegistry};
        use crate::app::preview::{DragState, ToolMode};
        use crate::app::shell::insertion_palette::PaletteMode;
        use std::sync::LazyLock;

        static SHORTCUT_REGISTRY: LazyLock<ShortcutRegistry> = LazyLock::new(ShortcutRegistry::new);

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

        if let Some(action) = SHORTCUT_REGISTRY.check(ctx, &focus) {
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
                    self.shell
                        .ui_store
                        .pending_actions
                        .push_back(ActorCommand::DeleteSelectedActors.into());
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
                        let nudge_data: Vec<(String, [f32; 2])> =
                            self.shell.document_store.source.document.active_timeline()
                                .map(|t| {
                                    selected.iter().filter_map(|actor| {
                                        t.get_track(actor).and_then(|track| {
                                            track.position.as_ref().map(|p| {
                                                let pos = p.evaluate(time_ms);
                                                (actor.clone(), [pos[0] + dx, pos[1] + dy])
                                            })
                                        })
                                    }).collect()
                                }).unwrap_or_default();
                        for (actor, new_pos) in &nudge_data {
                            self.shell.handle_property_edit(
                                crate::app::panels::PropertyEdit {
                                    time_s: None,
                                    actor: actor.clone(),
                                    property: "position".into(),
                                    value: crate::app::panels::PropertyValue::Vec2(*new_pos),
                                    create_keyframe: keyframe_mode,
                                },
                            );
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
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let [r, g, b, a] = surface::BASE.to_normalized_gamma_f32();
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

        // Handle keyboard shortcuts
        self.handle_keyboard_shortcuts(ui.ctx());

        // Sync audio with playback state
        if let Some(audio_engine) = &mut self.audio_engine {
            let segments = self.shell.document_store.source.document.all_audio_segments();
            let time_s = self.shell.preview_store.preview.playback.current_time_s();
            let playing = self.shell.preview_store.preview.playback.is_playing;
            audio_engine.sync(&segments, time_s, playing);
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

fn install_theme(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();

    // Tighter spacing
    style.spacing.item_spacing = Vec2::new(spatial::SPACE_2, spatial::SPACE_2);
    style.spacing.button_padding = Vec2::new(spatial::SPACE_4, spatial::SPACE_2);
    style.spacing.window_margin = egui::Margin::same(spatial::SPACE_4 as i8);
    style.spacing.indent = ICON_SLOT_WIDTH;

    // Background hierarchy (darkest to lightest)
    style.visuals.panel_fill = surface::PANEL;
    style.visuals.window_fill = surface::PANEL;
    style.visuals.extreme_bg_color = surface::BASE;
    style.visuals.faint_bg_color = surface::SURFACE;

    // Widget states
    style.visuals.widgets.noninteractive.bg_fill = surface::SURFACE;
    style.visuals.widgets.noninteractive.weak_bg_fill = surface::SURFACE;
    style.visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(spatial::STROKE_WIDTH, border::DEFAULT);
    style.visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(spatial::STROKE_WIDTH, text::SECONDARY);
    style.visuals.widgets.noninteractive.corner_radius =
        egui::CornerRadius::same(spatial::RADIUS_M as u8);

    style.visuals.widgets.inactive.bg_fill = surface::WIDGET;
    style.visuals.widgets.inactive.weak_bg_fill = surface::WIDGET;
    style.visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(spatial::STROKE_WIDTH, border::DEFAULT);
    style.visuals.widgets.inactive.fg_stroke =
        egui::Stroke::new(spatial::STROKE_WIDTH, text::PRIMARY);
    style.visuals.widgets.inactive.corner_radius =
        egui::CornerRadius::same(spatial::RADIUS_M as u8);

    style.visuals.widgets.hovered.bg_fill = surface::HOVER;
    style.visuals.widgets.hovered.weak_bg_fill = surface::HOVER;
    style.visuals.widgets.hovered.bg_stroke =
        egui::Stroke::new(spatial::STROKE_WIDTH, accent::PRIMARY);
    style.visuals.widgets.hovered.fg_stroke =
        egui::Stroke::new(spatial::STROKE_WIDTH, text::PRIMARY);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(spatial::RADIUS_M as u8);

    style.visuals.widgets.active.bg_fill = surface::ACTIVE;
    style.visuals.widgets.active.weak_bg_fill = surface::ACTIVE;
    style.visuals.widgets.active.bg_stroke =
        egui::Stroke::new(spatial::STROKE_WIDTH, accent::PRIMARY);
    style.visuals.widgets.active.fg_stroke =
        egui::Stroke::new(spatial::STROKE_WIDTH, text::PRIMARY);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(spatial::RADIUS_M as u8);

    // Selection
    style.visuals.selection.bg_fill = accent::selection();
    style.visuals.selection.stroke = egui::Stroke::new(spatial::STROKE_WIDTH, accent::PRIMARY);

    // Text colors
    style.visuals.override_text_color = Some(text::PRIMARY);

    // Strikethrough / separator
    style.visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(spatial::STROKE_WIDTH, surface::WIDGET);

    // Disable selectable labels globally (we handle selection manually)
    style.interaction.selectable_labels = false;

    ctx.set_global_style(style);
}
