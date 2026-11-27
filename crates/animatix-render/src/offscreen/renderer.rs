use super::*;

#[derive(Clone, Copy)]
enum ReadbackAlpha {
    Opaque,
    MayBePremultiplied,
}

#[cfg(target_os = "linux")]
fn linux_adapter_order(device_type: wgpu::DeviceType) -> u8 {
    match device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Other => 3,
        wgpu::DeviceType::Cpu => 4,
    }
}

#[cfg(all(test, target_os = "linux"))]
#[test]
fn linux_offscreen_prefers_gpus_regardless_of_discovery_order() {
    use wgpu::DeviceType::{Cpu, DiscreteGpu, IntegratedGpu, Other, VirtualGpu};
    let mut adapters = [Cpu, Other, IntegratedGpu, VirtualGpu, DiscreteGpu];
    adapters.sort_by_key(|&kind| linux_adapter_order(kind));
    assert_eq!(adapters, [DiscreteGpu, IntegratedGpu, VirtualGpu, Other, Cpu]);
}

impl OffscreenRenderer {
    /// Create a new offscreen renderer with an automatically-selected GPU adapter.
    pub fn new() -> Result<Self, String> {
        pollster::block_on(Self::new_async())
    }

    async fn new_async() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

        let (device, queue, core) = Self::initialize_renderer(&instance).await?;

        Ok(Self {
            device,
            queue,
            core,
            output_texture: None,
            output_view: None,
            readback_texture_gpu: None,
            readback_view_gpu: None,
            readback_buffers: [None, None],
            texture_a: None,
            view_a: None,
            texture_b: None,
            view_b: None,
            fitted_targets: None,
            compositor: None,
            dimensions: SceneDimensions {
                width: 0,
                height: 0,
            },
            bytes_per_row: 0,
            readback_buffer: None,
            readback_slot: 0,
        })
    }

    async fn initialize_renderer(
        instance: &wgpu::Instance,
    ) -> Result<(wgpu::Device, wgpu::Queue, RendererCore), String> {
        #[cfg(target_os = "linux")]
        {
            // Driver discovery order is not a hardware/CPU preference. Try
            // each usable GPU before software, including when the first GPU
            // cannot open its device inside a container.
            let mut adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;
            adapters.sort_by_key(|adapter| linux_adapter_order(adapter.get_info().device_type));
            if !adapters.is_empty() {
                let mut errors = Vec::new();
                for adapter in &adapters {
                    match Self::initialize_adapter(adapter).await {
                        Ok(initialized) => return Ok(initialized),
                        Err(error) => errors.push(error),
                    }
                }
                return Err(errors.join("; "));
            }
        }

        // Preserve wgpu's detailed no-adapter report on Linux, and the
        // existing adapter-selection policy on other platforms.
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|err| format!("Failed to find an appropriate adapter: {err}"))?;
        Self::initialize_adapter(&adapter).await
    }

    async fn initialize_adapter(
        adapter: &wgpu::Adapter,
    ) -> Result<(wgpu::Device, wgpu::Queue, RendererCore), String> {
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Animatix Offscreen Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                memory_hints: Default::default(),
                ..Default::default()
            })
            .await
            .map_err(|err| format!("Failed to create device on {}: {err}", info.name))?;
        let core = RendererCore::new(&device, &queue)
            .map_err(|err| format!("Renderer init failed on {}: {err}", info.name))?;
        tracing::debug!(name = %info.name, backend = ?info.backend, device_type = ?info.device_type, "selected offscreen adapter");
        Ok((device, queue, core))
    }

    /// Reference to the underlying WGPU device.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Reference to the underlying WGPU queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Reference to the inner [`RendererCore`].
    pub fn core(&self) -> &RendererCore {
        &self.core
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
        // TEMPORARY perf probe (env-gated): evaluate vs render vs readback.
        let t0 = crate::probe::timing_probe();
        self.render_to_output_texture(timeline, time_s, dimensions, debug_options)?;
        let t_eval_render_ms = crate::probe::probe_ms(t0);
        let frame = self.readback_output_with_alpha(dimensions, ReadbackAlpha::Opaque)?;
        if t0.is_some() {
            eprintln!(
                "[frame-timing] {}x{} evaluate+render={:.2}ms readback={:.2}ms",
                dimensions.width,
                dimensions.height,
                t_eval_render_ms,
                crate::probe::probe_ms(t0) - t_eval_render_ms,
            );
        }
        Ok(frame)
    }

    /// Render the authored canvas to export pixels, fitting it into the requested output.
    pub fn render_canvas(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        output: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<RenderedFrame, String> {
        let mapping = CanvasMapping::new(timeline.canvas_dimensions(), output);
        self.render_to_output_texture_inner(timeline, time_s, mapping, debug_options)?;
        self.readback_output_with_alpha(output, ReadbackAlpha::Opaque)
    }

    /// Queue a fitted export frame and its readback without waiting for the GPU.
    pub fn begin_canvas_frame(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        output: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<PendingFrame, String> {
        let mapping = CanvasMapping::new(timeline.canvas_dimensions(), output);
        self.render_to_output_texture_inner(timeline, time_s, mapping, debug_options)?;
        self.begin_readback_with_alpha(output, ReadbackAlpha::Opaque)
    }

    /// pipelined variant of [`Self::render_timeline_with_debug`]:
    /// evaluate, render, and queue the readback copy WITHOUT blocking. Pair
    /// with [`Self::wait_frame`], submitting the NEXT frame before waiting on
    /// this one, so the GPU renders while the CPU copies/encodes:
    ///
    /// ```text
    /// let mut pending = renderer.begin_frame_with_debug(&t, 0.0, dims, opts)?;
    /// for time in rest {
    ///     let next = renderer.begin_frame_with_debug(&t, time, dims, opts)?;
    ///     let frame = renderer.wait_frame(pending)?;   // overlaps with `time`'s GPU work
    ///     encode(frame);
    ///     pending = next;
    /// }
    /// let frame = renderer.wait_frame(pending)?;
    /// ```
    pub fn begin_frame_with_debug(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<PendingFrame, String> {
        self.render_to_output_texture(timeline, time_s, dimensions, debug_options)?;
        self.begin_readback_with_alpha(dimensions, ReadbackAlpha::Opaque)
    }

    /// Shared tail of `render_timeline_with_debug` / `begin_frame_with_debug`:
    /// evaluate and render the scene to the output texture without reading it back.
    fn render_to_output_texture(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<(), String> {
        let mapping = CanvasMapping::new(dimensions, dimensions);
        self.render_to_output_texture_inner(timeline, time_s, mapping, debug_options)
            .map(|_| ())
    }

    /// Render a frame and return both its pixels and the observable
    /// `animatix::timeline::SceneProgram` (per-actor `precise_bounds`, items,
    /// diagnostics) from
    /// the *same* evaluation.
    ///
    /// This is the content-level verification entry point used by
    /// `animatix audit`: the pixels answer "did it actually draw?", the
    /// bounds answer "where should it have drawn?". Calling the scene-only
    /// path plus a separate `evaluate_program_*` call would evaluate twice and
    /// could disagree; this keeps them consistent.
    pub fn render_timeline_observable(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<(RenderedFrame, animatix::timeline::scene_program::SceneProgram), String> {
        let mapping = CanvasMapping::new(dimensions, dimensions);
        let program =
            self.render_to_output_texture_inner(timeline, time_s, mapping, debug_options)?;
        let frame = self.readback_output_with_alpha(dimensions, ReadbackAlpha::Opaque)?;
        Ok((frame, program))
    }

    /// Render a fitted export frame and return the program in logical coordinates.
    pub fn render_canvas_observable(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        output: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<(RenderedFrame, animatix::timeline::scene_program::SceneProgram), String> {
        let mapping = CanvasMapping::new(timeline.canvas_dimensions(), output);
        let program =
            self.render_to_output_texture_inner(timeline, time_s, mapping, debug_options)?;
        let frame = self.readback_output_with_alpha(output, ReadbackAlpha::Opaque)?;
        Ok((frame, program))
    }

    /// Shared tail of [`Self::render_to_output_texture`] and
    /// [`Self::render_timeline_observable`]. Render the scene from the same
    /// evaluation that produced the observable program.
    fn render_to_output_texture_inner(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        mapping: CanvasMapping,
        debug_options: DebugRenderOptions,
    ) -> Result<animatix::timeline::scene_program::SceneProgram, String> {
        self.prepare_canvas(mapping, false)?;

        let t_eval = crate::probe::timing_probe();
        let mut recorder = crate::frame_gpu::SceneRecorder;
        let mut backend: Option<&mut dyn animatix::timeline::effects::FilterBackend> =
            Some(&mut recorder);
        let program = timeline.evaluate_program_with_debug(
            time_s,
            mapping.logical,
            debug_options,
            &mut backend,
        );
        super::timeline_render::check_video_diagnostics(&program)?;
        let probe_eval_ms = crate::probe::probe_ms(t_eval);

        let output_view = self.canvas_views(mapping)?.output.clone();

        self.core
            .frame
            .render_frame_into(&program.scene, &output_view, mapping.transform)?;
        self.present_canvas(mapping)?;
        if t_eval.is_some() {
            eprintln!(
                "[frame-phases] evaluate={:.2}ms main_render={:.2}ms",
                probe_eval_ms,
                crate::probe::probe_ms(t_eval) - probe_eval_ms,
            );
        }

        Ok(program)
    }

    /// Copy the output texture into the rotating readback buffer, submit, and
    /// return the in-flight frame handle. Does NOT block — pair with
    /// [`Self::wait_frame`] (or call [`Self::readback_output`] for the
    /// blocking one-shot).
    pub fn begin_readback(&mut self, dimensions: SceneDimensions) -> Result<PendingFrame, String> {
        self.begin_readback_with_alpha(dimensions, ReadbackAlpha::MayBePremultiplied)
    }

    fn begin_readback_with_alpha(
        &mut self,
        dimensions: SceneDimensions,
        alpha: ReadbackAlpha,
    ) -> Result<PendingFrame, String> {
        let output_texture = self
            .output_texture
            .as_ref()
            .ok_or_else(|| "Missing offscreen output texture".to_string())?;
        let slot = self.readback_slot;
        let output_buffer = self.readback_buffers[slot]
            .as_ref()
            .ok_or_else(|| "Missing offscreen readback buffer".to_string())?;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Offscreen Readback Encoder"),
        });

        // Single-scene frames clear to opaque black and accumulate with SrcOver.
        // Their premultiplied colors are already valid straight-alpha pixels.
        let output_texture = if matches!(alpha, ReadbackAlpha::MayBePremultiplied) {
            let source = self
                .output_view
                .as_ref()
                .ok_or_else(|| "Missing offscreen output view".to_string())?;
            let target = self
                .readback_view_gpu
                .as_ref()
                .ok_or_else(|| "Missing straight-alpha readback view".to_string())?;
            self.core.frame.blit(source, target, wgpu::TextureFormat::Rgba8Unorm, true);
            self.readback_texture_gpu
                .as_ref()
                .ok_or_else(|| "Missing straight-alpha readback texture".to_string())?
        } else {
            output_texture
        };

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

        let submission = self.queue.submit(std::iter::once(encoder.finish()));
        self.readback_slot = (slot + 1) % self.readback_buffers.len();

        Ok(PendingFrame {
            buffer: output_buffer.clone(),
            submission,
            dims: dimensions,
            bytes_per_row: self.bytes_per_row,
        })
    }

    /// Block until `pending`'s readback copy completed and copy the pixels
    /// into CPU memory. Waits only for THIS frame's submission — work queued
    /// after it (e.g. the next `begin_frame`) keeps running on the GPU.
    pub fn wait_frame(&mut self, pending: PendingFrame) -> Result<RenderedFrame, String> {
        let dimensions = pending.dims;
        let buffer_slice = pending.buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            tx.send(result).ok();
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(pending.submission),
                timeout: None,
            })
            .map_err(|err| format!("Failed to poll GPU device: {err}"))?;
        rx.recv()
            .map_err(|err| format!("Failed to receive mapped frame: {err}"))?
            .map_err(|err| format!("Failed to map preview frame: {err}"))?;

        let data = buffer_slice
            .get_mapped_range()
            .map_err(|err| format!("Failed to map preview frame: {err}"))?;
        // Reuse the previous frame's readback buffer when the
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
                let src_row = &data[y * pending.bytes_per_row as usize
                    ..y * pending.bytes_per_row as usize + dimensions.width as usize * 4];
                let dst_row = &mut rgba_mut
                    [y * dimensions.width as usize * 4..(y + 1) * dimensions.width as usize * 4];
                dst_row.copy_from_slice(src_row);
            }
        }
        drop(data);
        pending.buffer.unmap();

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

    /// Blocking one-shot readback (queue + wait).
    pub fn readback_output(
        &mut self,
        dimensions: SceneDimensions,
    ) -> Result<RenderedFrame, String> {
        self.readback_output_with_alpha(dimensions, ReadbackAlpha::MayBePremultiplied)
    }

    fn readback_output_with_alpha(
        &mut self,
        dimensions: SceneDimensions,
        alpha: ReadbackAlpha,
    ) -> Result<RenderedFrame, String> {
        let pending = self.begin_readback_with_alpha(dimensions, alpha)?;
        self.wait_frame(pending)
    }

    pub(super) fn ensure_targets(&mut self, dimensions: SceneDimensions) {
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
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            label: Some("Animatix Offscreen Output Texture"),
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.output_texture = Some(output_texture);
        self.output_view = Some(output_view);

        {
            let readback_texture = self.device.create_texture(&wgpu::TextureDescriptor {
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
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                label: Some("Animatix Straight Alpha Readback Texture"),
                view_formats: &[],
            });
            let readback_view_gpu =
                readback_texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.readback_texture_gpu = Some(readback_texture);
            self.readback_view_gpu = Some(readback_view_gpu);
        }

        // Two rotating MAP_READ buffers — frame N's pixels are read
        // (mapped) from slot A while frame N+1's copy lands in slot B.
        for (i, slot) in self.readback_buffers.iter_mut().enumerate() {
            *slot = Some(self.device.create_buffer(&wgpu::BufferDescriptor {
                size: (self.bytes_per_row * dimensions.height) as wgpu::BufferAddress,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                label: Some(&format!("Animatix Offscreen Readback Buffer {i}")),
                mapped_at_creation: false,
            }));
        }

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
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
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
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING,
            label: Some("Animatix Offscreen Texture B"),
            view_formats: &[],
        });
        let view_b = texture_b.create_view(&wgpu::TextureViewDescriptor::default());
        self.texture_b = Some(texture_b);
        self.view_b = Some(view_b);
    }
}
