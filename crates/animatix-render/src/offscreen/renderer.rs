use super::*;

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
            readback_buffers: [None, None],
            texture_a: None,
            view_a: None,
            texture_b: None,
            view_b: None,
            compositor: None,
            filter_backend: None,
            filter_backend_b: None,
            filter_backend_dimensions: None,
            dimensions: SceneDimensions {
                width: 0,
                height: 0,
            },
            bytes_per_row: 0,
            readback_buffer: None,
            readback_slot: 0,
        })
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
        let t0 = crate::filter_backend::timing_probe();
        self.render_to_output_texture(timeline, time_s, dimensions, debug_options)?;
        let t_eval_render_ms = crate::filter_backend::probe_ms(t0);
        let frame = self.readback_output(dimensions)?;
        if t0.is_some() {
            eprintln!(
                "[frame-timing] {}x{} evaluate+render={:.2}ms readback={:.2}ms",
                dimensions.width,
                dimensions.height,
                t_eval_render_ms,
                crate::filter_backend::probe_ms(t0) - t_eval_render_ms,
            );
        }
        Ok(frame)
    }

    /// PF-7 pipelined variant of [`Self::render_timeline_with_debug`]:
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
        self.begin_readback(dimensions)
    }

    /// Shared tail of `render_timeline_with_debug` / `begin_frame_with_debug`:
    /// evaluate, render the scene, and blit pending filter composites onto
    /// the output texture. Leaves the GPU work queued (no readback).
    fn render_to_output_texture(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<(), String> {
        self.render_to_output_texture_inner(timeline, time_s, dimensions, debug_options, false)
            .map(|_| ())
    }

    /// Render a frame and return both its pixels and the observable
    /// `animatix::timeline::SceneProgram` (per-actor `precise_bounds`, items,
    /// diagnostics) from
    /// the *same* evaluation.
    ///
    /// This is the content-level verification entry point (see
    /// [`animatix::verify`]): the pixels answer "did it actually draw?", the
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
        let program = self
            .render_to_output_texture_inner(timeline, time_s, dimensions, debug_options, true)?
            .expect("collect_items=true always returns a program");
        let frame = self.readback_output(dimensions)?;
        Ok((frame, program))
    }

    /// Shared tail of [`Self::render_to_output_texture`] and
    /// [`Self::render_timeline_observable`]. `collect_items` selects the
    /// observable evaluation path and makes the program available to the
    /// caller.
    fn render_to_output_texture_inner(
        &mut self,
        timeline: &Timeline,
        time_s: f64,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        _collect_items: bool,
    ) -> Result<Option<animatix::timeline::scene_program::SceneProgram>, String> {
        // `_collect_items` is vestigial: the zero-readback evaluate path always
        // produces the observable program (the pending-filter blits below
        // require it), so both callers receive identical behavior.
        if dimensions.width == 0 || dimensions.height == 0 {
            return Err("Preview dimensions must be greater than zero".to_string());
        }

        self.ensure_targets(dimensions);

        // Evaluate timeline with filter backend support.
        // Reuse cached backend if dimensions match, otherwise recreate. The
        // transition path's second backend shares this dimension key, so a
        // resize drops both and each is lazily rebuilt where it is used.
        if self.filter_backend_dimensions != Some(dimensions) {
            self.filter_backend = None;
            self.filter_backend_b = None;
            self.filter_backend =
                Some(GpuFilterBackend::new(self.device.clone(), self.queue.clone(), dimensions)?);
            self.filter_backend_dimensions = Some(dimensions);
        }
        let filter_backend = self.filter_backend.as_mut().unwrap();
        let mut fb: Option<&mut dyn animatix::timeline::effects::FilterBackend> =
            Some(filter_backend);
        let t_eval = crate::filter_backend::timing_probe();
        // Always take the zero-readback (pending) evaluate path: scopes record
        // GPU composites that are blitted after the main render (below), so
        // per-scope GPU->CPU readback stalls never enter the frame.
        let program =
            Some(timeline.evaluate_program_with_debug(time_s, dimensions, debug_options, &mut fb));
        // The observable path borrows the scene out of `program`; the
        // scene-only path evaluates directly. Either way `scene` is a borrow
        // that ends before the render call returns.
        let scene_owned;
        let scene: &vello::Scene = match program.as_ref() {
            Some(program) => &program.scene,
            None => {
                scene_owned =
                    timeline.evaluate_with_debug(time_s, dimensions, debug_options, &mut fb);
                &scene_owned
            },
        };
        let probe_eval_ms = crate::filter_backend::probe_ms(t_eval);

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
                scene,
            )
            .map_err(|e| e.to_string())?;
        if t_eval.is_some() {
            eprintln!(
                "[frame-phases] evaluate={:.2}ms main_render={:.2}ms",
                probe_eval_ms,
                crate::filter_backend::probe_ms(t_eval) - probe_eval_ms,
            );
        }

        // Blit pending zero-readback filter composites on top of the rendered scene
        let pending = self
            .filter_backend
            .as_mut()
            .map(|fb| fb.take_pending_composites())
            .unwrap_or_default();

        if let Some(backend) = self.filter_backend.as_mut() {
            let output_texture = self.output_texture.as_ref();
            drain_pending_layers(
                &self.core,
                &self.device,
                &self.queue,
                backend,
                pending,
                output_texture,
                output_view,
                1.0,
            );
        }
        Ok(program)
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

        {
            let Self {
                core,
                device,
                queue,
                filter_backend,
                view_a,
                texture_a,
                ..
            } = self;
            render_timeline_into_view(
                core,
                device,
                queue,
                timeline,
                time_s,
                dimensions,
                debug_options,
                filter_backend,
                view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?,
                texture_a.as_ref(),
                &[],
            )?;
        }

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

        {
            let Self {
                core,
                device,
                queue,
                filter_backend_b,
                view_b,
                texture_b,
                ..
            } = self;
            render_timeline_into_view(
                core,
                device,
                queue,
                timeline,
                time_s,
                dimensions,
                debug_options,
                filter_backend_b,
                view_b.as_ref().ok_or_else(|| "Missing offscreen view_b".to_string())?,
                texture_b.as_ref(),
                &[],
            )?;
        }

        self.texture_b.as_ref().ok_or_else(|| "Missing offscreen texture_b".to_string())
    }

    /// Copy the output texture into the rotating readback buffer, submit, and
    /// return the in-flight frame handle. Does NOT block — pair with
    /// [`Self::wait_frame`] (or call [`Self::readback_output`] for the
    /// blocking one-shot).
    pub fn begin_readback(&mut self, dimensions: SceneDimensions) -> Result<PendingFrame, String> {
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
        // Composite diagnostics: report how much of the composed output is
        // opaque when `ANIMATIX_DUMP_STAGES` is set, so a regressed composite
        // is visible in the log without re-running under a GPU capture.
        if std::env::var_os("ANIMATIX_DUMP_STAGES").is_some() {
            if let Some(output_texture) = self.output_texture.as_ref() {
                if let Some(fb) = self.filter_backend.as_mut() {
                    let img =
                        fb.debug_readback_texture(output_texture, wgpu::Origin3d::ZERO, dimensions);
                    let data = img.data.data.data();
                    let n = data.len() / 4;
                    let step = (n / 300).max(1);
                    let opaque = (0..n).step_by(step).filter(|&i| data[i * 4 + 3] > 100).count();
                    eprintln!("[dump] output_view opaque={opaque}/{}", n / step);
                }
            }
        }
        let pending = self.begin_readback(dimensions)?;
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
                | wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING,
            label: Some("Animatix Offscreen Output Texture"),
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.output_texture = Some(output_texture);
        self.output_view = Some(output_view);

        // PF-7: two rotating MAP_READ buffers — frame N's pixels are read
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
