use super::*;
use crate::document::timeline_keyframe_times_s;
use eframe::egui;

pub fn run_gui(path: Option<PathBuf>) {
    let initial_path = path.unwrap_or_else(default_file_path);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Animatix")
            .with_inner_size(egui::vec2(
                INITIAL_WINDOW_SIZE.0 as f32,
                INITIAL_WINDOW_SIZE.1 as f32,
            )),
        ..Default::default()
    };

    eframe::run_native(
        "Animatix",
        options,
        Box::new(move |cc| {
            let app = AnimatixApp::new(cc, initial_path)?;
            Ok(Box::new(app))
        }),
    )
    .expect("Failed to run Animatix GUI");
}

struct AnimatixApp {
    shell: GuiShell,
    preview_surface: PreviewSurface,
    preview_texture_id: Option<egui::TextureId>,
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

        let preview_surface = PreviewSurface::new(device, queue);
        let shell = GuiShell::load(initial_path);

        Ok(Self {
            shell,
            preview_surface,
            preview_texture_id: None,
        })
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        // Skip shortcuts when a text input is focused (e.g., code editor)
        // BUT still allow Ctrl+Z/Ctrl+Shift+Z for undo/redo of property edits
        let wants_keyboard = ctx.egui_wants_keyboard_input();

        let scrub_step_s = 0.1;

        // Undo/Redo (works even when editor is focused, for property edits)
        if ctx.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && !i.modifiers.shift) {
            self.shell.undo();
        }
        if ctx.input(|i| {
            i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && i.modifiers.shift
        }) {
            self.shell.redo();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Y) && i.modifiers.ctrl) {
            self.shell.redo();
        }

        // Skip remaining shortcuts when a text input is focused
        if wants_keyboard {
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
            self.shell.preview.toggle_playback();
            self.shell.preview_dirty = true;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Comma)) {
            let keyframes = self
                .shell
                .document
                .timeline
                .as_ref()
                .map(timeline_keyframe_times_s)
                .unwrap_or_default();
            self.shell.preview.go_to_previous_keyframe(&keyframes);
            self.shell.preview.status = format!(
                "Previous keyframe \u{2022} t = {:.2}s / {:.2}s",
                self.shell.preview.current_time_s, self.shell.preview.duration_s
            );
            self.shell.preview_dirty = true;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Period)) {
            let keyframes = self
                .shell
                .document
                .timeline
                .as_ref()
                .map(timeline_keyframe_times_s)
                .unwrap_or_default();
            self.shell.preview.go_to_next_keyframe(&keyframes);
            self.shell.preview.status = format!(
                "Next keyframe \u{2022} t = {:.2}s / {:.2}s",
                self.shell.preview.current_time_s, self.shell.preview.duration_s
            );
            self.shell.preview_dirty = true;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            self.shell.preview.current_time_s -= scrub_step_s;
            self.shell.preview.clamp_time();
            self.shell.preview.is_playing = false;
            self.shell.preview_dirty = true;
            self.shell.preview.status = format!(
                "Preview scrubbed \u{2022} t = {:.2}s / {:.2}s",
                self.shell.preview.current_time_s, self.shell.preview.duration_s
            );
        }

        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            self.shell.preview.current_time_s += scrub_step_s;
            self.shell.preview.clamp_time();
            self.shell.preview.is_playing = false;
            self.shell.preview_dirty = true;
            self.shell.preview.status = format!(
                "Preview scrubbed \u{2022} t = {:.2}s / {:.2}s",
                self.shell.preview.current_time_s, self.shell.preview.duration_s
            );
        }
    }

    fn sync_preview_surface(&mut self, frame: &mut eframe::Frame) -> Result<(), String> {
        let dimensions = self.shell.document.scene_dimensions;
        let render_state = frame
            .wgpu_render_state()
            .ok_or_else(|| "wgpu render state not available".to_string())?;
        let device = &render_state.device;
        let queue = &render_state.queue;

        self.preview_surface.set_dimensions(device, dimensions);

        if self.shell.preview_dirty {
            if let Some(timeline) = self.shell.document.timeline.as_ref() {
                if let Err(error) = self.preview_surface.render(
                    device,
                    queue,
                    timeline,
                    self.shell.preview.current_time_s,
                ) {
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

                self.shell.preview_dirty = false;
                self.shell.hit_regions = self.preview_surface.hit_regions().to_vec();
                self.shell
                    .clear_render_error(live_preview_status(&self.shell.preview));
            }
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
                .clear_render_error(live_preview_status(&self.shell.preview));
        }

        Ok(())
    }
}

impl eframe::App for AnimatixApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let [r, g, b, a] = Color32::from_rgb(12, 14, 18).to_normalized_gamma_f32();
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

        // Request repaint if needed
        if self.shell.is_playing() || self.shell.preview_dirty || self.shell.has_pending_rebuild() {
            ui.ctx().request_repaint();
        }
    }
}

fn live_preview_status(preview: &PreviewPaneState) -> String {
    format!(
        "Live preview \u{2022} t = {:.2}s / {:.2}s",
        preview.current_time_s, preview.duration_s
    )
}

fn install_theme(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();

    // Tighter spacing
    style.spacing.item_spacing = Vec2::new(4.0, 4.0);
    style.spacing.button_padding = Vec2::new(8.0, 4.0);
    style.spacing.window_margin = egui::Margin::same(6);
    style.spacing.indent = 14.0;

    // Background hierarchy (darkest to lightest)
    style.visuals.panel_fill = Color32::from_rgb(18, 20, 24);          // BG_PANEL
    style.visuals.window_fill = Color32::from_rgb(18, 20, 24);         // BG_PANEL
    style.visuals.extreme_bg_color = Color32::from_rgb(12, 14, 18);    // BG_BASE
    style.visuals.faint_bg_color = Color32::from_rgb(24, 27, 33);      // BG_SURFACE

    // Widget states
    style.visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(24, 27, 33);    // BG_SURFACE
    style.visuals.widgets.noninteractive.weak_bg_fill = Color32::from_rgb(24, 27, 33);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(40, 44, 52)); // BORDER_SUBTLE
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(150, 158, 175)); // TEXT_SECONDARY
    style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(4);

    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(32, 36, 44);          // BG_WIDGET
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(32, 36, 44);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(40, 44, 52));
    style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, Color32::from_rgb(200, 206, 220));
    style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(4);

    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 47, 57);            // BG_HOVER
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(42, 47, 57);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(84, 110, 255)); // BORDER_FOCUS
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::from_rgb(228, 232, 243)); // TEXT_PRIMARY
    style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(4);

    style.visuals.widgets.active.bg_fill = Color32::from_rgb(55, 62, 75);             // BG_ACTIVE
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(55, 62, 75);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, Color32::from_rgb(84, 110, 255));
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::from_rgb(228, 232, 243));
    style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(4);

    // Selection
    style.visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(84, 110, 255, 60);
    style.visuals.selection.stroke = Stroke::new(1.0, Color32::from_rgb(84, 110, 255));

    // Text colors
    style.visuals.override_text_color = Some(Color32::from_rgb(228, 232, 243)); // TEXT_PRIMARY

    // Strikethrough / separator
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(32, 36, 44));

    ctx.set_global_style(style);
}
