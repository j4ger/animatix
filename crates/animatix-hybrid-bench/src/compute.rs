//! Compute backend: classic vello `Renderer` (wgpu/WebGPU), rendered through
//! the production `RendererCore` offscreen→blit path.

use animatix_render::core::RendererCore;
use vello::Scene;
use vello::peniko::Color;
use wasm_bindgen::prelude::JsValue;
use wasm_bindgen_futures::JsFuture;
use web_sys::HtmlCanvasElement;

use crate::bench::{BenchResult, now_ms};

pub struct ComputeRunner {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub core: RendererCore,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    offscreen_view: wgpu::TextureView,
    width: u32,
    height: u32,
}

pub async fn init(
    canvas: HtmlCanvasElement,
    width: u32,
    height: u32,
) -> Result<ComputeRunner, String> {
    canvas.set_width(width.max(1));
    canvas.set_height(height.max(1));
    let width = width.max(1);
    let height = height.max(1);

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        })
        .await
        .map_err(|e| format!("no WebGPU adapter: {e}"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("hybrid-bench device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::default(),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("request_device: {e}"))?;

    let surface = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
        .map_err(|e| format!("create_surface: {e}"))?;
    let caps = surface.get_capabilities(&adapter);
    let format = caps
        .formats
        .iter()
        .find(|f| **f == wgpu::TextureFormat::Rgba8Unorm)
        .or_else(|| caps.formats.first())
        .copied()
        .ok_or_else(|| "surface reports no formats".to_string())?;
    let alpha_mode = caps
        .alpha_modes
        .iter()
        .copied()
        .find(|m| *m == wgpu::CompositeAlphaMode::Auto)
        .unwrap_or_else(|| {
            caps.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto)
        });
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width,
        height,
        present_mode: wgpu::PresentMode::Fifo,
        alpha_mode,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    // Offscreen target vello can render into (needs STORAGE_BINDING).
    let offscreen = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("hybrid-bench offscreen"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let offscreen_view = offscreen.create_view(&wgpu::TextureViewDescriptor::default());

    let core = RendererCore::new(&device, &queue).map_err(|e| format!("renderer init: {e:?}"))?;

    Ok(ComputeRunner {
        device,
        queue,
        core,
        surface,
        config,
        offscreen_view,
        width,
        height,
    })
}

impl ComputeRunner {
    pub fn render(&mut self, scene: &Scene) -> Result<(), String> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&self.device, &self.config);
                return Err("surface outdated (reconfigured)".to_string());
            },
            _ => return Err("surface acquire not ready".to_string()),
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.core
            .render_vello_scene_with_background(
                &self.device,
                &self.queue,
                &self.offscreen_view,
                self.width,
                self.height,
                scene,
                Color::from_rgba8(11, 14, 20, 255),
            )
            .map_err(|e| format!("render: {e:?}"))?;
        self.core.blit_texture_to_format(
            &self.device,
            &self.queue,
            &self.offscreen_view,
            &view,
            self.width,
            self.height,
            1.0,
            self.config.format,
        );
        drop(view);
        self.queue.present(frame);
        Ok(())
    }

    pub async fn drain(&self) {
        let queue = self.queue.clone();
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            queue.on_submitted_work_done(move || {
                let _ = resolve.call0(&JsValue::NULL);
            });
        });
        let _ = JsFuture::from(promise).await;
    }
}

pub async fn run(
    canvas: HtmlCanvasElement,
    scene_kind: &str,
    width: u32,
    height: u32,
    frames: u32,
    warmup: u32,
    reps: u32,
) -> Result<BenchResult, String> {
    let mut runner = init(canvas, width, height).await?;
    let frames = frames.max(1);
    let reps = reps.max(1);

    for i in 0..warmup {
        let s = crate::scene::build_vello(scene_kind, i as f64 / 60.0, width, height);
        runner.render(&s)?;
    }
    runner.drain().await;

    // Fixed-overhead baseline: an empty scene still pays the two compute passes
    // and the blit.
    let empty = Scene::new();
    let mut clear_only = f64::INFINITY;
    for _ in 0..reps {
        let t0 = now_ms();
        for _ in 0..frames {
            runner.render(&empty)?;
        }
        runner.drain().await;
        clear_only = clear_only.min((now_ms() - t0) / frames as f64);
    }

    let mut best_total = f64::INFINITY;
    let mut best_submit = f64::INFINITY;
    for _ in 0..reps {
        let t0 = now_ms();
        for i in 0..frames {
            let s = crate::scene::build_vello(scene_kind, i as f64 / 60.0, width, height);
            runner.render(&s)?;
        }
        let t_submit = now_ms();
        runner.drain().await;
        let t_end = now_ms();
        best_submit = best_submit.min((t_submit - t0) / frames as f64);
        best_total = best_total.min((t_end - t0) / frames as f64);
    }

    Ok(BenchResult {
        backend: "compute".to_string(),
        scene: scene_kind.to_string(),
        width,
        height,
        frames,
        reps,
        per_frame_ms: best_total,
        submit_ms: best_submit,
        gpu_ms: (best_total - best_submit).max(0.0),
        clear_only_ms: clear_only,
    })
}
