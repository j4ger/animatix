use super::core::RendererCore;
use crate::timeline::{SceneDimensions, Timeline};

#[derive(Debug, Clone)]
pub struct RenderedFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct OffscreenRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    core: RendererCore,
    texture: Option<wgpu::Texture>,
    buffer: Option<wgpu::Buffer>,
    dimensions: SceneDimensions,
    bytes_per_row: u32,
}

impl OffscreenRenderer {
    pub fn new() -> Result<Self, String> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, String> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|err| format!("Failed to find an appropriate adapter: {err}"))?;

        let needed_limits = wgpu::Limits::default().using_resolution(adapter.limits());

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Animatix Offscreen Device"),
                required_features: wgpu::Features::empty(),
                required_limits: needed_limits,
                memory_hints: Default::default(),
                ..Default::default()
            })
            .await
            .map_err(|err| format!("Failed to create device: {err}"))?;

        let core = RendererCore::new(&device, &queue);

        Ok(Self {
            device,
            queue,
            core,
            texture: None,
            buffer: None,
            dimensions: SceneDimensions { width: 0, height: 0 },
            bytes_per_row: 0,
        })
    }

    pub fn render_timeline(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        dimensions: SceneDimensions,
    ) -> Result<RenderedFrame, String> {
        if dimensions.width == 0 || dimensions.height == 0 {
            return Err("Preview dimensions must be greater than zero".to_string());
        }

        self.ensure_targets(dimensions);

        let scene = timeline.evaluate(time_s, dimensions);
        let texture = self
            .texture
            .as_ref()
            .ok_or_else(|| "Missing offscreen render target".to_string())?;
        let buffer = self
            .buffer
            .as_ref()
            .ok_or_else(|| "Missing offscreen staging buffer".to_string())?;

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.core.render_vello_scene(
            &self.device,
            &self.queue,
            &view,
            dimensions.width,
            dimensions.height,
            &scene,
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Animatix Offscreen Readback Encoder"),
            });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer,
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

        let buffer_slice = buffer.slice(..);
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
            .map_err(|err| format!("Failed to map preview frame: {err}"))?;

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
        buffer.unmap();

        Ok(RenderedFrame {
            width: dimensions.width,
            height: dimensions.height,
            rgba,
        })
    }

    fn ensure_targets(&mut self, dimensions: SceneDimensions) {
        if self.dimensions == dimensions {
            return;
        }

        self.dimensions = dimensions;
        self.bytes_per_row = (dimensions.width * 4 + 255) & !255;

        self.texture = Some(self.device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING,
            label: Some("Animatix Offscreen Output Texture"),
            view_formats: &[],
        }));

        self.buffer = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
            size: (self.bytes_per_row * dimensions.height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            label: Some("Animatix Offscreen Output Buffer"),
            mapped_at_creation: false,
        }));
    }
}
