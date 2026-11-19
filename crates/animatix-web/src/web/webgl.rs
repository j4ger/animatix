//! Per-canvas WebGL2 backend using the shared Vello GPU frame renderer.

use std::{cell::RefCell, sync::Arc};

use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;
use wgpu::rwh::{DisplayHandle, HandleError, HasDisplayHandle};

use super::AmxPlayer;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[derive(Debug)]
struct WebDisplay;

impl HasDisplayHandle for WebDisplay {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Ok(DisplayHandle::web())
    }
}

/// Wgpu GL surfaces are backed by a WebGL2 context owned by one canvas, so
/// each player retains its own instance, adapter, device, queue and renderer.
pub(super) struct Gpu {
    _instance: wgpu::Instance,
    _adapter: wgpu::Adapter,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) frame_gpu: RefCell<animatix_render::frame_gpu::FrameGpu>,
}

impl Gpu {
    async fn new(
        canvas: &HtmlCanvasElement,
    ) -> Result<(wgpu::Surface<'static>, wgpu::SurfaceConfiguration, Self), String> {
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::GL,
            ..wgpu::InstanceDescriptor::new_with_display_handle(Box::new(WebDisplay))
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|error| format!("WebGL2 canvas: {error}"))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|error| format!("WebGL2 adapter: {error}"))?;
        if adapter.get_info().backend != wgpu::Backend::Gl {
            return Err("expected a wgpu GL adapter for WebGL2".to_string());
        }
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("animatix WebGL2 device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits {
                    max_texture_dimension_2d: adapter.limits().max_texture_dimension_2d,
                    max_buffer_size: adapter.limits().max_buffer_size,
                    ..wgpu::Limits::downlevel_webgl2_defaults()
                },
                memory_hints: wgpu::MemoryHints::default(),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("WebGL2 device: {error}"))?;
        device.on_uncaptured_error(Arc::new(|error| {
            web_sys::console::error_1(&format!("wgpu WebGL2 uncaptured error: {error}").into());
        }));
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| "no WebGL2 surface configuration".to_string())?;
        let capabilities = surface.get_capabilities(&adapter);
        if capabilities.formats.contains(&FORMAT) {
            config.format = FORMAT;
        }
        if capabilities.alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
            config.alpha_mode = wgpu::CompositeAlphaMode::PreMultiplied;
        }
        surface.configure(&device, &config);
        let frame_gpu = animatix_render::frame_gpu::FrameGpu::new(&device, &queue);
        Ok((
            surface,
            config,
            Self {
                _instance: instance,
                _adapter: adapter,
                device,
                queue,
                frame_gpu: RefCell::new(frame_gpu),
            },
        ))
    }
}

/// Build the WebGL2 player for one canvas. Its device stays with that canvas;
/// the page runtime still pins one backend mode for every player.
pub(super) async fn create_webgl_player(canvas: HtmlCanvasElement) -> Result<AmxPlayer, JsError> {
    console_error_panic_hook::set_once();
    let (surface, config, gpu) = Gpu::new(&canvas).await.map_err(|error| JsError::new(&error))?;
    let mut player = AmxPlayer::new(canvas, surface, config);
    player.webgl = Some(std::rc::Rc::new(gpu));
    Ok(player)
}
