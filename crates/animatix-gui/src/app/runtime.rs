use super::*;

pub fn run_gui(path: Option<PathBuf>) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let initial_path = path.unwrap_or_else(default_file_path);
    let mut app = GuiRuntime::new(initial_path);
    event_loop.run_app(&mut app).expect("Failed to run GUI app");
}

struct GuiRuntime {
    initial_path: PathBuf,
    runtime: Option<WindowRuntime>,
}

impl GuiRuntime {
    fn new(initial_path: PathBuf) -> Self {
        Self {
            initial_path,
            runtime: None,
        }
    }
}

impl ApplicationHandler for GuiRuntime {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.runtime.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Animatix")
            .with_inner_size(LogicalSize::new(
                INITIAL_WINDOW_SIZE.0,
                INITIAL_WINDOW_SIZE.1,
            ));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("Failed to create window"),
        );

        self.runtime = Some(
            WindowRuntime::new(window, self.initial_path.clone())
                .expect("Failed to initialize egui runtime"),
        );
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        if runtime.window.id() != window_id {
            return;
        }

        if runtime.on_window_event(event_loop, &event) {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(runtime) = self.runtime.as_ref()
            && runtime.needs_redraw()
        {
            runtime.window.request_redraw();
        }
    }
}

struct WindowRuntime {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    render_pass: RenderPass,
    egui_ctx: egui::Context,
    egui_winit: EguiWinitState,
    shell: GuiShell,
    preview_surface: PreviewSurface,
}

impl WindowRuntime {
    fn new(window: Arc<Window>, initial_path: PathBuf) -> Result<Self, String> {
        let (surface, surface_config, device, queue, surface_format) =
            pollster::block_on(create_graphics_state(window.clone()))?;

        let egui_ctx = egui::Context::default();
        egui_ctx.set_visuals(egui::Visuals::dark());
        install_theme(&egui_ctx);

        let egui_winit = EguiWinitState::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(device.limits().max_texture_dimension_2d as usize),
        );

        let render_pass = RenderPass::new(&device, surface_format, 1);
        let preview_surface = PreviewSurface::new(&device, &queue);
        let shell = GuiShell::load(initial_path);

        Ok(Self {
            window,
            surface,
            surface_config,
            device,
            queue,
            render_pass,
            egui_ctx,
            egui_winit,
            shell,
            preview_surface,
        })
    }

    fn needs_redraw(&self) -> bool {
        self.shell.is_playing() || self.shell.preview_dirty || self.shell.has_pending_rebuild()
    }

    fn handle_keyboard_shortcut(&mut self, event: &winit::event::KeyEvent) -> bool {
        if event.state != ElementState::Pressed || event.repeat {
            return false;
        }

        let scrub_step_s = 0.1;
        match &event.logical_key {
            Key::Named(NamedKey::Space) => {
                self.shell.preview.toggle_playback();
                self.shell.preview_dirty = true;
                true
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.shell.preview.current_time_s -= scrub_step_s;
                self.shell.preview.clamp_time();
                self.shell.preview.is_playing = false;
                self.shell.preview_dirty = true;
                self.shell.preview.status = format!(
                    "Preview scrubbed • t = {:.2}s / {:.2}s",
                    self.shell.preview.current_time_s, self.shell.preview.duration_s
                );
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.shell.preview.current_time_s += scrub_step_s;
                self.shell.preview.clamp_time();
                self.shell.preview.is_playing = false;
                self.shell.preview_dirty = true;
                self.shell.preview.status = format!(
                    "Preview scrubbed • t = {:.2}s / {:.2}s",
                    self.shell.preview.current_time_s, self.shell.preview.duration_s
                );
                true
            }
            _ => false,
        }
    }

    fn on_window_event(&mut self, event_loop: &ActiveEventLoop, event: &WindowEvent) -> bool {
        let response = self.egui_winit.on_window_event(self.window.as_ref(), event);
        if response.repaint {
            self.window.request_redraw();
        }

        match event {
            WindowEvent::KeyboardInput { event, .. } if !response.consumed => {
                if self.handle_keyboard_shortcut(event) {
                    self.window.request_redraw();
                    return false;
                }
            }
            WindowEvent::CloseRequested => {
                self.shell.save_persistence();
                return true;
            }
            WindowEvent::Resized(size) => {
                self.resize(*size);
                self.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw() {
                    self.shell
                        .set_status(format!("Render failed • {error}"), Some(error));
                    self.window.request_redraw();
                }
            }
            _ => {}
        }

        if response.consumed {
            event_loop.set_control_flow(ControlFlow::Poll);
        }

        false
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }

        self.surface_config.width = new_size.width;
        self.surface_config.height = new_size.height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    fn redraw(&mut self) -> Result<(), String> {
        self.shell.prepare_frame();
        self.sync_preview_surface()?;

        let raw_input = self.egui_winit.take_egui_input(self.window.as_ref());
        let preview_texture_id = self.preview_surface.texture_id();
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            self.shell.ui(ctx, preview_texture_id);
        });

        self.egui_winit
            .handle_platform_output(self.window.as_ref(), full_output.platform_output.clone());

        let pixels_per_point = egui_winit::pixels_per_point(&self.egui_ctx, self.window.as_ref());
        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, pixels_per_point);
        let screen_descriptor = ScreenDescriptor {
            physical_width: self.surface_config.width,
            physical_height: self.surface_config.height,
            scale_factor: pixels_per_point,
        };

        self.render_pass
            .add_textures(&self.device, &self.queue, &full_output.textures_delta)
            .map_err(|err| err.to_string())?;
        self.render_pass
            .update_buffers(&self.device, &self.queue, &paint_jobs, &screen_descriptor);

        let surface_texture = match self.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(());
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                return Err("The GPU ran out of memory while acquiring the frame".to_string());
            }
            Err(wgpu::SurfaceError::Timeout) | Err(wgpu::SurfaceError::Other) => return Ok(()),
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Animatix Egui Encoder"),
            });

        self.render_pass
            .execute(
                &mut encoder,
                &view,
                &paint_jobs,
                &screen_descriptor,
                Some(clear_color()),
            )
            .map_err(|err| err.to_string())?;

        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();

        self.render_pass
            .remove_textures(full_output.textures_delta)
            .map_err(|err| err.to_string())?;

        if self.needs_redraw() {
            self.window.request_redraw();
        }

        Ok(())
    }

    fn sync_preview_surface(&mut self) -> Result<(), String> {
        let dimensions = self.shell.preview.dimensions;
        self.preview_surface
            .set_dimensions(&self.device, dimensions);

        if self.shell.preview_dirty {
            if let Some(timeline) = self.shell.document.timeline.as_ref() {
                self.preview_surface.render(
                    &self.device,
                    &self.queue,
                    timeline,
                    self.shell.preview.current_time_s,
                )?;
                let _ = self
                    .preview_surface
                    .sync_egui_texture(&self.device, &mut self.render_pass)?;
                self.shell.preview_dirty = false;
                self.shell.preview.error = None;
                self.shell.preview.status = format!(
                    "Live preview • t = {:.2}s / {:.2}s",
                    self.shell.preview.current_time_s, self.shell.preview.duration_s
                );
            }
        } else if self.preview_surface.texture_id().is_none()
            && self.preview_surface.dimensions().width > 0
            && self.preview_surface.dimensions().height > 0
        {
            let _ = self
                .preview_surface
                .sync_egui_texture(&self.device, &mut self.render_pass)?;
        }

        Ok(())
    }
}

async fn create_graphics_state(
    window: Arc<Window>,
) -> Result<
    (
        wgpu::Surface<'static>,
        wgpu::SurfaceConfiguration,
        wgpu::Device,
        wgpu::Queue,
        wgpu::TextureFormat,
    ),
    String,
> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });
    let size = window.inner_size();
    let surface = instance
        .create_surface(window.clone())
        .map_err(|err| format!("Failed to create window surface: {err}"))?;

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .map_err(|err| format!("Failed to find a compatible GPU adapter: {err}"))?;

    let limits = wgpu::Limits::default().using_resolution(adapter.limits());
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Animatix GUI Device"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            memory_hints: Default::default(),
            ..Default::default()
        })
        .await
        .map_err(|err| format!("Failed to create GPU device: {err}"))?;

    let caps = surface.get_capabilities(&adapter);
    let format = caps
        .formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .unwrap_or(caps.formats[0]);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: caps.present_modes[0],
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    Ok((surface, config, device, queue, format))
}

fn clear_color() -> wgpu::Color {
    let [r, g, b, a] = Color32::from_rgb(18, 20, 24).to_normalized_gamma_f32();
    wgpu::Color {
        r: r as f64,
        g: g as f64,
        b: b as f64,
        a: a as f64,
    }
}

fn install_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(12.0, 8.0);
    style.spacing.window_margin = egui::Margin::same(8);
    style.visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(22, 25, 31);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(35, 39, 47);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(49, 55, 66);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(70, 78, 92);
    style.visuals.panel_fill = Color32::from_rgb(18, 20, 24);
    ctx.set_style(style);
}
