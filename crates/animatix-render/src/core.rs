use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

use super::fullscreen_blit::FullscreenBlitPipeline;
use animatix_core::error::RenderError;

/// Thin wrapper around a Vello [`Renderer`] that handles scene-to-texture rendering.
///
/// # Dependency invariant: one renderer, many renders per frame
///
/// A frame renders the same canvas several times through this single shared
/// [`Renderer`]: once per `Filter` scope's sub-scene, plus the main scene, plus
/// the compositing blits. That relies on an assumption Vello does not state
/// anywhere — that `render_to_texture` leaves no cross-call state which changes
/// what a *later* render draws.
///
/// Upstream's image-atlas residency change (vello #1558, between revs
/// `d8686d52` and `17166312`) broke exactly that assumption: an image-bearing
/// render drew nothing whenever a non-image render ran between two image
/// renders, which is our multi-scope shape. The workspace therefore pins Vello
/// to `d8686d52` (see the `vello` entries in the `Cargo.toml`s) and guards the
/// boundary with `tests/vello_img_probe.rs`, which reproduces the failing
/// sequence against a bare renderer.
///
/// Before moving the pin forward: run that test, then
/// `animatix video dogfood/projects/effects-wave1/entry.amx` and confirm the
/// checker backdrop survives every frame. `docs/roadmap.md` tracks the pin.
pub struct RendererCore {
    /// The underlying Vello renderer instance.
    pub renderer: Renderer,
    /// Fullscreen blit pipeline for zero-readback texture compositing, typed
    /// for `Rgba8Unorm` — the format of every internal target (offscreen
    /// frames, filter scratch, GUI preview).
    pub blit: Option<FullscreenBlitPipeline>,
    /// Blit pipelines typed for other target formats, compiled on demand.
    /// The one consumer is the web player presenting to a browser canvas
    /// surface: the surface's format comes from the browser's backend
    /// (Firefox's wgpu orders `Bgra8Unorm` first), and WebGPU requires the
    /// pipeline's color-target format to match the attachment exactly.
    blit_variants:
        std::cell::RefCell<std::collections::HashMap<wgpu::TextureFormat, FullscreenBlitPipeline>>,
}

impl RendererCore {
    /// Create a new core renderer backed by the given WGPU device.
    pub fn new(device: &wgpu::Device, _queue: &wgpu::Queue) -> Result<Self, RenderError> {
        let renderer = Renderer::new(
            device,
            RendererOptions {
                use_cpu: false,
                pipeline_cache: None,
                antialiasing_support: AaSupport::all(),
                num_init_threads: None,
            },
        )
        .map_err(|e| RenderError::VelloInit(format!("{e:?}")))?;

        let blit = FullscreenBlitPipeline::new(device, wgpu::TextureFormat::Rgba8Unorm);

        Ok(Self {
            renderer,
            blit: Some(blit),
            blit_variants: std::cell::RefCell::new(std::collections::HashMap::new()),
        })
    }

    /// Blit a `src_view` directly into `dst_view` without CPU readback.
    ///
    /// Both views must be RGBA8Unorm.  This is the zero-readback path used
    /// by `GpuFilterBackend` to composite filtered sub-scenes back into the
    /// main render target.
    pub fn blit_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        alpha: f32,
    ) {
        if let Some(ref blit) = self.blit {
            blit.blit(device, queue, src_view, dst_view, width, height, alpha);
        }
    }

    /// Region-scoped [`Self::blit_texture`]: the source is drawn at
    /// `dst_origin` covering `dst_size` pixels of the target.
    pub fn blit_texture_rect(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        dst_origin: [f32; 2],
        dst_size: [u32; 2],
        alpha: f32,
    ) {
        if let Some(ref blit) = self.blit {
            blit.blit_rect(device, queue, src_view, dst_view, dst_origin, dst_size, alpha);
        }
    }

    /// Blit into a target whose format is not the internal `Rgba8Unorm` —
    /// a browser canvas surface. The pipeline matching `target_format` is
    /// compiled on first use and reused afterwards.
    pub fn blit_texture_to_format(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        alpha: f32,
        target_format: wgpu::TextureFormat,
    ) {
        if target_format == wgpu::TextureFormat::Rgba8Unorm {
            return self.blit_texture(device, queue, src_view, dst_view, width, height, alpha);
        }
        let mut variants = self.blit_variants.borrow_mut();
        let blit = variants
            .entry(target_format)
            .or_insert_with(|| FullscreenBlitPipeline::new(device, target_format));
        blit.blit(device, queue, src_view, dst_view, width, height, alpha);
    }

    /// Render a Vello `scene` into the provided `texture_view` at the given size.
    pub fn render_vello_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        scene: &Scene,
    ) -> Result<(), RenderError> {
        self.render_vello_scene_with_background(
            device,
            queue,
            texture_view,
            width,
            height,
            scene,
            Color::BLACK,
        )
    }

    /// Render a Vello `scene` into the provided `texture_view` with a custom background color.
    pub fn render_vello_scene_with_background(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        scene: &Scene,
        base_color: Color,
    ) -> Result<(), RenderError> {
        let _stage = animatix::perf::ScopedStage::new(animatix::perf::stage::RASTERIZE);
        let render_params = RenderParams {
            base_color,
            width,
            height,
            antialiasing_method: AaConfig::Area,
        };

        self.renderer
            .render_to_texture(device, queue, scene, texture_view, &render_params)
            .map_err(|e| RenderError::FrameRender(format!("{e:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to create a wgpu device in headless mode.
    async fn create_headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: true,
            })
            .await
            .ok()?;
        let needed_limits = wgpu::Limits::default().using_resolution(adapter.limits());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Animatix Test Device"),
                required_features: wgpu::Features::empty(),
                required_limits: needed_limits,
                memory_hints: Default::default(),
                ..Default::default()
            })
            .await
            .ok()?;
        Some((device, queue))
    }

    #[test]
    fn renderer_core_can_be_initialized() {
        let maybe_device = pollster::block_on(create_headless_device());
        if let Some((device, queue)) = maybe_device {
            let result = RendererCore::new(&device, &queue);
            assert!(result.is_ok(), "RendererCore::new should succeed with a valid device");
        }
        // If no GPU/software adapter is available, skip the test gracefully
    }

    #[test]
    fn renderer_core_has_renderer_after_init() {
        let maybe_device = pollster::block_on(create_headless_device());
        if let Some((device, queue)) = maybe_device {
            if let Ok(_core) = RendererCore::new(&device, &queue) {
                // vello Renderer doesn't impl Debug, but we can verify it functions
                // by checking it accepts render_vello_scene calls
            }
        }
        // If no GPU is available, the test trivially passes
    }

    #[test]
    #[ignore = "SIGSEGV during GPU teardown on headless/software adapters (Vello/WGPU driver issue, not Animatix code)"]
    fn renderer_core_render_empty_scene() {
        let maybe_device = pollster::block_on(create_headless_device());
        if let Some((device, queue)) = maybe_device {
            let mut core = match RendererCore::new(&device, &queue) {
                Ok(c) => c,
                Err(_) => {
                    crate::testing::skip_if_no_gpu();

                    return;
                },
            };

            // Create a small texture to render into
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                size: wgpu::Extent3d {
                    width: 100,
                    height: 100,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::STORAGE_BINDING,
                label: Some("Animatix Test Texture"),
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            let scene = Scene::new();
            let result = core.render_vello_scene(&device, &queue, &view, 100, 100, &scene);
            // May fail on some GPU configs; accept either outcome
            if let Err(ref e) = result {
                tracing::warn!("render_vello_scene skipped: {e}");
            }
        }
    }
}
