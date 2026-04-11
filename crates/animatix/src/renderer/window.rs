use super::core::RendererCore;
use crate::ast::Stmt;
use crate::timeline::{SceneDimensions, Timeline};
use std::sync::Arc;
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

struct State {
    timeline: Timeline,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    core: RendererCore,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
}

impl State {
    async fn new(window: Arc<Window>, timeline: &Timeline) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let needed_limits = wgpu::Limits::default().using_resolution(adapter.limits());

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: needed_limits,
                memory_hints: Default::default(),
                ..Default::default()
            })
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        let core = RendererCore::new(&device, &queue);

        Self {
            timeline: timeline.clone(),
            surface,
            config,
            size,
            core,
            device: Arc::new(device),
            queue: Arc::new(queue),
        }
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn render(&mut self, current_time: f64) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let scene = self.timeline.evaluate(
            current_time,
            SceneDimensions {
                width: self.config.width,
                height: self.config.height,
            },
        );

        let render_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: self.config.width,
                height: self.config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING,
            label: Some("Vello Render Target"),
            view_formats: &[],
        });
        let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.core.render_vello_scene(
            &self.device,
            &self.queue,
            &render_view,
            self.config.width,
            self.config.height,
            &scene,
        );

        let blitter = wgpu::util::TextureBlitter::new(&self.device, self.config.format);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blit Encoder"),
            });
        blitter.copy(&self.device, &mut encoder, &render_view, &view);
        self.queue.submit(std::iter::once(encoder.finish()));

        output.present();

        Ok(())
    }
}

struct App {
    window: Option<Arc<Window>>,
    state: Option<State>,
    timeline: Timeline,
    start_time: Option<Instant>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attributes = Window::default_attributes()
                .with_title("Animatix Static Scene Renderer")
                .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0));

            let window = Arc::new(event_loop.create_window(attributes).unwrap());
            self.window = Some(window.clone());

            let state = pollster::block_on(State::new(window.clone(), &self.timeline));
            self.state = Some(state);

            window.request_redraw();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = &self.window else { return };
        if window.id() != window_id {
            return;
        }

        let Some(state) = &mut self.state else { return };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                state.resize(physical_size);
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let current_time = if let Some(start) = self.start_time {
                    start.elapsed().as_secs_f64()
                } else {
                    let now = Instant::now();
                    self.start_time = Some(now);
                    0.0
                };

                match state.render(current_time) {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        state.resize(state.size);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(wgpu::SurfaceError::Timeout) => {}
                    Err(wgpu::SurfaceError::Other) => {}
                }
                window.request_redraw();
            }
            _ => {}
        }
    }
}

pub fn run(ast: &[Stmt]) {
    let timeline = Timeline::build(ast);

    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App {
        window: None,
        state: None,
        timeline,
        start_time: None,
    };

    event_loop.run_app(&mut app).unwrap();
}
