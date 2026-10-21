use animatix::timeline::effects::FilterBackend;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};

use super::core::RendererCore;
use super::filter_backend::GpuFilterBackend;
use super::transition::TransitionCompositor;

#[cfg(test)]
mod tests;
/// A single frame rendered to CPU-accessible RGBA memory.
///
/// The pixel buffer is shared (`Arc`): the renderer keeps its own handle to
/// the previous frame's allocation and reuses it in place on the next
/// readback (`Arc::make_mut` is a no-op clone once the encoder drops its
/// reference), so a steady-state video export performs zero per-frame
/// allocations for the multi-megabyte readback (PF-6 round 9: dhat measured
/// 3.7 MB/frame on a 720p export — 93% of the export path's churn).
mod transitions;

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
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) core: RendererCore,
    pub(super) output_texture: Option<wgpu::Texture>,
    pub(super) output_view: Option<wgpu::TextureView>,
    /// Two rotating MAP_READ buffers for the pipelined readback (PF-7): while
    /// frame N's copy is mapped/encoded, frame N+1's copy lands in the other
    /// buffer — one-frame GPU/CPU overlap without corrupting a mapped buffer.
    pub(super) readback_buffers: [Option<wgpu::Buffer>; 2],
    pub(super) texture_a: Option<wgpu::Texture>,
    pub(super) view_a: Option<wgpu::TextureView>,
    pub(super) texture_b: Option<wgpu::Texture>,
    pub(super) view_b: Option<wgpu::TextureView>,
    pub(super) compositor: Option<TransitionCompositor>,
    /// Cached GPU filter backend — recreated only when dimensions change.
    /// Serves the single-scene path and a transition's outgoing scene; the
    /// incoming scene gets [`Self::filter_backend_b`] so the two evaluations
    /// cannot stomp each other's pass state.
    pub(super) filter_backend: Option<GpuFilterBackend>,
    /// Second backend for a transition's incoming scene. Lazily created like
    /// [`Self::filter_backend`]; both reset together on a dimension change.
    pub(super) filter_backend_b: Option<GpuFilterBackend>,
    pub(super) filter_backend_dimensions: Option<SceneDimensions>,
    pub(super) dimensions: SceneDimensions,
    pub(super) bytes_per_row: u32,
    /// Recycled CPU readback buffer (PF-6 round 9): parked between frames and
    /// handed out again once the encoder drops its reference — the 720p RGBA
    /// readback was 3.7 MB allocated per frame (93% of export-path churn).
    pub(super) readback_buffer: Option<std::sync::Arc<Vec<u8>>>,
    /// Next `readback_buffers` slot for `queue_readback`.
    pub(super) readback_slot: usize,
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
    use animatix::timeline::effects::{EffectRegion, PendingComposite, PendingLayer};
    let scale_region = |r: EffectRegion, s: f32| EffectRegion {
        origin: [r.origin[0] * s, r.origin[1] * s],
        size: SceneDimensions {
            width: ((r.size.width as f32 * s).round() as u32).max(1),
            height: ((r.size.height as f32 * s).round() as u32).max(1),
        },
    };

    struct PreparedComposite {
        composite: PendingComposite,
        origin: [f32; 2],
        size: [u32; 2],
        corner_radius: Option<f32>,
        src_rect: Option<[f32; 4]>,
    }

    let mut batch: Vec<PreparedComposite> = Vec::new();
    let flush_batch =
        |batch: &mut Vec<PreparedComposite>,
         backend: &mut dyn animatix::timeline::effects::FilterBackend| {
            if batch.is_empty() {
                return;
            }
            let items: Vec<crate::fullscreen_blit::BatchedBlitItem<'_>> = batch
                .iter()
                .map(|p| crate::fullscreen_blit::BatchedBlitItem {
                    src_view: &p.composite.view,
                    dst_origin: p.origin,
                    dst_size: p.size,
                    alpha: p.composite.alpha,
                    corner_radius: p.corner_radius,
                    src_rect: p.src_rect,
                })
                .collect();
            core.blit_texture_rect_masked_batch(device, queue, view, &items);
            for item in batch.drain(..) {
                backend.recycle_pending_composite(item.composite);
            }
        };

    for layer in layers {
        // `s` is the factor still to apply when the composite is blitted. A
        // backdrop is scaled *before* its pass runs — the region is read out of
        // the target at the target's own resolution — so its result already
        // speaks target pixels and must not be scaled twice.
        let (composite, layer_scale) = match layer {
            PendingLayer::Composite(composite) => (composite, scale),
            PendingLayer::Backdrop(backdrop) => {
                // Flush prior composites so the target is up-to-date before the backdrop reads it.
                flush_batch(&mut batch, backend);

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
        batch.push(PreparedComposite {
            composite,
            origin,
            size,
            corner_radius: (radius > 0.0).then_some(radius),
            src_rect,
        });
    }

    flush_batch(&mut batch, backend);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_timeline_into_view(
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
    suppressed_actors: &[String],
) -> Result<(), String> {
    if backend.is_none() {
        *backend = Some(GpuFilterBackend::new(device.clone(), queue.clone(), dimensions)?);
    }
    let mut fb: Option<&mut dyn animatix::timeline::effects::FilterBackend> =
        backend.as_mut().map(|b| b as _);
    let scene = if suppressed_actors.is_empty() {
        timeline
            .evaluate_program_with_debug(time_s, dimensions, debug_options, &mut fb)
            .scene
    } else {
        timeline
            .evaluate_program_with_suppressed(
                time_s,
                dimensions,
                debug_options,
                &mut fb,
                suppressed_actors,
            )
            .scene
    };

    core.render_vello_scene(device, queue, view, dimensions.width, dimensions.height, &scene)
        .map_err(|e| e.to_string())?;

    let pending = backend.as_mut().map(|fb| fb.take_pending_composites()).unwrap_or_default();
    if let (Some(backend), Some(target)) = (backend.as_mut(), backdrop_target) {
        drain_pending_layers(core, device, queue, backend, pending, Some(target), view, 1.0);
    }
    Ok(())
}
