use super::core::RendererCore;
use super::types::SdfInstance;
use crate::ast::Stmt;
use crate::timeline::Timeline;
use std::sync::Arc;
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};
struct State {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    core: RendererCore,
}

impl State {
    async fn new(window: Arc<Window>, instances: &[SdfInstance]) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
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

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
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

        let core = RendererCore::new(
            device,
            queue,
            size.width,
            size.height,
            surface_format,
            instances,
        )
        .await;

        Self {
            surface,
            config,
            size,
            core,
        }
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.core.device, &self.config);

            self.core
                .camera_uniform
                .update_proj(self.size.width as f32, self.size.height as f32);
            self.core.queue.write_buffer(
                &self.core.camera_buffer,
                0,
                bytemuck::cast_slice(&[self.core.camera_uniform]),
            );
        }
    }

    fn update_instances(&mut self, instances: &[SdfInstance]) {
        if let Some(buffer) = &self.core.instance_buffer {
            if instances.len() as u32 <= self.core.num_instances {
                self.core
                    .queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(instances));
            }
        }
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            self.core
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Render Encoder"),
                });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.core.render_pipeline);
            render_pass.set_bind_group(0, &self.core.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.core.vertex_buffer.slice(..));

            render_pass
                .set_index_buffer(self.core.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.core.num_indices, 0, 0..self.core.num_instances);
        }

        self.core.queue.submit(std::iter::once(encoder.finish()));
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

            let initial_instances = self.timeline.evaluate(0.0);
            let state = pollster::block_on(State::new(window.clone(), &initial_instances));
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
                let instances = self.timeline.evaluate(current_time);
                state.update_instances(&instances);
                match state.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        state.resize(state.size);
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(wgpu::SurfaceError::Timeout) => {}
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
