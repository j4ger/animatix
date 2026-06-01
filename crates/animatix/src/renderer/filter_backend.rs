//! Shared GPU filter backend for preview and export renderers.
//!
//! Both [`crate::renderer::offscreen::OffscreenRenderer`] and the GUI's
//! [`PreviewSurface`](crate::preview_surface::PreviewSurface) need identical
//! offscreen → readback → CPU filter behaviour for [`Filter`](crate::timeline::ActorKindId::Filter)
//! actors.  This module provides a single [`GpuFilterBackend`] implementation
//! that can be instantiated by any renderer that owns (or can borrow) a
//! [`wgpu::Device`] and [`wgpu::Queue`].

use crate::renderer::core::RendererCore;
use crate::timeline::filter::FilterBackend;
use crate::timeline::image::SceneImage;
use crate::timeline::SceneDimensions;

/// GPU-backed filter backend that owns its own temporary targets and a
/// dedicated [`RendererCore`] so it never contends with the main renderer.
///
/// Created on-demand per evaluation by cloning the caller's `Device`/`Queue`
/// and spinning up a fresh Vello renderer.  The overhead is acceptable because
/// filter evaluation is only triggered when a `Filter` actor is present and
/// its properties are non-identity.
pub struct GpuFilterBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    core: RendererCore,
    output_texture: Option<wgpu::Texture>,
    output_view: Option<wgpu::TextureView>,
    output_buffer: Option<wgpu::Buffer>,
    bytes_per_row: u32,
    dimensions: SceneDimensions,
}

impl GpuFilterBackend {
    /// Create a new backend from a cloned device/queue pair.
    ///
    /// # Errors
    /// Returns an error if the inner [`RendererCore`] fails to initialise.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        dimensions: SceneDimensions,
    ) -> Result<Self, String> {
        let core = RendererCore::new(&device, &queue)
            .map_err(|e| format!("Failed to create filter renderer core: {e}"))?;
        let bytes_per_row = (dimensions.width * 4 + 255) & !255;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            label: Some("Animatix Filter Temp Texture"),
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            size: (bytes_per_row * dimensions.height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            label: Some("Animatix Filter Temp Buffer"),
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            core,
            output_texture: Some(texture),
            output_view: Some(view),
            output_buffer: Some(buffer),
            bytes_per_row,
            dimensions,
        })
    }
}

impl FilterBackend for GpuFilterBackend {
    fn render_scene_to_image(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
    ) -> Result<SceneImage, String> {
        let output_view = self
            .output_view
            .as_ref()
            .ok_or_else(|| "Missing filter output view".to_string())?;

        self.core
            .render_vello_scene_with_background(
                &self.device,
                &self.queue,
                output_view,
                dimensions.width,
                dimensions.height,
                scene,
                vello::peniko::Color::TRANSPARENT,
            )
            .map_err(|e| e.to_string())?;

        let output_texture = self
            .output_texture
            .as_ref()
            .ok_or_else(|| "Missing filter output texture".to_string())?;
        let output_buffer = self
            .output_buffer
            .as_ref()
            .ok_or_else(|| "Missing filter output buffer".to_string())?;

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Animatix Filter Readback Encoder"),
            });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: output_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.bytes_per_row),
                    rows_per_image: Some(dimensions.height),
                },
            },
            wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        let buffer_slice = output_buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|err| format!("Failed to poll GPU device: {err}"))?;
        rx.recv()
            .map_err(|err| format!("Failed to receive mapped frame: {err}"))?
            .map_err(|err| format!("Failed to map filter frame: {err}"))?;

        let data = buffer_slice.get_mapped_range();
        let mut rgba = vec![0; (dimensions.width * dimensions.height * 4) as usize];
        for y in 0..dimensions.height as usize {
            let src_row = &data[y * self.bytes_per_row as usize
                ..y * self.bytes_per_row as usize + dimensions.width as usize * 4];
            let dst_row = &mut rgba[y * dimensions.width as usize * 4
                ..(y + 1) * dimensions.width as usize * 4];
            dst_row.copy_from_slice(src_row);
        }
        drop(data);
        output_buffer.unmap();

        let data = vello::peniko::ImageData {
            data: rgba.into(),
            format: vello::peniko::ImageFormat::Rgba8,
            alpha_type: vello::peniko::ImageAlphaType::Alpha,
            width: dimensions.width,
            height: dimensions.height,
        };

        Ok(SceneImage {
            data,
            natural_size: [dimensions.width as f32, dimensions.height as f32],
        })
    }
}
