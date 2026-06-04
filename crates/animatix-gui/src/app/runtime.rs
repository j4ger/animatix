use super::*;
use crate::app::commands::{Command, ShellAction, ViewAction};
use crate::app::persistence::load_app_state;
use crate::app::design_tokens::*;
use eframe::egui;

pub fn run_gui(path: Option<PathBuf>) {
    let initial_path = path
        .or_else(load_app_state)
        .unwrap_or_else(default_file_path);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Animatix")
            .with_inner_size(egui::vec2(
                INITIAL_WINDOW_SIZE.0 as f32,
                INITIAL_WINDOW_SIZE.1 as f32,
            )),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "Animatix",
        options,
        Box::new(move |cc| {
            let app = AnimatixApp::new(cc, initial_path)?;
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
    /// Set to true when a screenshot is requested; cleared after saving.
    screenshot_pending: bool,
}

impl AnimatixApp {
    fn new(cc: &eframe::CreationContext<'_>, initial_path: PathBuf) -> Result<Self, String> {
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

        let preview_surface = PreviewSurface::new(device, queue).map_err(|e| format!("Preview surface init failed: {e}"))?;
        let shell = GuiShell::load(initial_path);

        Ok(Self {
            shell,
            preview_surface,
            preview_texture_id: None,
            screenshot_pending: false,
        })
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        // Skip shortcuts when a text input is focused (e.g., code editor)
        // BUT still allow Ctrl+Z/Ctrl+Shift+Z for undo/redo of property edits
        let wants_keyboard = ctx.egui_wants_keyboard_input();

        let scrub_step_s = self.shell.ui_store.scrub_step_s;

        // Undo/Redo (works even when editor is focused, for property edits)
        if ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::Undo));
        }
        if ctx.input(|i| {
            i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && i.modifiers.shift
        }) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::Redo));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Y) && i.modifiers.ctrl) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::Redo));
        }

        // Save (Ctrl+S)
        if ctx.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::Save));
        }

        // Reload (Ctrl+R)
        if ctx.input(|i| i.key_pressed(egui::Key::R) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::Reload));
        }

        // Rebuild (Ctrl+Shift+R)
        if ctx.input(|i| i.key_pressed(egui::Key::R) && i.modifiers.ctrl && i.modifiers.shift) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::Rebuild));
        }

        // Screenshot (Ctrl+Shift+S or F12)
        if ctx.input(|i| {
            (i.key_pressed(egui::Key::S) && i.modifiers.ctrl && i.modifiers.shift)
                || i.key_pressed(egui::Key::F12)
        }) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            self.screenshot_pending = true;
            self.shell.preview_store.preview.status = "Screenshot requested…".to_string();
        }

        // Skip remaining shortcuts when a text input is focused
        if wants_keyboard {
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::TogglePlayback));
        }

        // Scene jump hotkeys (1/2/3) — jump to Nth scene in composition
        let scene_names = self.shell.document_store.source.document.scene_names();
        if ctx.input(|i| i.key_pressed(egui::Key::Num1)) && !scene_names.is_empty() {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::SelectScene(scene_names[0].clone())));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Num2)) && scene_names.len() >= 2 {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::SelectScene(scene_names[1].clone())));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Num3)) && scene_names.len() >= 3 {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::SelectScene(scene_names[2].clone())));
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Comma)) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::PrevKeyframe));
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Period)) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::NextKeyframe));
        }

        // Arrow keys: nudge selected actors OR scrub timeline
        let has_selection = !self.shell.ui_store.selection.selected_actors.is_empty();
        let arrow_left = ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft));
        let arrow_right = ctx.input(|i| i.key_pressed(egui::Key::ArrowRight));
        let arrow_up = ctx.input(|i| i.key_pressed(egui::Key::ArrowUp));
        let arrow_down = ctx.input(|i| i.key_pressed(egui::Key::ArrowDown));

        if has_selection && (arrow_left || arrow_right || arrow_up || arrow_down) {
            let nudge_step = if ctx.input(|i| i.modifiers.shift) {
                self.shell.ui_store.nudge_step_shift_px
            } else if self.shell.preview_store.preview.overlay.show_grid {
                self.shell.preview_store.preview.overlay.grid_size
            } else {
                self.shell.ui_store.nudge_step_px
            };
            let dx = if arrow_left { -nudge_step } else if arrow_right { nudge_step } else { 0.0 };
            let dy = if arrow_up { -nudge_step } else if arrow_down { nudge_step } else { 0.0 };

            let time_ms = (self.shell.preview_store.preview.playback.current_time_s() * 1000.0) as u64;
            let keyframe_mode = self.shell.ui_store.keyframe_mode;
            let selected: Vec<String> = self.shell.ui_store.selection.selected_actors.iter().cloned().collect();
            let mut edits = Vec::new();
            if let Some(ref timeline) = self.shell.document_store.source.document.timeline {
                for actor in &selected {
                    if let Some(track) = timeline.get_track(actor) {
                        let pos = track.position.as_ref().map(|p| p.evaluate(time_ms)).unwrap_or([0.0, 0.0]);
                        let new_pos = [pos[0] + dx, pos[1] + dy];
                        edits.push(crate::app::panels::PropertyEdit { time_s: None,
                            actor: actor.clone(),
                            property: "position".into(),
                            value: crate::app::panels::PropertyValue::Vec2(new_pos),
                            create_keyframe: keyframe_mode,
                        });
                    }
                }
            }
            for edit in edits {
                self.shell.handle_property_edit(edit);
            }
        } else if arrow_left {
            let new_time = self.shell.preview_store.preview.playback.current_time_s() - scrub_step_s;
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::ScrubTo(new_time)));
        } else if arrow_right {
            let new_time = self.shell.preview_store.preview.playback.current_time_s() + scrub_step_s;
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::ScrubTo(new_time)));
        }

        // Delete key: remove selected actor(s)
        if ctx.input(|i| i.key_pressed(egui::Key::Delete)) && has_selection {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::DeleteSelectedActors));
        }

        // Duplicate selected actor(s) (Ctrl+D)
        if ctx.input(|i| i.key_pressed(egui::Key::D) && i.modifiers.ctrl) && has_selection {
            for label in self.shell.ui_store.selection.selected_actors.iter().cloned().collect::<Vec<_>>() {
                self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::DuplicateActor(label)));
            }
        }

        // Esc: cancel active drag or reset tool mode to Select
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if !matches!(self.shell.ui_store.interaction.drag_state, crate::app::preview::DragState::None) {
                self.shell.ui_store.interaction.drag_state = crate::app::preview::DragState::None;
                self.shell.preview_store.preview.status = "Drag cancelled".to_string();
            } else if self.shell.ui_store.view.tool_mode != crate::app::preview::ToolMode::Select {
                self.shell.ui_store.view.tool_mode = crate::app::preview::ToolMode::Select;
                self.shell.preview_store.preview.status = "Tool: Select".to_string();
            }
        }

        // Tool mode shortcuts
        if ctx.input(|i| i.key_pressed(egui::Key::M)) {
            self.shell.ui_store.view.tool_mode = crate::app::preview::ToolMode::Move;
            self.shell.preview_store.preview.status = "Tool: Move".to_string();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.shift) {
            self.shell.ui_store.view.tool_mode = crate::app::preview::ToolMode::Scale;
            self.shell.preview_store.preview.status = "Tool: Scale".to_string();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::R)) {
            self.shell.ui_store.view.tool_mode = crate::app::preview::ToolMode::Rotate;
            self.shell.preview_store.preview.status = "Tool: Rotate".to_string();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::V)) {
            self.shell.ui_store.view.tool_mode = crate::app::preview::ToolMode::Vertex;
            self.shell.preview_store.preview.status = "Tool: Vertex".to_string();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::P)) {
            self.shell.ui_store.view.tool_mode = crate::app::preview::ToolMode::Pivot;
            self.shell.preview_store.preview.status = "Tool: Pivot".to_string();
        }

        // Zoom-to-selection (F) and zoom-to-all (Shift+F)
        if ctx.input(|i| i.key_pressed(egui::Key::F) && !i.modifiers.shift) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::ZoomToSelection));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F) && i.modifiers.shift) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::ZoomToAll));
        }

        // Command palette (Ctrl+Shift+P)
        if ctx.input(|i| i.key_pressed(egui::Key::P) && i.modifiers.ctrl && i.modifiers.shift) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::View(ViewAction::OpenCommandPalette));
        }

        // Find / Replace (Ctrl+F)
        if ctx.input(|i| i.key_pressed(egui::Key::F) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::View(ViewAction::OpenFindReplace));
        }

        // Group (Ctrl+G) / Ungroup (Ctrl+Shift+G)
        if ctx.input(|i| i.key_pressed(egui::Key::G) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::GroupSelectedActors));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::G) && i.modifiers.ctrl && i.modifiers.shift) {
            self.shell.ui_store.pending_actions.push_back(ShellAction::Command(Command::UngroupSelectedActors));
        }
    }

    fn sync_preview_surface(&mut self, frame: &mut eframe::Frame) -> Result<(), String> {
        let dimensions = self.shell.document_store.source.document.scene_dimensions;
        let render_state = frame
            .wgpu_render_state()
            .ok_or_else(|| "wgpu render state not available".to_string())?;
        let device = &render_state.device;
        let queue = &render_state.queue;

        self.preview_surface.set_dimensions(device, dimensions);

        if self.shell.preview_store.preview_dirty {
            let debug = animatix::timeline::DebugRenderOptions {
                draw_bounds: self.shell.ui_store.view.debug_bounds,
                compute_hit_regions: true,
            };

            let render_result = if let Some(composition) = self.shell.document_store.source.document.composition.as_ref()
            {
                self.preview_surface.render_composition(
                    device,
                    queue,
                    composition,
                    self.shell.preview_store.preview.playback.current_time_s(),
                    debug,
                )
            } else if let Some(timeline) = self.shell.document_store.source.document.timeline.as_ref() {
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
                    }
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
            let mut runtime_diagnostics = Vec::new();
            if let Some(composition) = self.shell.document_store.source.document.composition.as_ref() {
                for scene in composition.scenes.values() {
                    runtime_diagnostics.extend(scene.timeline.runtime_diagnostics());
                }
            } else if let Some(timeline) = self.shell.document_store.source.document.timeline.as_ref() {
                runtime_diagnostics.extend(timeline.runtime_diagnostics());
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
            self.shell.ui_store.selection.hit_regions = self.shell
                .document_store
                .source
                .cached_hit_regions
                .clone();
            self.shell
                .clear_any_error(live_preview_status(
                    &self.shell.preview_store.preview,
                    self.shell.document_store.source.document.active_scene.as_deref(),
                ));
        } else if self.preview_texture_id.is_none()
            && self.preview_surface.dimensions().width > 0
            && self.preview_surface.dimensions().height > 0
        {
            // Register texture if not yet registered
            if let Some(sample_view) = self.preview_surface.sample_view() {
                let mut renderer = render_state.renderer.write();
                let texture_id = renderer.register_native_texture(
                    device,
                    sample_view,
                    wgpu::FilterMode::Linear,
                );
                self.preview_texture_id = Some(texture_id);
            }
            self.shell
                .clear_any_error(live_preview_status(
                    &self.shell.preview_store.preview,
                    self.shell.document_store.source.document.active_scene.as_deref(),
                ));
        }

        Ok(())
    }
}

impl eframe::App for AnimatixApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let [r, g, b, a] = BG_BASE.to_normalized_gamma_f32();
        [r, g, b, a]
    }

    fn on_exit(&mut self) {
        self.shell.save_persistence();
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Prepare frame (hot reload, playback tick, pending rebuild)
        self.shell.prepare_frame();

        // Handle keyboard shortcuts
        self.handle_keyboard_shortcuts(ui.ctx());

        // Sync preview surface (render vello, register texture)
        if let Err(error) = self.sync_preview_surface(frame) {
            self.shell.set_render_error(error);
        }

        // Render the UI
        self.shell.ui(ui, self.preview_texture_id);

        // Handle screenshot events
        if self.screenshot_pending {
            ui.input(|i| {
                for event in &i.raw.events {
                    if let egui::Event::Screenshot { image, .. } = event {
                        if let Err(e) = save_screenshot(image, i.pixels_per_point) {
                            tracing::warn!("Failed to save screenshot: {}", e);
                            self.shell.preview_store.preview.status =
                                format!("Screenshot failed: {}", e);
                        } else {
                            self.shell.preview_store.preview.status = "Screenshot saved".to_string();
                        }
                        self.screenshot_pending = false;
                    }
                }
            });
        }

        // Request repaint if needed
        if self.shell.is_playing() || self.shell.preview_store.preview_dirty || self.shell.has_pending_rebuild() {
            ui.ctx().request_repaint();
        }
    }
}

/// Save a full-viewport screenshot to disk.
/// Files are written to `/tmp/animatix_screenshots/` with a timestamp.
fn save_screenshot(image: &egui::ColorImage, pixels_per_point: f32) -> Result<(), String> {
    let dir = std::path::PathBuf::from("/tmp/animatix_screenshots");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create dir: {e}"))?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let path = dir.join(format!("animatix_{timestamp}.png"));

    // The screenshot image may be at native resolution (retina / HiDPI).
    // Scale it down to logical pixels so the saved PNG matches what the user sees.
    let [img_w, img_h] = image.size;
    let logical_w = (img_w as f32 / pixels_per_point).round() as u32;
    let logical_h = (img_h as f32 / pixels_per_point).round() as u32;

    let raw: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    let src = image::RgbaImage::from_raw(img_w as u32, img_h as u32, raw)
        .ok_or("invalid screenshot buffer")?;
    let resized = image::imageops::resize(
        &src,
        logical_w.max(1),
        logical_h.max(1),
        image::imageops::FilterType::Lanczos3,
    );

    resized
        .save(&path)
        .map_err(|e| format!("save png: {e}"))?;

    tracing::info!("Screenshot saved to {}", path.display());
    Ok(())
}

fn live_preview_status(preview: &PreviewPaneState, active_scene: Option<&str>) -> String {
    if let Some(scene) = active_scene {
        format!(
            "{} \u{2022} t = {:.2}s / {:.2}s",
            scene, preview.playback.current_time_s(), preview.playback.duration_s
        )
    } else {
        format!(
            "Live preview \u{2022} t = {:.2}s / {:.2}s",
            preview.playback.current_time_s(), preview.playback.duration_s
        )
    }
}

fn install_theme(ctx: &egui::Context) {
    const WIDGET_HOVER: Color32 = BG_HOVER;

    let mut style = (*ctx.global_style()).clone();

    // Tighter spacing
    style.spacing.item_spacing = Vec2::new(PAD_S, PAD_S);
    style.spacing.button_padding = Vec2::new(PAD_L, PAD_S);
    style.spacing.window_margin = egui::Margin::same(PAD_L as i8);
    style.spacing.indent = ICON_SLOT_WIDTH;

    // Background hierarchy (darkest to lightest)
    style.visuals.panel_fill = BG_PANEL;
    style.visuals.window_fill = BG_PANEL;
    style.visuals.extreme_bg_color = BG_BASE;
    style.visuals.faint_bg_color = BG_SURFACE;

    // Widget states
    style.visuals.widgets.noninteractive.bg_fill = BG_SURFACE;
    style.visuals.widgets.noninteractive.weak_bg_fill = BG_SURFACE;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(STROKE_WIDTH, BORDER);
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(STROKE_WIDTH, TEXT_SECONDARY);
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(4);

    style.visuals.widgets.inactive.bg_fill = BG_WIDGET;
    style.visuals.widgets.inactive.weak_bg_fill = BG_WIDGET;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(STROKE_WIDTH, BORDER);
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(STROKE_WIDTH, TEXT_PRIMARY);
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(4);

    style.visuals.widgets.hovered.bg_fill = WIDGET_HOVER;
    style.visuals.widgets.hovered.weak_bg_fill = WIDGET_HOVER;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(STROKE_WIDTH, ACCENT_BLUE);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(STROKE_WIDTH, TEXT_PRIMARY);
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(4);

    style.visuals.widgets.active.bg_fill = BG_ACTIVE;
    style.visuals.widgets.active.weak_bg_fill = BG_ACTIVE;
    style.visuals.widgets.active.bg_stroke = Stroke::new(STROKE_WIDTH, ACCENT_BLUE);
    style.visuals.widgets.active.fg_stroke = Stroke::new(STROKE_WIDTH, TEXT_PRIMARY);
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(4);

    // Selection
    style.visuals.selection.bg_fill = accent_selection();
    style.visuals.selection.stroke = Stroke::new(STROKE_WIDTH, ACCENT_BLUE);

    // Text colors
    style.visuals.override_text_color = Some(TEXT_PRIMARY);

    // Strikethrough / separator
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(STROKE_WIDTH, BG_WIDGET);

    ctx.set_global_style(style);
}
