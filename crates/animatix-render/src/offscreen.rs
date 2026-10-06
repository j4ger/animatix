use super::core::RendererCore;
use super::filter_backend::GpuFilterBackend;
use super::transition::TransitionCompositor;
use animatix::timeline::effects::FilterBackend;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};

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
    /// Two rotating MAP_READ buffers for the pipelined readback (PF-7): while
    /// frame N's copy is mapped/encoded, frame N+1's copy lands in the other
    /// buffer — one-frame GPU/CPU overlap without corrupting a mapped buffer.
    readback_buffers: [Option<wgpu::Buffer>; 2],
    texture_a: Option<wgpu::Texture>,
    view_a: Option<wgpu::TextureView>,
    texture_b: Option<wgpu::Texture>,
    view_b: Option<wgpu::TextureView>,
    compositor: Option<TransitionCompositor>,
    /// Cached GPU filter backend — recreated only when dimensions change.
    /// Serves the single-scene path and a transition's outgoing scene; the
    /// incoming scene gets [`Self::filter_backend_b`] so the two evaluations
    /// cannot stomp each other's pass state.
    filter_backend: Option<GpuFilterBackend>,
    /// Second backend for a transition's incoming scene. Lazily created like
    /// [`Self::filter_backend`]; both reset together on a dimension change.
    filter_backend_b: Option<GpuFilterBackend>,
    filter_backend_dimensions: Option<SceneDimensions>,
    dimensions: SceneDimensions,
    bytes_per_row: u32,
    /// Recycled CPU readback buffer (PF-6 round 9): parked between frames and
    /// handed out again once the encoder drops its reference — the 720p RGBA
    /// readback was 3.7 MB allocated per frame (93% of export-path churn).
    readback_buffer: Option<std::sync::Arc<Vec<u8>>>,
    /// Next `readback_buffers` slot for `queue_readback`.
    readback_slot: usize,
}

/// A frame whose GPU readback copy has been queued but not yet waited on.
///
/// Plain data — the renderer stays free between `begin_frame` and
/// `wait_frame`, which is what makes the one-frame overlap possible
/// (`begin(t1)` before `wait(t0)` keeps the GPU busy while the CPU handles
/// frame t0's pixels).
#[derive(Debug)]
pub struct PendingFrame {
    buffer: wgpu::Buffer,
    submission: wgpu::SubmissionIndex,
    dims: SceneDimensions,
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

    /// Pipelined variant of [`Self::render_transition`] (see
    /// [`Self::begin_frame_with_debug`]).
    pub fn begin_transition(
        &mut self,
        from_timeline: &Timeline,
        from_time: f64,
        to_timeline: &Timeline,
        to_time: f64,
        progress: f32,
        transition_id: String,
        easing: animatix::easing::Easing,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<PendingFrame, String> {
        self.render_transition_to_output(
            from_timeline,
            from_time,
            to_timeline,
            to_time,
            progress,
            transition_id,
            easing,
            dimensions,
            debug_options,
        )?;
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
            )?;
        }

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
        easing: animatix::easing::Easing,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<RenderedFrame, String> {
        self.render_transition_to_output(
            from_timeline,
            from_time,
            to_timeline,
            to_time,
            progress,
            transition_id,
            easing,
            dimensions,
            debug_options,
        )?;
        self.readback_output(dimensions)
    }

    /// Shared tail of `render_transition` / `begin_transition`: render both
    /// scenes to the intermediate textures, composite onto the output texture.
    /// Leaves the GPU work queued (no readback).
    #[allow(clippy::too_many_arguments)]
    fn render_transition_to_output(
        &mut self,
        from_timeline: &Timeline,
        from_time: f64,
        to_timeline: &Timeline,
        to_time: f64,
        progress: f32,
        transition_id: String,
        easing: animatix::easing::Easing,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<(), String> {
        if dimensions.width == 0 || dimensions.height == 0 {
            return Err("Preview dimensions must be greater than zero".to_string());
        }

        self.ensure_targets(dimensions);

        // The two scenes share one dimension key: a resize drops both backends
        // and each is lazily rebuilt in `render_timeline_into_view`.
        if self.filter_backend_dimensions != Some(dimensions) {
            self.filter_backend = None;
            self.filter_backend_b = None;
            self.filter_backend_dimensions = Some(dimensions);
        }

        // Lazy-init compositor
        if self.compositor.is_none() {
            self.compositor =
                Some(TransitionCompositor::new(&self.device).map_err(|e| e.to_string())?);
        }
        let compositor =
            self.compositor.as_ref().ok_or_else(|| "Missing compositor".to_string())?;

        // Render the outgoing scene to texture_a, then drop scene_a before
        // creating scene_b to avoid holding both large vello::Scene objects
        // simultaneously. Each scene keeps its own filter backend, so a
        // `Filter` scope shows up in the transition frame exactly as it does
        // in the single-scene path (its pending composites are blitted onto
        // the scene's own texture before the blend reads it).
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
                from_timeline,
                from_time,
                dimensions,
                debug_options,
                filter_backend,
                view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?,
                texture_a.as_ref(),
            )?;
        }

        // Render to scene to texture_b
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
                to_timeline,
                to_time,
                dimensions,
                debug_options,
                filter_backend_b,
                view_b.as_ref().ok_or_else(|| "Missing offscreen view_b".to_string())?,
                texture_b.as_ref(),
            )?;
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

        Ok(())
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

/// Evaluate one frame of `timeline` with `backend` (lazily created) and draw
/// the result — the encoded scene *plus* the scope's pending filter
/// composites — into `view`.
///
/// This is the per-target tail of the single-scene path
/// (`render_to_output_texture_inner`), shared with the transition path so
/// both scenes of a blend keep their `Filter` scopes. Free-standing so a
/// call site can borrow the core and one backend slot off `self` disjointly.
/// Composite every queued layer onto `view`, in queue order.
///
/// A `Backdrop` layer is served here rather than during evaluation: its input is
/// the frame that has just been rendered into `target`, and that frame did not
/// exist while the scene was being built. Order is what keeps a `Glass` panel's
/// children above its blur — the queue is append-only, so the depth the walk
/// produced survives the split into separate renders.
pub fn drain_pending_layers(
    core: &RendererCore,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    backend: &mut dyn animatix::timeline::effects::FilterBackend,
    layers: Vec<animatix::timeline::effects::PendingLayer>,
    target: Option<&wgpu::Texture>,
    view: &wgpu::TextureView,
    scale: f32,
) {
    use animatix::timeline::effects::{EffectRegion, PendingLayer};
    let scale_region = |r: EffectRegion, s: f32| EffectRegion {
        origin: [r.origin[0] * s, r.origin[1] * s],
        size: SceneDimensions {
            width: ((r.size.width as f32 * s).round() as u32).max(1),
            height: ((r.size.height as f32 * s).round() as u32).max(1),
        },
    };
    for layer in layers {
        // `s` is the factor still to apply when the composite is blitted. A
        // backdrop is scaled *before* its pass runs — the region is read out of
        // the target at the target's own resolution — so its result already
        // speaks target pixels and must not be scaled twice.
        let (composite, layer_scale) = match layer {
            PendingLayer::Composite(composite) => (composite, scale),
            PendingLayer::Backdrop(backdrop) => {
                let Some(target) = target else {
                    tracing::warn!("backdrop queued with no render target to read");
                    continue;
                };
                let backdrop = if (scale - 1.0).abs() > f32::EPSILON {
                    animatix::timeline::effects::PendingBackdrop {
                        region: scale_region(backdrop.region, scale),
                        clip: scale_region(backdrop.clip, scale),
                        corner_radius: backdrop.corner_radius * scale,
                        ..backdrop
                    }
                } else {
                    backdrop
                };
                match backend.run_backdrop(target, &backdrop) {
                    Ok(mut composite) => {
                        // The rects handed to `run_backdrop` were already put in
                        // target pixels above, so whatever it produced is in
                        // target space and needs no factor — including the
                        // `texel_scale` the backend stamped from its own raster.
                        composite.texel_scale = 1.0;
                        (composite, 1.0)
                    },
                    Err(e) => {
                        tracing::warn!("backdrop pass failed: {e}");
                        continue;
                    },
                }
            },
        };
        // `texel_scale` is how many texture pixels stand for one scene pixel. A
        // chain that was told the raster scale filtered at that scale, so its
        // result is already in target pixels and the frame scale must not be
        // applied a second time; `layer_scale` carries what the layer needs
        // otherwise.
        let s = layer_scale / composite.texel_scale.max(f32::EPSILON);
        let tex = composite.texture.size();
        let (origin, size) = match composite.clip_rect {
            Some([x0, y0, x1, y1]) => (
                [x0 * s, y0 * s],
                [
                    (((x1 - x0) * s).round() as u32).max(1),
                    (((y1 - y0) * s).round() as u32).max(1),
                ],
            ),
            None => (
                [composite.origin[0] * s, composite.origin[1] * s],
                [
                    ((tex.width as f32 * s).round() as u32).max(1),
                    ((tex.height as f32 * s).round() as u32).max(1),
                ],
            ),
        };
        let src_rect = composite.src_rect.map(|[x0, y0, x1, y1]| [x0 * s, y0 * s, x1 * s, y1 * s]);
        let radius = composite.corner_radius * s;
        core.blit_texture_rect_masked(
            device,
            queue,
            &composite.view,
            view,
            origin,
            size,
            composite.alpha,
            (radius > 0.0).then_some(radius),
            src_rect,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_timeline_into_view(
    core: &mut RendererCore,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    timeline: &Timeline,
    time_s: f64,
    dimensions: SceneDimensions,
    debug_options: DebugRenderOptions,
    backend: &mut Option<GpuFilterBackend>,
    view: &wgpu::TextureView,
    backdrop_target: Option<&wgpu::Texture>,
) -> Result<(), String> {
    if backend.is_none() {
        *backend = Some(GpuFilterBackend::new(device.clone(), queue.clone(), dimensions)?);
    }
    let mut fb: Option<&mut dyn animatix::timeline::effects::FilterBackend> =
        backend.as_mut().map(|b| b as _);
    let scene = timeline
        .evaluate_program_with_debug(time_s, dimensions, debug_options, &mut fb)
        .scene;

    core.render_vello_scene(device, queue, view, dimensions.width, dimensions.height, &scene)
        .map_err(|e| e.to_string())?;

    let pending = backend.as_mut().map(|fb| fb.take_pending_composites()).unwrap_or_default();
    if let (Some(backend), Some(target)) = (backend.as_mut(), backdrop_target) {
        drain_pending_layers(core, device, queue, backend, pending, Some(target), view, 1.0);
    }
    Ok(())
}

#[cfg(test)]
mod readback_reuse_tests {
    use super::*;

    /// Fixture built through the source pipeline so these tests exercise only
    /// public engine API (the render crate has no access to engine internals).
    fn solid_rect_timeline() -> Timeline {
        let source = r#"
#0s
r: Rect, at: (50, 50), size: (100, 100), color: (1, 1, 1, 1)
"#;
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("AST");
        let report = animatix::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
        report.output
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
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
        };
        let mut fresh = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
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
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
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

    /// PF-7: the pipelined begin/wait pair must produce pixels identical to
    /// the blocking path, across the rotating buffer pair (frame A in slot 0,
    /// frame B in slot 1, frame C back in slot 0 after A was unmapped).
    #[test]
    fn pipelined_frames_match_blocking_path() {
        let mut pipelined = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
        };
        let mut blocking = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
        };
        let timeline = solid_rect_timeline();
        let dims = SceneDimensions {
            width: 200,
            height: 160,
        };

        // Three in-flight frames exercise the slot rotation.
        let pending_a = pipelined
            .begin_frame_with_debug(&timeline, 0.0, dims, DebugRenderOptions::default())
            .expect("begin a");
        let pending_b = pipelined
            .begin_frame_with_debug(&timeline, 0.0, dims, DebugRenderOptions::default())
            .expect("begin b");
        let frame_a = pipelined.wait_frame(pending_a).expect("wait a");
        let pending_c = pipelined
            .begin_frame_with_debug(&timeline, 0.0, dims, DebugRenderOptions::default())
            .expect("begin c");
        let frame_b = pipelined.wait_frame(pending_b).expect("wait b");
        let frame_c = pipelined.wait_frame(pending_c).expect("wait c");

        let reference = blocking.render_timeline(&timeline, 0.0, dims).expect("blocking");
        for (name, frame) in [("a", &frame_a), ("b", &frame_b), ("c", &frame_c)] {
            assert_eq!(frame.rgba.as_ref(), reference.rgba.as_ref(), "frame {name} diverged");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
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

    /// Derived region of interest: a `Filter` with no authored `bounds` must
    /// still keep its content — the region is derived from the content bounds
    /// recorded during the sub-scene evaluation and padded by the chain's
    /// support. (Regression guard for the derived-ROI crop: a bug here would
    /// clip or blank the filtered content.)
    #[test]
    fn derived_effect_region_preserves_content() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
        };

        let source = r#"
config { resolution: (400, 300) }

#0s
fx: Filter {
  soft: Blur, radius: 4
  box: Rect, size: (120, 120), color: (1, 1, 1, 1), at: (0, 0)
}
"#;
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let ast = ast.expect("AST");
        let report = animatix::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        let timeline = report.output;

        let frame = renderer
            .render_timeline(
                &timeline,
                0.0,
                animatix::timeline::SceneDimensions {
                    width: 400,
                    height: 300,
                },
            )
            .expect("render");

        let px = |x: usize, y: usize| -> [u8; 4] {
            let base = (y * frame.width as usize + x) * 4;
            [
                frame.rgba[base],
                frame.rgba[base + 1],
                frame.rgba[base + 2],
                frame.rgba[base + 3],
            ]
        };

        // The offscreen target composites over an opaque black backdrop, so
        // assertions run on luminance. Rect centre: white.
        let center = px(200, 150);
        assert!(center[0] > 200, "rect centre must stay white: {center:?}");
        // On the rect edge column: blur mixes content with the backdrop
        // (premultiplied alpha softens twice), so expect mid luminance.
        let spill = px(260, 150);
        assert!(
            spill[0] > 20 && spill[0] < 200,
            "expected blurred edge luminance, got {spill:?}"
        );
        // Well outside the content + support: untouched backdrop.
        let far = px(60, 40);
        assert!(
            far[0] + far[1] + far[2] < 20,
            "content must not leak outside the derived region: {far:?}"
        );
    }

    /// Authored `bounds:` on a Filter scopes effect processing and composites
    /// the result back at the region origin.
    #[test]
    fn authored_effect_bounds_region_renders_at_origin() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
        };

        let source = r#"
config { resolution: (400, 300) }

#0s
fx: Filter, bounds: (100, 60, 200, 180) {
  soft: Blur, radius: 4
  box: Rect, size: (120, 120), color: (1, 1, 1, 1), at: (0, 0)
}
"#;
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let ast = ast.expect("AST");
        let report = animatix::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        let timeline = report.output;

        let frame = renderer
            .render_timeline(
                &timeline,
                0.0,
                animatix::timeline::SceneDimensions {
                    width: 400,
                    height: 300,
                },
            )
            .expect("render");

        let px = |x: usize, y: usize| -> [u8; 4] {
            let base = (y * frame.width as usize + x) * 4;
            [
                frame.rgba[base],
                frame.rgba[base + 1],
                frame.rgba[base + 2],
                frame.rgba[base + 3],
            ]
        };

        // The rect (140..260, 90..210) sits inside the authored region
        // (100..300, 60..240). The backdrop is opaque black: check luminance.
        let center = px(200, 150);
        assert!(center[0] > 200, "rect centre must stay white: {center:?}");
        let far = px(40, 30);
        assert!(
            far[0] + far[1] + far[2] < 20,
            "content must not leak outside the authored region: {far:?}"
        );
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
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
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
        let report = animatix::timeline::Timeline::build_with_diagnostics(
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
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
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
        let report = animatix::timeline::Timeline::build_with_diagnostics(
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

    /// A non-Rect/Ellipse `clip_shape` (Polygon here) must define the actual
    /// clip geometry. Before the `Primitive::clip_path` capability this
    /// silently fell back to a rectangular clip, so a diamond mask rendered as
    /// a rectangle. The clip child must also not paint itself.
    #[test]
    fn mask_clip_shape_polygon_defines_clip_region() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
        };

        let source = r#"
config { resolution: (400, 300) }

m: Mask, size: (100, 75), at: (200, 150) {
  clip_shape: Polygon, points: {(0, -50), (50, 0), (0, 50), (-50, 0)}, color: accent.success
  big: Rect, size: (400, 300), color: (1, 0, 0, 1)
}

#0s
fade-in m [1ms]
"#;
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let ast = ast.expect("AST");
        let report = animatix::timeline::Timeline::build_with_diagnostics(
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

        // Diamond center → red child visible.
        assert!(is_red(200, 150), "diamond center should show the red child");
        // Inside the mask box but outside the diamond (|dx| + |dy| > 50):
        // clipped away — a rectangular fallback would show red here.
        assert!(!is_red(235, 178), "outside the diamond must stay background");
        assert!(!is_red(60, 150), "far outside the mask must stay background");
    }

    /// A `clip_shape` whose primitive has no clip geometry warns instead of
    /// silently clipping with a rectangle.
    #[test]
    fn mask_clip_shape_without_geometry_warns() {
        let source = r#"
config { resolution: (400, 300) }

m: Mask, size: (100, 75), at: (200, 150) {
  clip_shape: Text, text: "clip"
  big: Rect, size: (400, 300), color: (1, 0, 0, 1)
}

#0s
fade-in m [1ms]
"#;
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let ast = ast.expect("AST");
        let report = animatix::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        let timeline = report.output;

        let _ = timeline.evaluate(
            0.5,
            SceneDimensions {
                width: 400,
                height: 300,
            },
        );
        let warnings = timeline.runtime_diagnostics();
        assert!(
            warnings.iter().any(|d| d.message.contains("provides no clip geometry")),
            "expected a clip-geometry warning, got {:?}",
            warnings.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
        );
    }

    /// Plot geometry is not a clip provider (it is time-varying and
    /// `stroke_progress`-trimmed), so a plot `clip_shape` warns and falls back
    /// to a rectangular clip instead of clipping to a moving partial curve.
    #[test]
    fn mask_clip_shape_plot_falls_back_with_warning() {
        let source = r#"
config { resolution: (400, 300) }

m: Mask, size: (100, 75), at: (200, 150) {
  clip_shape: PlotCurve, kind: "cartesian", func: (x) => x
  big: Rect, size: (400, 300), color: (1, 0, 0, 1)
}

#0s
fade-in m [1ms]
"#;
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let ast = ast.expect("AST");
        let report = animatix::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        let timeline = report.output;

        let _ = timeline.evaluate(
            0.5,
            SceneDimensions {
                width: 400,
                height: 300,
            },
        );
        let warnings = timeline.runtime_diagnostics();
        assert!(
            warnings.iter().any(|d| d.message.contains("provides no clip geometry")),
            "expected a clip-geometry warning for a plot clip_shape, got {:?}",
            warnings.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn hosted_bar_chart_paints_bars_across_the_full_graph_axis() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
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
        let report = animatix::timeline::Timeline::build_with_diagnostics(
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
    fn offscreen_renderer_ensure_targets_is_idempotent() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
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

    /// Regression (effects-wave1 dogfood): a scope whose chain runs under a
    /// *derived* region used to composite stale ping-pong texels from the
    /// previous scope into the frame (an opaque garbage rectangle around the
    /// moving content, found with a `MotionBlur` card flying over a backdrop).
    /// The backdrop must survive intact inside another scope's region.
    #[test]
    fn region_composite_does_not_leak_stale_pixels() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
        };

        let source = r#"
config { resolution: (640, 360) }

sheet: Rect, size: (640, 360), color: (1, 1, 1, 1), anchor: scene.center

card: Filter, anchor: scene.center, offset: (-150, 0) {
  swipe: MotionBlur, length: 20, angle: 0
  plate: Rect, size: (140, 90), color: (1, 0.2, 0.2, 1)
}

#0s
fade-in sheet [100ms]
fade-in card [100ms]

#0.5s
card.offset = (0, 0) [1s, ease: linear]
card.swipe.length = 20 [0.5s, ease: linear]
"#;
        let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
        assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
        let ast = ast.expect("AST");
        let report = animatix::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);

        // Mid-flight: the card centre is at (245, 180) (offset halfway from
        // -150 to 0) and its derived region spans (155, 115)-(335, 245).
        let frame = renderer
            .render_timeline(
                &report.output,
                1.0,
                animatix::timeline::SceneDimensions {
                    width: 640,
                    height: 360,
                },
            )
            .expect("render");

        let px = |x: usize, y: usize| -> [u8; 4] {
            let base = (y * frame.width as usize + x) * 4;
            [
                frame.rgba[base],
                frame.rgba[base + 1],
                frame.rgba[base + 2],
                frame.rgba[base + 3],
            ]
        };

        // Inside the card's region but outside the card and its smear: the
        // white backdrop must survive the region composite untouched. The
        // leak pasted opaque garbage here.
        let backdrop_inside_region = px(165, 125);
        assert!(
            backdrop_inside_region[0] > 200,
            "backdrop inside another scope's region must not be overwritten: {backdrop_inside_region:?}"
        );

        // The card plate itself stays red at its mid-flight position.
        let plate = px(245, 180);
        assert!(
            plate[0] > 200 && plate[1] < 120,
            "moving plate must render red at its mid-flight position: {plate:?}"
        );
    }
}
