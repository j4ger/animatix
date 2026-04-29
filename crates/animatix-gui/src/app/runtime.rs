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
        let scrub_step_s = 0.1;

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
        let [r, g, b, a] = Color32::from_rgb(18, 20, 24).to_normalized_gamma_f32();
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
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(12.0, 8.0);
    style.spacing.window_margin = egui::Margin::same(8);
    style.visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(22, 25, 31);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(35, 39, 47);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(49, 55, 66);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(70, 78, 92);
    style.visuals.panel_fill = Color32::from_rgb(18, 20, 24);
    ctx.set_global_style(style);
}
