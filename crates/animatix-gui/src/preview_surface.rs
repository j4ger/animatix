use animatix::renderer::core::RendererCore;
use animatix::renderer::transition::TransitionCompositor;
use animatix::composition::Composition;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use kurbo::Rect;

pub struct PreviewSurface {
    renderer: RendererCore,
    render_texture: Option<wgpu::Texture>,
    render_view: Option<wgpu::TextureView>,
    render_texture_b: Option<wgpu::Texture>,
    render_view_b: Option<wgpu::TextureView>,
    composite_texture: Option<wgpu::Texture>,
    composite_view: Option<wgpu::TextureView>,
    sample_texture: Option<wgpu::Texture>,
    sample_view: Option<wgpu::TextureView>,
    compositor: Option<TransitionCompositor>,
    dimensions: SceneDimensions,
    /// Per-actor world-space hit regions from the last render call.
    hit_regions: Vec<(String, Rect)>,
}

impl PreviewSurface {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, String> {
        Ok(Self {
            renderer: RendererCore::new(device, queue).map_err(|e| e.to_string())?,
            render_texture: None,
            render_view: None,
            render_texture_b: None,
            render_view_b: None,
            composite_texture: None,
            composite_view: None,
            sample_texture: None,
            sample_view: None,
            compositor: None,
            dimensions: SceneDimensions {
                width: 0,
                height: 0,
            },
            hit_regions: Vec::new(),
        })
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

        // Primary render target (single scene or "from" scene during transition)
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
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Secondary render target ("to" scene during transition)
        let render_texture_b = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Animatix Egui Preview Render Texture B"),
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
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let render_view_b = render_texture_b.create_view(&wgpu::TextureViewDescriptor::default());

        // Composite output target (transition compositor writes here)
        let composite_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Animatix Egui Preview Composite Texture"),
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
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let composite_view = composite_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Sample texture for egui display (sRGB)
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
        self.render_texture_b = Some(render_texture_b);
        self.render_view_b = Some(render_view_b);
        self.composite_texture = Some(composite_texture);
        self.composite_view = Some(composite_view);
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
        let scene = timeline.evaluate_with_debug(time_s, self.dimensions, debug_options);
        self.hit_regions = timeline.hit_regions();

        let render_view = self
            .render_view
            .as_ref()
            .ok_or_else(|| "Preview texture is not initialized".to_string())?;

        self.renderer
            .render_vello_scene(
                device,
                queue,
                render_view,
                self.dimensions.width,
                self.dimensions.height,
                &scene,
            )
            .map_err(|e| e.to_string())?;

        self.copy_to_sample(device, queue)
    }

    fn copy_to_sample(&self, device: &wgpu::Device, queue: &wgpu::Queue) -> Result<(), String> {
        let render_texture = self
            .render_texture
            .as_ref()
            .ok_or_else(|| "Preview render texture is not initialized".to_string())?;
        let sample_texture = self
            .sample_texture
            .as_ref()
            .ok_or_else(|| "Preview sample texture is not initialized".to_string())?;

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

    fn copy_texture_to_sample(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source_texture: &wgpu::Texture,
    ) -> Result<(), String> {
        let sample_texture = self
            .sample_texture
            .as_ref()
            .ok_or_else(|| "Preview sample texture is not initialized".to_string())?;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Egui Preview Copy Encoder"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source_texture,
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

    pub fn render_composition(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        composition: &Composition,
        global_time_s: f64,
        debug_options: DebugRenderOptions,
    ) -> Result<(), String> {
        let (scene_name, local_time_s, transition_blend) = composition.evaluate(global_time_s);

        if let Some(blend) = transition_blend {
            let from_scene = composition.scenes.get(&blend.from_scene);
            let to_scene = composition.scenes.get(&blend.to_scene);

            if let (Some(from), Some(to)) = (from_scene, to_scene) {
                let to_start = composition
                    .scene_start_times
                    .get(&blend.to_scene)
                    .copied()
                    .unwrap_or(0.0);
                let to_local = global_time_s - to_start;

                // Lazy-init compositor
                if self.compositor.is_none() {
                    self.compositor = Some(TransitionCompositor::new(device).map_err(|e| e.to_string())?);
                }
                let compositor = self.compositor.as_ref().unwrap();

                let render_view = self.render_view.as_ref()
                    .ok_or_else(|| "Preview render view is not initialized".to_string())?;
                let render_view_b = self.render_view_b.as_ref()
                    .ok_or_else(|| "Preview render view b is not initialized".to_string())?;
                let composite_view = self.composite_view.as_ref()
                    .ok_or_else(|| "Preview composite view is not initialized".to_string())?;

                // Render from scene to render_texture, then drop scene_a
                {
                    let scene_a = from.timeline.evaluate_with_debug(local_time_s, self.dimensions, debug_options);
                    self.hit_regions = from.timeline.hit_regions();
                    self.renderer.render_vello_scene(
                        device, queue, render_view,
                        self.dimensions.width, self.dimensions.height, &scene_a,
                    ).map_err(|e| e.to_string())?;
                }

                // Render to scene to render_texture_b
                {
                    let scene_b = to.timeline.evaluate_with_debug(to_local, self.dimensions, debug_options);
                    self.renderer.render_vello_scene(
                        device, queue, render_view_b,
                        self.dimensions.width, self.dimensions.height, &scene_b,
                    ).map_err(|e| e.to_string())?;
                }

                // Composite both scenes
                compositor.render(
                    device,
                    queue,
                    render_view,
                    render_view_b,
                    composite_view,
                    self.dimensions.width,
                    self.dimensions.height,
                    blend.progress as f32,
                    blend.transition_type,
                ).map_err(|e| e.to_string())?;

                // Copy composite result to sample texture
                self.copy_texture_to_sample(device, queue, self.composite_texture.as_ref().unwrap())?;
            } else {
                self.hit_regions.clear();
            }
        } else if let Some(scene) = composition.scenes.get(&scene_name) {
            self.render(device, queue, &scene.timeline, local_time_s, debug_options)?;
        } else {
            self.hit_regions.clear();
        }

        Ok(())
    }
}
