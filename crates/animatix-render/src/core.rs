use animatix_core::error::RenderError;
use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};

use super::fullscreen_blit::FullscreenBlitPipeline;

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
/// Upstream issue #1948 (the patchless-resolve variant of #1558) broke that
/// assumption: an image-free render could discard the persistent image atlas,
/// so the next image render drew nothing. The workspace uses Vello 0.11.0 with
/// a local patch in the root `Cargo.toml` that keeps the atlas alive across
/// image-free renders. `tests/vello_img_probe.rs` asserts the A–H boundary
/// sequence; the remaining upstream fix and patch removal condition are tracked
/// in `docs/roadmap.md`.
pub struct RendererCore {
    /// The underlying Vello renderer instance.
    pub renderer: Renderer,
    /// Fullscreen blit pipeline for zero-readback texture compositing, typed
    /// for `Rgba8Unorm` — the format of every internal target (offscreen
    /// frames, filter scratch, GUI preview).
    pub blit: Option<FullscreenBlitPipeline>,
    /// Hardware line strip pipeline for fast vector paths / curves without CPU triangulation.
    pub line_pipeline: Option<crate::instanced_line::InstancedLinePipeline>,
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
        let line_pipeline = crate::instanced_line::InstancedLinePipeline::new(
            device,
            wgpu::TextureFormat::Rgba8Unorm,
        );

        Ok(Self {
            renderer,
            blit: Some(blit),
            line_pipeline: Some(line_pipeline),
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

    /// Blit a rect with an optional rounded clip and an optional sub-rectangle
    /// of the source texture to sample. A `Glass` backdrop composite needs both:
    /// its texture covers a padded region while only the panel rect is painted.
    pub fn blit_texture_rect_masked(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        dst_origin: [f32; 2],
        dst_size: [u32; 2],
        alpha: f32,
        corner_radius: Option<f32>,
        src_rect: Option<[f32; 4]>,
    ) {
        let mask = match (corner_radius, src_rect) {
            (None, None) => None,
            _ => Some(crate::fullscreen_blit::BlitMask {
                src_rect: src_rect.unwrap_or([0.0, 0.0, 0.0, 0.0]),
                corner_radius: corner_radius.unwrap_or(0.0),
            }),
        };
        if let Some(ref blit) = self.blit {
            blit.blit_rect_masked(
                device,
                queue,
                src_view,
                dst_view,
                dst_origin,
                dst_size,
                alpha,
                mask.as_ref(),
            );
        }
    }

    /// Batched variant of [`Self::blit_texture_rect_masked`]. All items are
    /// executed in a single render pass with dynamic uniform offsets, minimizing
    /// draw call overhead and GPU queue submissions.
    pub fn blit_texture_rect_masked_batch(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        dst_view: &wgpu::TextureView,
        items: &[crate::fullscreen_blit::BatchedBlitItem<'_>],
    ) {
        if let Some(ref blit) = self.blit {
            blit.blit_batch(device, queue, dst_view, items);
        }
    }

    /// Render hardware-accelerated anti-aliased line segments into `target_view`.
    pub fn render_line_segments(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_view: &wgpu::TextureView,
        viewport_size: [f32; 2],
        instances: &[crate::instanced_line::LineSegmentInstance],
    ) {
        if let Some(ref lines) = self.line_pipeline {
            lines.render_lines(device, queue, target_view, viewport_size, instances);
        }
    }

    /// Render hardware-accelerated anti-aliased polyline strip into `target_view`.
    pub fn render_line_strip(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_view: &wgpu::TextureView,
        viewport_size: [f32; 2],
        points: &[[f32; 2]],
        width: f32,
        feather: f32,
        color: [f32; 4],
    ) {
        if let Some(ref lines) = self.line_pipeline {
            let instances = crate::instanced_line::InstancedLinePipeline::segments_from_strip(
                points, width, feather, color,
            );
            lines.render_lines(device, queue, target_view, viewport_size, &instances);
        }
    }

    /// Render a hardware-accelerated parametric curve `f(t)` into `target_view`.
    pub fn render_parametric_curve<F: FnMut(f32) -> [f32; 2]>(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target_view: &wgpu::TextureView,
        viewport_size: [f32; 2],
        f: F,
        t_range: [f32; 2],
        steps: usize,
        width: f32,
        feather: f32,
        color: [f32; 4],
    ) {
        if let Some(ref lines) = self.line_pipeline {
            let instances = crate::instanced_line::InstancedLinePipeline::segments_from_fn(
                f, t_range, steps, width, feather, color,
            );
            lines.render_lines(device, queue, target_view, viewport_size, &instances);
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

    use crate::test_support::create_headless_device;

    #[test]
    fn renderer_core_can_be_initialized() {
        let maybe_device = pollster::block_on(create_headless_device("Animatix Test Device"));
        if let Some((device, queue)) = maybe_device {
            let result = RendererCore::new(&device, &queue);
            assert!(result.is_ok(), "RendererCore::new should succeed with a valid device");
        }
        // If no GPU/software adapter is available, skip the test gracefully
    }
}
