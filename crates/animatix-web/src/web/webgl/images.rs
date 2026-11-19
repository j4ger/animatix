use std::collections::HashMap;

use vello::peniko::{ImageAlphaType, ImageBrush, ImageFormat, ImageQuality};
use vello_gpu::{Image, ImageSource, TextureBindings, TextureId, TextureRegion};

#[derive(Default)]
pub(super) struct Images {
    views: HashMap<TextureId, wgpu::TextureView>,
}

impl Images {
    pub(super) fn clear(&mut self) {
        self.views.clear();
    }

    pub(super) fn bind(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        brush: &ImageBrush,
        bindings: &mut TextureBindings,
    ) -> Result<Image, String> {
        // Inline filters use small sequential IDs in their own half of this namespace.
        let id = TextureId(brush.image.data.id() | (1u64 << 63));
        let view = if let Some(view) = self.views.get(&id) {
            view.clone()
        } else {
            let view = upload(device, queue, brush)?;
            self.views.insert(id, view.clone());
            view
        };
        let mut sampler = brush.sampler;
        if sampler.quality == ImageQuality::High {
            return Err("WebGL2 experiment requires low or medium image sampling".into());
        }
        // The external texture path resolves through a regular filterable 2D view.
        sampler.alpha = sampler.alpha.clamp(0.0, 1.0);
        bindings.insert(id, view);
        Ok(Image {
            image: ImageSource::external_texture(id, TextureRegion::Full, true),
            sampler,
        })
    }
}

fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    brush: &ImageBrush,
) -> Result<wgpu::TextureView, String> {
    let image = &brush.image;
    let size = wgpu::Extent3d {
        width: image.width,
        height: image.height,
        depth_or_array_layers: 1,
    };
    let max = device.limits().max_texture_dimension_2d.min(u32::from(u16::MAX));
    if image.width == 0 || image.height == 0 || image.width.max(image.height) > max {
        return Err("image exceeds WebGL2 texture limits".into());
    }
    let mut bytes = image.data.data().to_vec();
    if Some(bytes.len()) != image.format.size_in_bytes(image.width, image.height) {
        return Err("invalid raster image byte count".into());
    }
    for pixel in bytes.chunks_exact_mut(4) {
        if image.format == ImageFormat::Bgra8 {
            pixel.swap(0, 2);
        }
        if image.alpha_type == ImageAlphaType::Alpha {
            let alpha = u16::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
            }
        }
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Animatix WebGL2 image"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(image.width * 4),
            rows_per_image: Some(image.height),
        },
        size,
    );
    Ok(texture.create_view(&Default::default()))
}
