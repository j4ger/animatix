use super::core::RendererCore;
use super::filter_backend::GpuFilterBackend;
use super::transition::TransitionCompositor;
use crate::timeline::filter::FilterBackend;
use crate::timeline::{DebugRenderOptions, SceneDimensions, Timeline};

/// A single frame rendered to CPU-accessible RGBA memory.
#[derive(Debug, Clone)]
pub struct RenderedFrame {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Raw RGBA8 pixel data, row-major order.
    pub rgba: Vec<u8>,
}

/// GPU-backed offscreen renderer that evaluates a [`Timeline`] and produces
/// [`RenderedFrame`] buffers or intermediate GPU textures.
pub struct OffscreenRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    core: RendererCore,
    output_texture: Option<wgpu::Texture>,
    output_view: Option<wgpu::TextureView>,
    output_buffer: Option<wgpu::Buffer>,
    texture_a: Option<wgpu::Texture>,
    view_a: Option<wgpu::TextureView>,
    texture_b: Option<wgpu::Texture>,
    view_b: Option<wgpu::TextureView>,
    compositor: Option<TransitionCompositor>,
    /// Cached GPU filter backend — recreated only when dimensions change.
    filter_backend: Option<GpuFilterBackend>,
    filter_backend_dimensions: Option<SceneDimensions>,
    dimensions: SceneDimensions,
    bytes_per_row: u32,
}

impl OffscreenRenderer {
    /// Create a new offscreen renderer with an automatically-selected GPU adapter.
    pub fn new() -> Result<Self, String> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

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

        let core =
            RendererCore::new(&device, &queue).map_err(|e| format!("Renderer init failed: {e}"))?;

        Ok(Self {
            device,
            queue,
            core,
            output_texture: None,
            output_view: None,
            output_buffer: None,
            texture_a: None,
            view_a: None,
            texture_b: None,
            view_b: None,
            compositor: None,
            filter_backend: None,
            filter_backend_dimensions: None,
            dimensions: SceneDimensions {
                width: 0,
                height: 0,
            },
            bytes_per_row: 0,
        })
    }

    /// Render a single frame of `timeline` at `time_s` with the given dimensions.
    pub fn render_timeline(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        dimensions: SceneDimensions,
    ) -> Result<RenderedFrame, String> {
        self.render_timeline_with_debug(timeline, time_s, dimensions, DebugRenderOptions::default())
    }

    /// Render a single frame of `timeline` at `time_s` with the given dimensions
    /// and debug visualization options.
    pub fn render_timeline_with_debug(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<RenderedFrame, String> {
        if dimensions.width == 0 || dimensions.height == 0 {
            return Err("Preview dimensions must be greater than zero".to_string());
        }

        self.ensure_targets(dimensions);

        // Evaluate timeline with filter backend support.
        // Reuse cached backend if dimensions match, otherwise recreate.
        if self.filter_backend_dimensions != Some(dimensions) {
            self.filter_backend =
                Some(GpuFilterBackend::new(self.device.clone(), self.queue.clone(), dimensions)?);
            self.filter_backend_dimensions = Some(dimensions);
        }
        let filter_backend = self.filter_backend.as_mut().unwrap();
        let mut fb: Option<&mut dyn crate::timeline::filter::FilterBackend> = Some(filter_backend);
        let scene = timeline
            .evaluate_program_with_debug(time_s, dimensions, debug_options, &mut fb)
            .scene;

        let output_view = self
            .output_view
            .as_ref()
            .ok_or_else(|| "Missing offscreen output view".to_string())?;

        self.core
            .render_vello_scene(
                &self.device,
                &self.queue,
                output_view,
                dimensions.width,
                dimensions.height,
                &scene,
            )
            .map_err(|e| e.to_string())?;

        // Blit pending zero-readback filter composites on top of the rendered scene
        let pending = self
            .filter_backend
            .as_mut()
            .map(|fb| fb.take_pending_composites())
            .unwrap_or_default();

        for composite in pending {
            self.core.blit_texture(
                &self.device,
                &self.queue,
                &composite.view,
                output_view,
                dimensions.width,
                dimensions.height,
                composite.alpha,
            );
        }

        self.readback_output(dimensions)
    }

    /// Render a timeline to the primary offscreen texture (texture_a).
    /// Returns a reference to the texture for use as a compositor input.
    pub fn render_timeline_to_texture_a(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<&wgpu::Texture, String> {
        if dimensions.width == 0 || dimensions.height == 0 {
            return Err("Preview dimensions must be greater than zero".to_string());
        }

        self.ensure_targets(dimensions);

        let mut fb = None;
        let scene = timeline
            .evaluate_program_with_debug(time_s, dimensions, debug_options, &mut fb)
            .scene;
        let view_a = self.view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?;

        self.core
            .render_vello_scene(
                &self.device,
                &self.queue,
                view_a,
                dimensions.width,
                dimensions.height,
                &scene,
            )
            .map_err(|e| e.to_string())?;

        self.texture_a.as_ref().ok_or_else(|| "Missing offscreen texture_a".to_string())
    }

    /// Render a timeline to the secondary offscreen texture (texture_b).
    /// Returns a reference to the texture for use as a compositor input.
    pub fn render_timeline_to_texture_b(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<&wgpu::Texture, String> {
        if dimensions.width == 0 || dimensions.height == 0 {
            return Err("Preview dimensions must be greater than zero".to_string());
        }

        self.ensure_targets(dimensions);

        let mut fb = None;
        let scene = timeline
            .evaluate_program_with_debug(time_s, dimensions, debug_options, &mut fb)
            .scene;
        let view_b = self.view_b.as_ref().ok_or_else(|| "Missing offscreen view_b".to_string())?;

        self.core
            .render_vello_scene(
                &self.device,
                &self.queue,
                view_b,
                dimensions.width,
                dimensions.height,
                &scene,
            )
            .map_err(|e| e.to_string())?;

        self.texture_b.as_ref().ok_or_else(|| "Missing offscreen texture_b".to_string())
    }

    /// Render a transition between two timelines by compositing them with the
    /// given progress and transition type. Returns a CPU-readback frame.
    pub fn render_transition(
        &mut self,
        from_timeline: &Timeline,
        from_time: f64,
        to_timeline: &Timeline,
        to_time: f64,
        progress: f32,
        transition_id: String,
        easing: crate::easing::Easing,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<RenderedFrame, String> {
        if dimensions.width == 0 || dimensions.height == 0 {
            return Err("Preview dimensions must be greater than zero".to_string());
        }

        self.ensure_targets(dimensions);

        // Lazy-init compositor
        if self.compositor.is_none() {
            self.compositor =
                Some(TransitionCompositor::new(&self.device).map_err(|e| e.to_string())?);
        }
        let compositor =
            self.compositor.as_ref().ok_or_else(|| "Missing compositor".to_string())?;

        // Render from scene to texture_a, then drop scene_a before creating scene_b
        // to avoid holding both large vello::Scene objects simultaneously.
        {
            let mut fb = None;
            let scene_a = from_timeline
                .evaluate_program_with_debug(from_time, dimensions, debug_options, &mut fb)
                .scene;
            let view_a =
                self.view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?;
            self.core
                .render_vello_scene(
                    &self.device,
                    &self.queue,
                    view_a,
                    dimensions.width,
                    dimensions.height,
                    &scene_a,
                )
                .map_err(|e| e.to_string())?;
        }

        // Render to scene to texture_b
        {
            let mut fb = None;
            let scene_b = to_timeline
                .evaluate_program_with_debug(to_time, dimensions, debug_options, &mut fb)
                .scene;
            let view_b =
                self.view_b.as_ref().ok_or_else(|| "Missing offscreen view_b".to_string())?;
            self.core
                .render_vello_scene(
                    &self.device,
                    &self.queue,
                    view_b,
                    dimensions.width,
                    dimensions.height,
                    &scene_b,
                )
                .map_err(|e| e.to_string())?;
        }

        // Composite to output_texture
        let output_view = self
            .output_view
            .as_ref()
            .ok_or_else(|| "Missing offscreen output view".to_string())?;
        let view_a = self.view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?;
        let view_b = self.view_b.as_ref().ok_or_else(|| "Missing offscreen view_b".to_string())?;
        compositor
            .render(
                &self.device,
                &self.queue,
                view_a,
                view_b,
                output_view,
                dimensions.width,
                dimensions.height,
                progress,
                &transition_id,
                easing,
            )
            .map_err(|e| e.to_string())?;

        self.readback_output(dimensions)
    }

    /// Copy the output texture to the output buffer and read back CPU-visible RGBA data.
    pub fn readback_output(
        &mut self,
        dimensions: SceneDimensions,
    ) -> Result<RenderedFrame, String> {
        let output_texture = self
            .output_texture
            .as_ref()
            .ok_or_else(|| "Missing offscreen output texture".to_string())?;
        let output_buffer = self
            .output_buffer
            .as_ref()
            .ok_or_else(|| "Missing offscreen output buffer".to_string())?;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Offscreen Readback Encoder"),
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
            tx.send(result).ok();
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
            let dst_row = &mut rgba
                [y * dimensions.width as usize * 4..(y + 1) * dimensions.width as usize * 4];
            dst_row.copy_from_slice(src_row);
        }
        drop(data);
        output_buffer.unmap();

        Ok(RenderedFrame {
            width: dimensions.width,
            height: dimensions.height,
            rgba,
        })
    }

    fn ensure_targets(&mut self, dimensions: SceneDimensions) {
        if self.dimensions == dimensions
            && self.output_texture.is_some()
            && self.texture_a.is_some()
            && self.texture_b.is_some()
        {
            return;
        }

        self.dimensions = dimensions;
        self.bytes_per_row = (dimensions.width * 4 + 255) & !255;

        // Output texture (for single-scene render or compositor output)
        let output_texture = self.device.create_texture(&wgpu::TextureDescriptor {
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
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.output_texture = Some(output_texture);
        self.output_view = Some(output_view);

        self.output_buffer = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
            size: (self.bytes_per_row * dimensions.height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            label: Some("Animatix Offscreen Output Buffer"),
            mapped_at_creation: false,
        }));

        // Texture A (primary intermediate target)
        let texture_a = self.device.create_texture(&wgpu::TextureDescriptor {
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
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING,
            label: Some("Animatix Offscreen Texture A"),
            view_formats: &[],
        });
        let view_a = texture_a.create_view(&wgpu::TextureViewDescriptor::default());
        self.texture_a = Some(texture_a);
        self.view_a = Some(view_a);

        // Texture B (secondary intermediate target)
        let texture_b = self.device.create_texture(&wgpu::TextureDescriptor {
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
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING,
            label: Some("Animatix Offscreen Texture B"),
            view_formats: &[],
        });
        let view_b = texture_b.create_view(&wgpu::TextureViewDescriptor::default());
        self.texture_b = Some(texture_b);
        self.view_b = Some(view_b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::Easing;
    use crate::timeline::{AnimationTrack, PropertyTrack};

    #[test]
    fn offscreen_renderer_can_be_initialized() {
        // OffscreenRenderer::new() uses pollster::block_on internally.
        // In headless/CI environments this may succeed or fail gracefully.
        let result = OffscreenRenderer::new();
        // Should either succeed (GPU available) or fail with a GPU error (no adapter).
        // We just verify it doesn't panic.
        if let Ok(_renderer) = result {
            // Initialization succeeded — renderer is valid
        }
    }

    #[test]
    fn offscreen_renderer_with_zero_dimensions_fails_gracefully() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => return, // Skip if no GPU
        };

        let timeline = Timeline::new();
        let result = renderer.render_timeline(
            &timeline,
            0.0,
            SceneDimensions {
                width: 0,
                height: 0,
            },
        );
        assert!(result.is_err(), "zero dimensions should error");
        assert!(result.unwrap_err().contains("greater than zero"), "should give clear error");
    }

    #[test]
    fn offscreen_renderer_render_timeline_produces_frame() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => return, // Skip if no GPU
        };

        // Create a minimal timeline
        let mut timeline = Timeline::new();
        let mut track = AnimationTrack::new("test".to_string());
        track.first_seen_ms = 0;
        track.style.color = Some({
            let mut t = PropertyTrack::new([1.0, 0.0, 0.0, 1.0]);
            t.add_keyframe(0, [1.0, 0.0, 0.0, 1.0], Easing::Linear);
            t
        });
        timeline.tracks.insert("test".to_string(), track);
        timeline.root_nodes.push("test".to_string());

        let dimensions = SceneDimensions {
            width: 100,
            height: 100,
        };
        let frame = renderer.render_timeline(&timeline, 0.0, dimensions);

        assert!(frame.is_ok(), "render should produce a frame: {:?}", frame.err());
        let frame = frame.expect("frame should be Ok after is_ok check");
        assert_eq!(frame.width, 100);
        assert_eq!(frame.height, 100);
        assert_eq!(frame.rgba.len(), 100 * 100 * 4, "RGBA data should have correct size");
    }

    #[test]
    fn offscreen_renderer_ensure_targets_is_idempotent() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => return, // Skip if no GPU
        };

        let dimensions = SceneDimensions {
            width: 200,
            height: 200,
        };

        // First render may fail on some GPU configs; we just verify no panic
        let timeline = Timeline::new();
        let first = renderer.render_timeline(&timeline, 0.0, dimensions);
        let second = renderer.render_timeline(&timeline, 1.0, dimensions);
        // Both should either succeed or fail consistently
        if first.is_ok() {
            assert!(second.is_ok(), "second render should also succeed");
        }
    }
}
