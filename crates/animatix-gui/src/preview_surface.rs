use animatix::composition::Composition;
use animatix::timeline::effects::FilterBackend;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use animatix_render::core::RendererCore;
use animatix_render::filter_backend::GpuFilterBackend;
use animatix_render::transition::TransitionCompositor;
use kurbo::Rect;

pub struct PreviewSurface {
    renderer: RendererCore,
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
    filter_backend: Option<GpuFilterBackend>,
    filter_backend_from: Option<GpuFilterBackend>,
    filter_backend_to: Option<GpuFilterBackend>,
    filter_backend_dimensions: Option<SceneDimensions>,
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
    renderer: &mut RendererCore,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    timeline: &Timeline,
    time_s: f64,
    dimensions: SceneDimensions,
    debug_options: DebugRenderOptions,
    backend: &mut Option<GpuFilterBackend>,
    target: &wgpu::Texture,
    target_view: &wgpu::TextureView,
) -> Result<Vec<(String, Rect)>, String> {
    if timeline.needs_filter_backend() {
        if backend.is_none() {
            *backend = Some(GpuFilterBackend::new(device.clone(), queue.clone(), dimensions)?);
        }
    } else {
        *backend = None;
    }
    let scene = {
        let mut filter_backend: Option<&mut dyn FilterBackend> =
            backend.as_mut().map(|backend| backend as &mut dyn FilterBackend);
        timeline.evaluate_with_debug(time_s, dimensions, debug_options, &mut filter_backend)
    };
    let hit_regions = timeline.hit_regions();
    animatix_render::offscreen::rasterize_scene(
        renderer,
        device,
        queue,
        target_view,
        &scene,
        dimensions,
        animatix_render::offscreen::RasterFit::OneToOne,
        backend.as_mut(),
        Some(target),
    )?;
    Ok(hit_regions)
}

impl PreviewSurface {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, String> {
        Ok(Self {
            renderer: RendererCore::new(device, queue).map_err(|e| e.to_string())?,
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
            filter_backend: None,
            filter_backend_from: None,
            filter_backend_to: None,
            filter_backend_dimensions: None,
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
                | wgpu::TextureUsages::STORAGE_BINDING
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
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING,
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
        // Each backend owns dimension-sized scratch targets and caches.
        self.filter_backend = None;
        self.filter_backend_from = None;
        self.filter_backend_to = None;
        self.filter_backend_dimensions = None;
        if let Some(compositor) = &self.compositor {
            compositor.clear_cached_textures();
        }
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        timeline: &Timeline,
        time_s: f64,
        debug_options: DebugRenderOptions,
    ) -> Result<(), String> {
        // Keep backend scratch targets aligned with the preview texture size.
        if self.filter_backend_dimensions != Some(self.dimensions) {
            self.filter_backend = None;
            self.filter_backend_from = None;
            self.filter_backend_to = None;
            self.filter_backend_dimensions = Some(self.dimensions);
        }
        let needs_filter_backend = timeline.needs_filter_backend();
        if needs_filter_backend && self.filter_backend.is_none() {
            self.filter_backend =
                self.filter_backend_to.take().or_else(|| self.filter_backend_from.take());
        }
        self.filter_backend_from = None;
        self.filter_backend_to = None;
        if !needs_filter_backend {
            self.filter_backend = None;
        }
        let target = self
            .render_texture
            .as_ref()
            .ok_or_else(|| "Preview texture is not initialized".to_string())?;
        let render_view = self
            .render_view
            .as_ref()
            .ok_or_else(|| "Preview texture is not initialized".to_string())?;
        self.hit_regions = render_scene_into_target(
            &mut self.renderer,
            device,
            queue,
            timeline,
            time_s,
            self.dimensions,
            debug_options,
            &mut self.filter_backend,
            target,
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

                // All scratch slots share the same dimensions. Reset on resize,
                // then move a live direct backend into an FX side when possible.
                if self.filter_backend_dimensions != Some(self.dimensions) {
                    self.filter_backend = None;
                    self.filter_backend_from = None;
                    self.filter_backend_to = None;
                    self.filter_backend_dimensions = Some(self.dimensions);
                }
                let from_needs_filter_backend = from.timeline.needs_filter_backend();
                let to_needs_filter_backend = to.timeline.needs_filter_backend();
                if from_needs_filter_backend
                    && !to_needs_filter_backend
                    && self.filter_backend_from.is_none()
                {
                    self.filter_backend_from = self.filter_backend_to.take();
                }
                if to_needs_filter_backend
                    && !from_needs_filter_backend
                    && self.filter_backend_to.is_none()
                {
                    self.filter_backend_to = self.filter_backend_from.take();
                }
                if let Some(backend) = self.filter_backend.take() {
                    if from_needs_filter_backend && self.filter_backend_from.is_none() {
                        self.filter_backend_from = Some(backend);
                    } else if to_needs_filter_backend && self.filter_backend_to.is_none() {
                        self.filter_backend_to = Some(backend);
                    }
                }

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
                let render_texture = self
                    .render_texture
                    .as_ref()
                    .ok_or_else(|| "Preview render texture is not initialized".to_string())?;
                let render_view_b = self
                    .render_view_b
                    .as_ref()
                    .ok_or_else(|| "Preview render view b is not initialized".to_string())?;
                let render_texture_b = self
                    .render_texture_b
                    .as_ref()
                    .ok_or_else(|| "Preview render texture b is not initialized".to_string())?;
                let composite_view = self
                    .composite_view
                    .as_ref()
                    .ok_or_else(|| "Preview composite view is not initialized".to_string())?;

                // Give each side its own optional backend so pending effect
                // layers drain into their matching transition input texture.
                {
                    let mut from_hit_regions = render_scene_into_target(
                        &mut self.renderer,
                        device,
                        queue,
                        &from.timeline,
                        local_time_s,
                        self.dimensions,
                        debug_options,
                        &mut self.filter_backend_from,
                        render_texture,
                        render_view,
                    )?;

                    // Render to scene to render_texture_b
                    let to_hit_regions = render_scene_into_target(
                        &mut self.renderer,
                        device,
                        queue,
                        &to.timeline,
                        to_local,
                        self.dimensions,
                        debug_options,
                        &mut self.filter_backend_to,
                        render_texture_b,
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
            self.render(device, queue, &scene.timeline, local_time_s, debug_options)?;
        } else {
            self.hit_regions.clear();
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "preview_surface/tests.rs"]
mod tests;
