use super::core::RendererCore;
use super::error::RenderError;
use super::offscreen::OffscreenRenderer;
use super::RenderedFrame;
use crate::ast::Stmt;
use crate::composition::Composition;
use crate::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use std::sync::Arc;
use std::time::Instant;
use tracing::error;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

struct State {
    timeline: Timeline,
    debug_options: DebugRenderOptions,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    core: RendererCore,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    render_texture: Option<wgpu::Texture>,
    render_view: Option<wgpu::TextureView>,
    /// Cached texture blitter — created once, reused every frame.
    blitter: wgpu::util::TextureBlitter,
}

impl State {
    async fn new(
        window: Arc<Window>,
        event_loop: &ActiveEventLoop,
        timeline: &Timeline,
        debug_options: DebugRenderOptions,
    ) -> Result<Self, RenderError> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(event_loop.owned_display_handle()),
        ));

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| RenderError::SurfaceCreation(format!("{e:?}")))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_e| RenderError::AdapterNotFound)?;

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
            .map_err(|e| RenderError::DeviceRequestFailed(format!("{e:?}")))?;

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

        let core = RendererCore::new(&device, &queue)?;

        let blitter = wgpu::util::TextureBlitter::new(&device, config.format);
        let mut state = Self {
            timeline: timeline.clone(),
            debug_options,
            surface,
            config,
            size,
            core,
            device: Arc::new(device),
            queue: Arc::new(queue),
            render_texture: None,
            render_view: None,
            blitter,
        };
        state.ensure_render_texture();
        Ok(state)
    }

    fn ensure_render_texture(&mut self) {
        let needed = self.render_texture.as_ref().is_none_or(|t| {
            t.width() != self.config.width || t.height() != self.config.height
        });
        if needed {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
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
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.render_texture = Some(texture);
            self.render_view = Some(view);
        }
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.render_texture = None;
            self.render_view = None;
        }
    }

    fn render(&mut self, current_time: f64) -> Result<(), String> {
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Suboptimal(_) => return Ok(()),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.resize(self.size);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Surface validation failed".to_string());
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut fb = None;
        let scene = self.timeline.evaluate_with_debug(
            current_time,
            SceneDimensions {
                width: self.config.width,
                height: self.config.height,
            },
            self.debug_options,
            &mut fb,
        );

        self.ensure_render_texture();
        let render_view = self
            .render_view
            .as_ref()
            .ok_or("Render texture not initialized")?;

        self.core
            .render_vello_scene(
                &self.device,
                &self.queue,
                render_view,
                self.config.width,
                self.config.height,
                &scene,
            )
            .map_err(|e| e.to_string())?;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blit Encoder"),
            });
        self.blitter.copy(&self.device, &mut encoder, render_view, &view);
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
    loop_playback: bool,
    loop_duration_s: Option<f64>,
    debug_options: DebugRenderOptions,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attributes = Window::default_attributes()
                .with_title("Animatix Static Scene Renderer")
                .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0));

            let window = match event_loop.create_window(attributes) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    error!("Failed to create window: {e}");
                    event_loop.exit();
                    return;
                }
            };
            self.window = Some(window.clone());

            let state = match pollster::block_on(State::new(
                window.clone(),
                event_loop,
                &self.timeline,
                self.debug_options,
            )) {
                Ok(s) => s,
                Err(e) => {
                    error!("Renderer initialization failed: {e}");
                    event_loop.exit();
                    return;
                }
            };
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
                let current_time = if self.loop_playback {
                    self.loop_duration_s
                        .filter(|duration| *duration > 0.0)
                        .map(|duration| current_time % duration)
                        .unwrap_or(current_time)
                } else {
                    current_time
                };

                if let Err(e) = state.render(current_time) { error!("Render error: {e}"); }
                window.request_redraw();
            }
            _ => {}
        }
    }
}

/// Open a live preview window and play back the animation described by `ast`.
pub fn run(ast: &[Stmt]) -> Result<(), RenderError> {
    let timeline = Timeline::build(ast);
    run_timeline(timeline)
}

/// Open a live preview window and play back the given `timeline`.
pub fn run_timeline(timeline: Timeline) -> Result<(), RenderError> {
    run_timeline_with_options(timeline, false, DebugRenderOptions::default())
}

/// Open a live preview window and play back the given `timeline` with optional
/// looping and debug visualization.
pub fn run_timeline_with_options(
    timeline: Timeline,
    loop_playback: bool,
    debug_options: DebugRenderOptions,
) -> Result<(), RenderError> {
    let event_loop = EventLoop::new().map_err(|e| RenderError::EventLoopCreation(format!("{e:?}")))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let loop_duration_s = loop_playback.then(|| timeline.duration_seconds());

    let mut app = App {
        window: None,
        state: None,
        timeline,
        start_time: None,
        loop_playback,
        loop_duration_s,
        debug_options,
    };

    event_loop.run_app(&mut app).map_err(|e| RenderError::EventLoopCreation(format!("{e:?}")))
}

// ---------------------------------------------------------------------------
// Composition Preview (multi-scene with transition blending)
// ---------------------------------------------------------------------------

/// Per-frame state for composition live preview.
struct CompositionState {
    composition: Composition,
    debug_options: DebugRenderOptions,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    render_texture: Option<wgpu::Texture>,
    render_view: Option<wgpu::TextureView>,
    offscreen: OffscreenRenderer,
    /// Cached texture blitter — created once, reused every frame.
    blitter: wgpu::util::TextureBlitter,
}

impl CompositionState {
    async fn new(
        window: Arc<Window>,
        event_loop: &ActiveEventLoop,
        composition: &Composition,
        debug_options: DebugRenderOptions,
    ) -> Result<Self, RenderError> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(event_loop.owned_display_handle()),
        ));

        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| RenderError::SurfaceCreation(format!("{e:?}")))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_e| RenderError::AdapterNotFound)?;

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
            .map_err(|e| RenderError::DeviceRequestFailed(format!("{e:?}")))?;

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

        // OffscreenRenderer creates its own internal GPU device/queue for CPU-readback frames.
        let offscreen = OffscreenRenderer::new().map_err(|e| {
            RenderError::DeviceRequestFailed(format!("OffscreenRenderer init failed: {e}"))
        })?;

        let blitter = wgpu::util::TextureBlitter::new(&device, config.format);
        let mut state = Self {
            composition: composition.clone(),
            debug_options,
            surface,
            config,
            size,
            device: Arc::new(device),
            queue: Arc::new(queue),
            render_texture: None,
            render_view: None,
            offscreen,
            blitter,
        };
        state.ensure_render_texture();
        Ok(state)
    }

    fn ensure_render_texture(&mut self) {
        let needed = self.render_texture.as_ref().is_none_or(|t| {
            t.width() != self.config.width || t.height() != self.config.height
        });
        if needed {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                size: wgpu::Extent3d {
                    width: self.config.width,
                    height: self.config.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                label: Some("Composition Blit Source"),
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.render_texture = Some(texture);
            self.render_view = Some(view);
        }
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.render_texture = None;
            self.render_view = None;
        }
    }

    /// Upload a `RenderedFrame`'s RGBA bytes to the intermediate texture and blit to surface.
    fn present_frame(&mut self, frame: &RenderedFrame) -> Result<(), String> {
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Suboptimal(_) => return Ok(()),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.resize(self.size);
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                return Err("Surface validation failed".to_string());
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.ensure_render_texture();
        let render_texture = self
            .render_texture
            .as_ref()
            .ok_or("Render texture not initialized")?;
        let render_view = self
            .render_view
            .as_ref()
            .ok_or("Render view not initialized")?;

        // Upload RGBA bytes to intermediate texture
        let width = frame.width.min(self.config.width);
        let height = frame.height.min(self.config.height);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: render_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.width * 4),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        // Blit intermediate texture to surface
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Composition Blit Encoder"),
            });
        self.blitter.copy(&self.device, &mut encoder, render_view, &view);
        self.queue.submit(std::iter::once(encoder.finish()));

        output.present();
        Ok(())
    }

    fn render(&mut self, current_time: f64) -> Result<(), String> {
        let dims = SceneDimensions {
            width: self.config.width,
            height: self.config.height,
        };

        let (scene_name, local_time, blend) = self.composition.evaluate(current_time);

        if let Some(blend) = blend {
            // Transition between two scenes
            let from_scene = self
                .composition
                .scenes
                .get(&blend.from_scene)
                .ok_or_else(|| format!("Transition from scene '{}' not found", blend.from_scene))?;
            let to_scene = self
                .composition
                .scenes
                .get(&blend.to_scene)
                .ok_or_else(|| format!("Transition to scene '{}' not found", blend.to_scene))?;

            let frame = self.offscreen.render_transition(
                &from_scene.timeline,
                blend.from_local,
                &to_scene.timeline,
                blend.to_local,
                blend.progress as f32,
                blend.id.clone(),
                blend.easing,
                dims,
                self.debug_options,
            )?;
            self.present_frame(&frame)
        } else if let Some(scene) = self.composition.scenes.get(&scene_name) {
            // Single active scene, no transition
            let frame = self.offscreen.render_timeline_with_debug(
                &scene.timeline,
                local_time,
                dims,
                self.debug_options,
            )?;
            self.present_frame(&frame)
        } else {
            // Should not happen for valid compositions
            Ok(())
        }
    }
}

/// Winit application for multi-scene composition live preview with transition blending.
struct CompositionApp {
    window: Option<Arc<Window>>,
    state: Option<CompositionState>,
    composition: Composition,
    start_time: Option<Instant>,
    loop_playback: bool,
    loop_duration_s: Option<f64>,
    debug_options: DebugRenderOptions,
}

impl ApplicationHandler for CompositionApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attributes = Window::default_attributes()
                .with_title("Animatix Composition Preview")
                .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0));

            let window = match event_loop.create_window(attributes) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    error!("Failed to create window: {e}");
                    event_loop.exit();
                    return;
                }
            };
            self.window = Some(window.clone());

            let state = match pollster::block_on(CompositionState::new(
                window.clone(),
                event_loop,
                &self.composition,
                self.debug_options,
            )) {
                Ok(s) => s,
                Err(e) => {
                    error!("Composition renderer initialization failed: {e}");
                    event_loop.exit();
                    return;
                }
            };
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
                let current_time = if self.loop_playback {
                    self.loop_duration_s
                        .filter(|duration| *duration > 0.0)
                        .map(|duration| current_time % duration)
                        .unwrap_or(current_time)
                } else {
                    current_time
                };

                if let Err(e) = state.render(current_time) { error!("Render error: {e}"); }
                window.request_redraw();
            }
            _ => {}
        }
    }
}

/// Open a live preview window with full composition playback and transition blending.
pub fn run_composition_with_options(
    composition: Composition,
    loop_playback: bool,
    debug_options: DebugRenderOptions,
) -> Result<(), RenderError> {
    let event_loop = EventLoop::new().map_err(|e| RenderError::EventLoopCreation(format!("{e:?}")))?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let loop_duration_s = loop_playback.then_some(composition.global_duration_s);

    let mut app = CompositionApp {
        window: None,
        state: None,
        composition,
        start_time: None,
        loop_playback,
        loop_duration_s,
        debug_options,
    };

    event_loop.run_app(&mut app).map_err(|e| RenderError::EventLoopCreation(format!("{e:?}")))
}
