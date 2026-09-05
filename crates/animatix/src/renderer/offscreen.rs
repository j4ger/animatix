use super::core::RendererCore;
use super::filter_backend::GpuFilterBackend;
use super::transition::TransitionCompositor;
use crate::timeline::filter::FilterBackend;
use crate::timeline::{DebugRenderOptions, SceneDimensions, Timeline};

/// A single frame rendered to CPU-accessible RGBA memory.
///
/// The pixel buffer is shared (`Arc`): the renderer keeps its own handle to
/// the previous frame's allocation and reuses it in place on the next
/// readback (`Arc::make_mut` is a no-op clone once the encoder drops its
/// reference), so a steady-state video export performs zero per-frame
/// allocations for the multi-megabyte readback (PF-6 round 9: dhat measured
/// 3.7 MB/frame on a 720p export — 93% of the export path's churn).
#[derive(Debug, Clone)]
pub struct RenderedFrame {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Raw RGBA8 pixel data, row-major order.
    pub rgba: std::sync::Arc<Vec<u8>>,
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
    /// Recycled CPU readback buffer (PF-6 round 9): parked between frames and
    /// handed out again once the encoder drops its reference — the 720p RGBA
    /// readback was 3.7 MB allocated per frame (93% of export-path churn).
    readback_buffer: Option<std::sync::Arc<Vec<u8>>>,
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
            readback_buffer: None,
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
        let scene = timeline.evaluate_with_debug(time_s, dimensions, debug_options, &mut fb);

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
        let scene = timeline.evaluate_with_debug(time_s, dimensions, debug_options, &mut fb);
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
        let scene = timeline.evaluate_with_debug(time_s, dimensions, debug_options, &mut fb);
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
            let scene_a =
                from_timeline.evaluate_with_debug(from_time, dimensions, debug_options, &mut fb);
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
            let scene_b =
                to_timeline.evaluate_with_debug(to_time, dimensions, debug_options, &mut fb);
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
        // PF-6 round 9: reuse the previous frame's readback buffer when the
        // encoder has already consumed it (strong count back to our handle);
        // `Arc::make_mut` clones only when someone still holds a reference.
        let mut rgba = match self.readback_buffer.take() {
            Some(prev) if std::sync::Arc::strong_count(&prev) == 1 => {
                let mut prev = std::sync::Arc::try_unwrap(prev).expect("count checked above");
                prev.clear();
                prev.resize((dimensions.width * dimensions.height * 4) as usize, 0);
                std::sync::Arc::new(prev)
            },
            _ => std::sync::Arc::new(vec![0; (dimensions.width * dimensions.height * 4) as usize]),
        };
        {
            let rgba_mut = std::sync::Arc::make_mut(&mut rgba);
            for y in 0..dimensions.height as usize {
                let src_row = &data[y * self.bytes_per_row as usize
                    ..y * self.bytes_per_row as usize + dimensions.width as usize * 4];
                let dst_row = &mut rgba_mut
                    [y * dimensions.width as usize * 4..(y + 1) * dimensions.width as usize * 4];
                dst_row.copy_from_slice(src_row);
            }
        }
        drop(data);
        output_buffer.unmap();

        // Park the buffer for the next frame's readback. The parked copy and
        // the returned frame share one allocation: if the consumer still
        // holds it next frame, `make_mut`/the count check above falls back to
        // a fresh allocation — correctness never depends on the reuse.
        self.readback_buffer = Some(std::sync::Arc::clone(&rgba));

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
mod readback_reuse_tests {
    use super::*;
    use crate::timeline::AnimationTrack;

    fn solid_rect_timeline() -> Timeline {
        let mut timeline = Timeline::new();
        let mut track = AnimationTrack::new("r".to_string());
        track.first_seen_ms = 0;
        track.shape.shape_type = Some({
            let mut t = crate::timeline::PropertyTrack::new(crate::timeline::ShapeType::Rect);
            t.add_keyframe(0, crate::timeline::ShapeType::Rect, crate::easing::Easing::Linear);
            t
        });
        track.geometry.size = Some({
            let mut t = crate::timeline::PropertyTrack::new([100.0, 100.0]);
            t.add_keyframe(0, [100.0, 100.0], crate::easing::Easing::Linear);
            t
        });
        track.style.color = Some({
            let mut t = crate::timeline::PropertyTrack::new([1.0, 0.0, 0.0, 1.0]);
            t.add_keyframe(0, [1.0, 0.0, 0.0, 1.0], crate::easing::Easing::Linear);
            t
        });
        timeline.tracks_mut().insert("r".to_string(), track);
        timeline.root_nodes.push("r".to_string());
        timeline
    }

    /// PF-6 round 9: the readback buffer is parked and reused. The second
    /// render from the same renderer walks the reuse path — its pixels must
    /// be byte-identical to a fresh renderer's (no stale-buffer bleed, no
    /// unzeroed padding), and its `Arc` must share the renderer's parked
    /// buffer while the caller holds it (the fallback only fires when a
    /// consumer keeps the previous frame alive).
    #[test]
    fn readback_reuse_is_pixel_identical_to_fresh_renderer() {
        let mut reused = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => return, // Skip if no GPU
        };
        let mut fresh = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => return,
        };
        let timeline = solid_rect_timeline();
        let dims = SceneDimensions {
            width: 320,
            height: 240,
        };

        // Frame 1 primes the parked buffer; frame 2 takes the reuse path.
        let _ = reused.render_timeline(&timeline, 0.0, dims).expect("frame 1");
        let second = reused.render_timeline(&timeline, 0.0, dims).expect("frame 2");
        let reference = fresh.render_timeline(&timeline, 0.0, dims).expect("reference");

        // The reuse path actually ran: frame 2's buffer IS the parked one.
        let parked = reused.readback_buffer.as_ref().expect("parked buffer");
        assert!(
            std::sync::Arc::ptr_eq(parked, &second.rgba),
            "frame 2 should share the renderer's parked readback buffer"
        );

        assert_eq!(second.rgba.len(), reference.rgba.len());
        assert!(
            second.rgba.as_ref() == reference.rgba.as_ref(),
            "reused-readback frame diverged from a fresh renderer's output"
        );
    }

    /// The parked buffer must not be handed to two callers at once: holding
    /// the previous frame alive forces the next readback onto a fresh
    /// allocation (never a shared mutation).
    #[test]
    fn held_frame_forces_fresh_allocation_not_shared_mutation() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => return,
        };
        let timeline = solid_rect_timeline();
        let dims = SceneDimensions {
            width: 160,
            height: 120,
        };

        let held = renderer.render_timeline(&timeline, 0.0, dims).expect("frame 1");
        let next = renderer.render_timeline(&timeline, 0.0, dims).expect("frame 2");
        assert!(
            !std::sync::Arc::ptr_eq(&held.rgba, &next.rgba),
            "a held frame must force a new buffer, not share the parked one"
        );
        // And the held frame's pixels are untouched by the second render.
        let mid = ((60 * held.width + 80) * 4) as usize;
        assert!(held.rgba[mid] > 200, "held frame was clobbered by the next render");
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

    /// A Mask must clip its children to the mask's own rect AT THE MASK'S
    /// POSITION. (Regression: the clip layer was pushed with the identity
    /// transform, pinning the clip at the scene origin — children of any mask
    /// not at the top-left corner were clipped away entirely, so Mask +
    /// Image rendered nothing.)
    #[test]
    fn mask_clips_children_at_mask_position() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => return, // Skip if no GPU
        };

        // The oversized red child must show through the Mask's clip rect and
        // be clipped outside it. (Regression: the clip layer was pushed at the
        // scene origin, clipping away every child of any Mask not positioned
        // at the top-left corner.)
        let source = r#"
config { resolution: (400, 300) }

#0s
m: Mask, size: (200, 150), at: (300, 150) {
  big: Rect, size: (400, 300), color: (1, 0, 0, 1)
}
"#;
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let ast = ast.expect("AST");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        let timeline = report.output;

        let frame = renderer
            .render_timeline(
                &timeline,
                0.5,
                SceneDimensions {
                    width: 400,
                    height: 300,
                },
            )
            .expect("render should succeed");

        let px = |x: u32, y: u32| -> [u8; 4] {
            let i = ((y * frame.width + x) * 4) as usize;
            [
                frame.rgba[i],
                frame.rgba[i + 1],
                frame.rgba[i + 2],
                frame.rgba[i + 3],
            ]
        };

        // Inside the mask rect (mask spans 200..400 x 75..225): the oversized
        // child rect shows through, clipped to red.
        let inside = px(300, 150);
        assert!(
            inside[0] > 200 && inside[1] < 80 && inside[2] < 80,
            "mask interior should show the red child, got {inside:?}"
        );
        // Outside the mask rect: background, NOT the oversized child.
        let outside = px(60, 150);
        assert!(
            !(outside[0] > 200 && outside[1] < 80 && outside[2] < 80),
            "child must be clipped to the mask rect, found red outside at {outside:?}"
        );
    }

    /// A `clip_shape` child defines the clip geometry (ellipse here) and is
    /// not rendered itself: an oversized red child shows only inside the
    /// ellipse, and the mask-rect corners outside the ellipse stay background.
    #[test]
    fn mask_clip_shape_ellipse_defines_clip_region() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => return, // Skip if no GPU
        };

        let source = r#"
config { resolution: (400, 300) }

m: Mask, size: (200, 150), at: (300, 150) {
  clip_shape: Ellipse, size: (100, 100)
  big: Rect, size: (400, 300), color: (1, 0, 0, 1)
}

#0s
fade-in m [1ms]
"#;
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let ast = ast.expect("AST");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        let timeline = report.output;

        let frame = renderer
            .render_timeline(
                &timeline,
                0.5,
                SceneDimensions {
                    width: 400,
                    height: 300,
                },
            )
            .expect("render should succeed");

        let is_red = |x: u32, y: u32| -> bool {
            let i = ((y * frame.width + x) * 4) as usize;
            let px = [frame.rgba[i], frame.rgba[i + 1], frame.rgba[i + 2]];
            px[0] > 200 && px[1] < 80 && px[2] < 80
        };

        // Mask center: inside the ellipse → red child visible.
        assert!(is_red(300, 150), "ellipse center should show the red child");
        // Mask-rect corner (inside the mask size, outside the ellipse):
        // clipped away → background, and the clip_shape itself must NOT paint.
        assert!(!is_red(215, 85), "outside the ellipse must stay background");
        // Far outside the mask entirely.
        assert!(!is_red(60, 150), "far outside the mask must stay background");
    }

    #[test]
    fn hosted_bar_chart_paints_bars_across_the_full_graph_axis() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => return, // Skip if no GPU
        };

        // Regression: hosted plots used to occupy only the central half of the
        // Graph axis (the `{graph}_size` env key was seeded half but consumed
        // as full). Bars must visibly paint from the left third to the right
        // third of the axis box, not cluster in the middle. (The geometry-level
        // assertion `hosted_bar_chart_spans_graph_axis` only checks the env/
        // paths; this one checks the rasterized frame.)
        let source = r#"
config { resolution: (800, 400) }

#0s
g: Graph, size: (600, 300), at: (400, 200), x_domain: (0, 4), y_domain: (0, 100) {
  bars: BarChart,
    data: {("A", 80), ("B", 60), ("C", 85), ("D", 90)},
    bar_colors: {(1, 0.5, 0.2, 1), (1, 0.5, 0.2, 1), (1, 0.5, 0.2, 1), (1, 0.5, 0.2, 1)}
}
"#;
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let ast = ast.expect("AST");
        let report = crate::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        let timeline = report.output;

        let frame = renderer
            .render_timeline(
                &timeline,
                0.5,
                SceneDimensions {
                    width: 800,
                    height: 400,
                },
            )
            .expect("render should succeed");

        let px = |x: u32, y: u32| -> [u8; 4] {
            let i = ((y * frame.width + x) * 4) as usize;
            [
                frame.rgba[i],
                frame.rgba[i + 1],
                frame.rgba[i + 2],
                frame.rgba[i + 3],
            ]
        };

        // Scan a horizontal strip through the upper-middle of the bars. The
        // Graph spans ~x 100..700; with values 60-90 of a 0..100 domain every
        // bar paints at screen y=330 here. Ancestral "central half" behavior
        // would leave bars within ~250..550 only (see also the vertical-overhang
        // note in probe 008 — a separate hosted-BarChart baseline issue).
        let is_bar = |c: [u8; 4]| c[0] > 150 && c[1] < 180 && c[2] < 130;
        let mut min_x = u32::MAX;
        let mut max_x = 0u32;
        for x in 110..690u32 {
            if is_bar(px(x, 330)) {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
            }
        }
        assert!(
            min_x < 280 && max_x > 520,
            "bars should span the full axis (left third to right third), got min_x={min_x} max_x={max_x}"
        );
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
