use super::core::RendererCore;
use super::types::{SdfInstance, TextInstance};
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
    async fn new(window: Arc<Window>, timeline: &Timeline) -> Self {
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

        let all_glyphs = timeline.extract_all_glyphs();
        let font_atlas = crate::renderer::msdf::FontAtlas::new(&device, &queue, &all_glyphs);
        let (instances, text_instances, _) = timeline.evaluate(0.0, &font_atlas);

        let core = RendererCore::new(
            device,
            queue,
            size.width,
            size.height,
            surface_format,
            &instances,
            &text_instances,
            font_atlas,
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

    fn update_instances(&mut self, instances: &[SdfInstance], text_instances: &[TextInstance]) {
        if let Some(buffer) = &self.core.instance_buffer {
            if instances.len() as u32 <= self.core.num_instances {
                self.core
                    .queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(instances));
            }
        }
        if let Some(buffer) = &self.core.text_instance_buffer {
            if text_instances.len() as u32 <= self.core.num_text_instances {
                self.core
                    .queue
                    .write_buffer(buffer, 0, bytemuck::cast_slice(text_instances));
            }
        }
    }

    fn render(&mut self, bg_color: [f32; 4]) -> Result<(), wgpu::SurfaceError> {
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
                            r: bg_color[0] as f64,
                            g: bg_color[1] as f64,
                            b: bg_color[2] as f64,
                            a: bg_color[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            if self.core.num_instances > 0 {
                render_pass.set_pipeline(&self.core.render_pipeline);
                render_pass.set_bind_group(0, &self.core.bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.core.vertex_buffer.slice(..));
                render_pass
                    .set_index_buffer(self.core.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..self.core.num_indices, 0, 0..self.core.num_instances);
            }

            if self.core.num_text_instances > 0 {
                render_pass.set_pipeline(&self.core.text_render_pipeline);
                render_pass.set_bind_group(0, &self.core.text_bind_group, &[]);
                render_pass.set_bind_group(1, &self.core.font_atlas.bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.core.vertex_buffer.slice(..));
                render_pass
                    .set_index_buffer(self.core.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(
                    0..self.core.num_indices,
                    0,
                    0..self.core.num_text_instances,
                );
            }
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
                let (instances, text_instances, bg_color) =
                    self.timeline.evaluate(current_time, &state.core.font_atlas);
                state.update_instances(&instances, &text_instances);
                match state.render(bg_color) {
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
