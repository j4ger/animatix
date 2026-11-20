use animatix::composition::Composition;
use animatix::timeline::effects::FilterBackend;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use animatix_render::frame_gpu::{FrameGpu, SceneRecorder};
use animatix_render::transition::TransitionCompositor;
use kurbo::Rect;

pub struct PreviewSurface {
    renderer: FrameGpu,
    render_texture: Option<wgpu::Texture>,
    render_view: Option<wgpu::TextureView>,
    render_sample_view: Option<wgpu::TextureView>,
    render_texture_b: Option<wgpu::Texture>,
    render_view_b: Option<wgpu::TextureView>,
    composite_texture: Option<wgpu::Texture>,
    composite_view: Option<wgpu::TextureView>,
    composite_sample_view: Option<wgpu::TextureView>,
    sample_source: SampleSource,
    compositor: Option<TransitionCompositor>,
    dimensions: SceneDimensions,
    /// Per-actor world-space hit regions from the last render call.
    hit_regions: Vec<(String, Rect)>,
}

#[derive(Clone, Copy)]
enum SampleSource {
    Render,
    Composite,
}

fn render_scene_into_target(
    renderer: &mut FrameGpu,
    timeline: &Timeline,
    time_s: f64,
    dimensions: SceneDimensions,
    debug_options: DebugRenderOptions,
    target: &wgpu::TextureView,
) -> Result<Vec<(String, Rect)>, String> {
    let mut recorder = SceneRecorder;
    let mut backend: Option<&mut dyn FilterBackend> = Some(&mut recorder);
    let scene = timeline.evaluate_with_debug(time_s, dimensions, debug_options, &mut backend);
    renderer.render_frame_into(&scene, target, kurbo::Affine::IDENTITY)?;
    Ok(timeline.hit_regions())
}

impl PreviewSurface {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, String> {
        Ok(Self {
            renderer: FrameGpu::new(device, queue),
            render_texture: None,
            render_view: None,
            render_sample_view: None,
            render_texture_b: None,
            render_view_b: None,
            composite_texture: None,
            composite_view: None,
            composite_sample_view: None,
            sample_source: SampleSource::Render,
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

    /// Takes ownership of the hit regions, leaving an empty Vec in their place.
    /// This avoids a per-frame `.to_vec()` allocation when transferring data
    /// to the document store cache.
    pub fn take_hit_regions(&mut self) -> Vec<(String, Rect)> {
        std::mem::take(&mut self.hit_regions)
    }

    /// Returns the active sRGB sampling view for registration with egui.
    pub fn sample_view(&self) -> Option<&wgpu::TextureView> {
        match self.sample_source {
            SampleSource::Render => {
                self.render_texture.as_ref()?;
                self.render_sample_view.as_ref()
            },
            SampleSource::Composite => {
                self.composite_texture.as_ref()?;
                self.composite_sample_view.as_ref()
            },
        }
    }

    fn set_sample_source(&mut self, source: SampleSource) {
        self.sample_source = source;
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
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let render_sample_view = render_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Animatix Egui Preview Render sRGB View"),
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            usage: Some(wgpu::TextureUsages::TEXTURE_BINDING),
            ..Default::default()
        });

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
                | wgpu::TextureUsages::COPY_DST
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        });
        let composite_view = composite_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let composite_sample_view = composite_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Animatix Egui Preview Composite sRGB View"),
            format: Some(wgpu::TextureFormat::Rgba8UnormSrgb),
            usage: Some(wgpu::TextureUsages::TEXTURE_BINDING),
            ..Default::default()
        });

        self.render_texture = Some(render_texture);
        self.render_view = Some(render_view);
        self.render_sample_view = Some(render_sample_view);
        self.render_texture_b = Some(render_texture_b);
        self.render_view_b = Some(render_view_b);
        self.composite_texture = Some(composite_texture);
        self.composite_view = Some(composite_view);
        self.composite_sample_view = Some(composite_sample_view);
        self.set_sample_source(SampleSource::Render);
        self.dimensions = dimensions;
        if let Some(compositor) = &self.compositor {
            compositor.clear_cached_textures();
        }
    }

    pub fn render(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        debug_options: DebugRenderOptions,
    ) -> Result<(), String> {
        let render_view = self
            .render_view
            .as_ref()
            .ok_or_else(|| "Preview texture is not initialized".to_string())?;
        self.hit_regions = render_scene_into_target(
            &mut self.renderer,
            timeline,
            time_s,
            self.dimensions,
            debug_options,
            render_view,
        )?;

        self.set_sample_source(SampleSource::Render);
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
                let to_start =
                    composition.scene_start_times.get(&blend.to_scene).copied().unwrap_or(0.0);
                let to_local = global_time_s - to_start;

                // Lazy-init compositor
                if self.compositor.is_none() {
                    self.compositor =
                        Some(TransitionCompositor::new(device).map_err(|e| e.to_string())?);
                }
                let compositor = self
                    .compositor
                    .as_ref()
                    .ok_or_else(|| "Compositor not initialized".to_string())?;

                let render_view = self
                    .render_view
                    .as_ref()
                    .ok_or_else(|| "Preview render view is not initialized".to_string())?;
                let render_view_b = self
                    .render_view_b
                    .as_ref()
                    .ok_or_else(|| "Preview render view b is not initialized".to_string())?;
                let composite_view = self
                    .composite_view
                    .as_ref()
                    .ok_or_else(|| "Preview composite view is not initialized".to_string())?;

                {
                    let mut from_hit_regions = render_scene_into_target(
                        &mut self.renderer,
                        &from.timeline,
                        local_time_s,
                        self.dimensions,
                        debug_options,
                        render_view,
                    )?;

                    // Render to scene to render_texture_b
                    let to_hit_regions = render_scene_into_target(
                        &mut self.renderer,
                        &to.timeline,
                        to_local,
                        self.dimensions,
                        debug_options,
                        render_view_b,
                    )?;

                    // Merge hit regions: from scene first, then to scene (to scene is on top)
                    from_hit_regions.extend(to_hit_regions);
                    self.hit_regions = from_hit_regions;
                }

                // Composite both scenes
                compositor
                    .render(
                        device,
                        queue,
                        render_view,
                        render_view_b,
                        composite_view,
                        self.dimensions.width,
                        self.dimensions.height,
                        blend.progress as f32,
                        &blend.id,
                        blend.easing,
                    )
                    .map_err(|e| e.to_string())?;

                self.set_sample_source(SampleSource::Composite);
            } else {
                self.hit_regions.clear();
            }
        } else if let Some(scene) = composition.scenes.get(&scene_name) {
            self.render(&scene.timeline, local_time_s, debug_options)?;
        } else {
            self.hit_regions.clear();
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "preview_surface/tests.rs"]
mod tests;
