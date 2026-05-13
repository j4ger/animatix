use animatix::renderer::core::RendererCore;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use kurbo::Rect;

pub struct PreviewSurface {
    renderer: RendererCore,
    render_texture: Option<wgpu::Texture>,
    render_view: Option<wgpu::TextureView>,
    sample_texture: Option<wgpu::Texture>,
    sample_view: Option<wgpu::TextureView>,
    dimensions: SceneDimensions,
    /// Per-actor world-space hit regions from the last render call.
    hit_regions: Vec<(String, Rect)>,
}

impl PreviewSurface {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            renderer: RendererCore::new(device, queue),
            render_texture: None,
            render_view: None,
            sample_texture: None,
            sample_view: None,
            dimensions: SceneDimensions {
                width: 0,
                height: 0,
            },
            hit_regions: Vec::new(),
        }
    }

    pub fn dimensions(&self) -> SceneDimensions {
        self.dimensions
    }

    /// Returns the actor hit regions from the last render call.
    pub fn hit_regions(&self) -> &[(String, Rect)] {
        &self.hit_regions
    }

    /// Returns the sample texture view for registration with egui.
    pub fn sample_view(&self) -> Option<&wgpu::TextureView> {
        self.sample_view.as_ref()
    }

    pub fn set_dimensions(&mut self, device: &wgpu::Device, dimensions: SceneDimensions) {
        if dimensions.width == 0 || dimensions.height == 0 || self.dimensions == dimensions {
            return;
        }

        let render_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Animatix Egui Preview Render Texture"),
            size: wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sample_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Animatix Egui Preview Sample Texture"),
            size: wgpu::Extent3d {
                width: dimensions.width,
                height: dimensions.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sample_view = sample_texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.render_texture = Some(render_texture);
        self.render_view = Some(render_view);
        self.sample_texture = Some(sample_texture);
        self.sample_view = Some(sample_view);
        self.dimensions = dimensions;
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        timeline: &Timeline,
        time_s: f64,
        debug_options: DebugRenderOptions,
    ) -> Result<(), String> {
        let render_view = self
            .render_view
            .as_ref()
            .ok_or_else(|| "Preview texture is not initialized".to_string())?;
        let render_texture = self
            .render_texture
            .as_ref()
            .ok_or_else(|| "Preview texture is not initialized".to_string())?;
        let sample_texture = self
            .sample_texture
            .as_ref()
            .ok_or_else(|| "Preview texture is not initialized".to_string())?;

        let scene = timeline.evaluate_with_debug(time_s, self.dimensions, debug_options);
        self.hit_regions = timeline.hit_regions();
        self.renderer.render_vello_scene(
            device,
            queue,
            render_view,
            self.dimensions.width,
            self.dimensions.height,
            &scene,
        );

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Egui Preview Copy Encoder"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: render_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: sample_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: self.dimensions.width,
                height: self.dimensions.height,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(encoder.finish()));

        Ok(())
    }
}
