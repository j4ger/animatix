//! Frame presentation: evaluate a built document, rasterize it offscreen, and
//! blit the result to the canvas surface.

use animatix::composition::BuildTarget;
use animatix::scene::Scene;
use animatix::timeline::effects::FilterBackend;
use animatix::timeline::frame_signature::FrameSignature;
use animatix::timeline::{DebugRenderOptions, RuntimeInputs, SceneDimensions, Timeline};
#[cfg(not(feature = "vello-gpu-webgpu"))]
pub(super) use animatix_render::core::RendererCore as FrameRenderer;
#[cfg(not(feature = "vello-gpu-webgpu"))]
use animatix_render::filter_backend::GpuFilterBackend;
#[cfg(feature = "vello-gpu-webgpu")]
pub(super) use animatix_render::frame_gpu::FrameGpu as FrameRenderer;
#[cfg(not(feature = "vello-gpu-webgpu"))]
use animatix_render::offscreen::{RasterFit, rasterize_scene, rasterize_scene_and_present};
#[cfg(not(feature = "vello-gpu-webgpu"))]
use animatix_render::transition::composite_flip_overlay;
use animatix_render::transition::{
    TransitionCompositor, collect_flip_pairs, encode_flip_overlay, plan_shared_actors,
};
use wasm_bindgen::prelude::*;
use wgpu::CurrentSurfaceTexture;

use super::context::with_engine;
use super::{AmxPlayer, FrameTarget, MAX_RENDER_SCALE, MIN_RENDER_SCALE, OffscreenTarget};

/// Raster target size for `scale`, rounded to whole pixels and clamped to
/// `MIN_RENDER_SCALE..=MAX_RENDER_SCALE`.
pub(super) fn raster_dims(dims: SceneDimensions, scale: f32) -> (u32, u32) {
    let s = scale.clamp(MIN_RENDER_SCALE, MAX_RENDER_SCALE);
    let w = ((dims.width as f32 * s).round() as u32).max(1);
    let h = ((dims.height as f32 * s).round() as u32).max(1);
    (w, h)
}

#[wasm_bindgen]
impl AmxPlayer {
    /// Render the document at `time_s` into the canvas.
    ///
    /// No-ops when nothing is loaded, so the shell can keep rAF running
    /// through edits.
    pub fn render_frame(&mut self, time_s: f64) -> Result<(), JsError> {
        if !self.has_document() {
            return Ok(());
        }
        #[cfg(feature = "vello-gpu")]
        if let Some(webgl) = self.webgl.clone() {
            let mut frame_gpu = webgl.frame_gpu.borrow_mut();
            return self.render_frame_inner(&webgl.device, &webgl.queue, &mut frame_gpu, time_s);
        }
        with_engine(|ctx| {
            let ctx = ctx.map_err(|error| JsError::new(&error))?;
            if self.device_generation != ctx.generation {
                return Err(JsError::new("WebGPU context generation changed"));
            }
            #[cfg(not(feature = "vello-gpu-webgpu"))]
            let mut renderer = ctx.core.borrow_mut();
            #[cfg(feature = "vello-gpu-webgpu")]
            let mut renderer = ctx.frame_gpu.borrow_mut();
            self.render_frame_inner(&ctx.device, &ctx.queue, &mut renderer, time_s)
        })
    }

    /// What the frame about to be drawn at `time_s` depends on, in one
    /// comparable value. See `animatix::timeline::frame_signature` for why each
    /// member belongs there.
    fn frame_signature(
        &self,
        time_s: f64,
        surface_width: u32,
        surface_height: u32,
    ) -> FrameSignature {
        let (raster_width, raster_height) = raster_dims(self.dims, self.render_scale);
        FrameSignature {
            // Quantised exactly like the engine's own scene cache: a difference
            // the evaluator cannot see must not make the dedup redraw.
            time_ms: (time_s * 1000.0) as u64,
            scene_width: self.dims.width,
            scene_height: self.dims.height,
            raster_width,
            raster_height,
            surface_width,
            surface_height,
            // This Web revision covers both installs and input snapshots; the
            // shared signature has one generation slot for either change.
            document_generation: self.render_generation,
            content_epoch: self.target.as_ref().map_or(0, BuildTarget::content_epoch),
            // The web player has no debug overlays; a driver that can toggle
            // them names them here instead.
            debug_bits: 0,
        }
    }

    fn render_frame_inner(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut FrameRenderer,
        time_s: f64,
    ) -> Result<(), JsError> {
        let width = self.canvas.width().max(1);
        let height = self.canvas.height().max(1);
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(device, &self.config);
        }

        // A loop rests on its finished frame for the whole `hold` window while the
        // embed asks for it 60 times a second. Skip the tick before acquiring
        // anything: a skipped tick does no GPU work and the canvas keeps its image.
        let signature = self.frame_signature(time_s, width, height);
        if self.dedup.is_presented(&signature) {
            self.frames_deduped += 1;
            return Ok(());
        }

        // Vello's final pass needs STORAGE_BINDING on its target, which a default
        // canvas config does not expose; asking for it works in Chromium but
        // Firefox/Safari are unverified. Render offscreen at scene resolution and
        // blit up instead: that blit *is* the upscale, preserving `set_render_scale`
        // and the adaptive quality controller. A transition renders both scenes
        // into their own targets and blends them into a third.
        let blending = matches!(self.target.as_ref(), Some(BuildTarget::MultiScene(_)));
        // The transition compositor compiles its WGSL pipeline on first use;
        // the player owns it so the lazy init has a home beside its targets.
        if blending && self.compositor.is_none() {
            self.compositor =
                Some(TransitionCompositor::new(device).map_err(|e| JsError::new(&e))?);
        }

        let frame = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => {
                frame
            },
            // The surface changed under us (e.g. canvas resize raced the
            // frame) — reconfigure and let the next rAF tick retry. A
            // reconfigure can hand us a fresh swapchain, so forget what was on
            // screen: the image the dedup is remembering may no longer exist.
            CurrentSurfaceTexture::Outdated => {
                self.surface.configure(device, &self.config);
                self.dedup.invalidate();
                return Ok(());
            },
            // Timeout/Occluded/Lost/Validation: skip this frame; the shell's
            // render loop simply tries again next tick.
            _ => return Ok(()),
        };
        let surface_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut frame_ctx = FrameCtx {
            renderer,
            device,
            queue,
            dims: self.dims,
            scale: self.render_scale,
            #[cfg(not(feature = "vello-gpu-webgpu"))]
            scratch: &mut self.scaled,
            compositor: self.compositor.as_ref(),
            target: self.target.as_ref(),
            runtime_inputs: &self.runtime_inputs,
            time_s,
        };
        let frame_result = render_document_for_surface(
            &mut frame_ctx,
            Targets {
                primary: &mut self.offscreen,
                #[cfg(not(feature = "vello-gpu-webgpu"))]
                primary_backend: &mut self.filter_backend,
                secondary: &mut self.offscreen_to,
                #[cfg(not(feature = "vello-gpu-webgpu"))]
                secondary_backend: &mut self.filter_backend_to,
                composite: &mut self.composite,
            },
            SurfaceTarget {
                view: &surface_view,
                #[cfg(not(feature = "vello-gpu-webgpu"))]
                size: [width, height],
                format: self.config.format,
            },
        );

        match frame_result {
            Ok(()) => {
                drop(surface_view);
                queue.present(frame);
                self.frames_drawn += 1;
                // Only now: this is the frame the canvas is showing, and the
                // signature is the claim that redrawing it would change nothing.
                self.dedup.record(signature);
                Ok(())
            },
            Err(error) => {
                // Never present an acquired image that was not rendered.
                drop(surface_view);
                Err(JsError::new(&format!("render failed: {error}")))
            },
        }
    }
}

/// Everything a frame render needs that does not depend on the target slot: GPU
/// handles, renderer core, scene geometry, raster scale and the playhead. Bundling
/// lets [`render_document`]/[`render_timeline`] take a request plus a target.
pub(super) struct FrameCtx<'a> {
    pub(super) renderer: &'a mut FrameRenderer,
    pub(super) device: &'a wgpu::Device,
    pub(super) queue: &'a wgpu::Queue,
    /// Scene dimensions the timeline is evaluated against (layout resolution).
    pub(super) dims: SceneDimensions,
    /// Raster scale applied to the encoded scene, `MIN_RENDER_SCALE..=MAX_RENDER_SCALE`.
    pub(super) scale: f32,
    /// Re-encode buffer for a scaled raster; keeps a scaled frame from
    /// allocating a scene per render.
    #[cfg(not(feature = "vello-gpu-webgpu"))]
    pub(super) scratch: &'a mut Scene,
    pub(super) compositor: Option<&'a TransitionCompositor>,
    /// The document being drawn, or `None` when nothing is loaded.
    pub(super) target: Option<&'a BuildTarget>,
    /// Current validated scalar overlay; multi-scene documents always carry an
    /// empty snapshot because the Web v1 API rejects their runtime input calls.
    pub(super) runtime_inputs: &'a RuntimeInputs,
    pub(super) time_s: f64,
}

/// The offscreen slots a document is drawn into. Each scene target has its own
/// filter backend: during a transition both scenes render in one frame and a
/// shared backend would let the evaluations stomp each other's pass state.
pub(super) struct Targets<'a> {
    /// Outgoing/single scene.
    pub(super) primary: &'a mut Option<OffscreenTarget>,
    #[cfg(not(feature = "vello-gpu-webgpu"))]
    pub(super) primary_backend: &'a mut Option<GpuFilterBackend>,
    /// Incoming scene of a transition.
    pub(super) secondary: &'a mut Option<OffscreenTarget>,
    #[cfg(not(feature = "vello-gpu-webgpu"))]
    pub(super) secondary_backend: &'a mut Option<GpuFilterBackend>,
    /// The transition compositor's blended output.
    pub(super) composite: &'a mut Option<OffscreenTarget>,
}

#[derive(Clone, Copy)]
struct SurfaceTarget<'a> {
    view: &'a wgpu::TextureView,
    #[cfg(not(feature = "vello-gpu-webgpu"))]
    size: [u32; 2],
    format: wgpu::TextureFormat,
}

/// One timeline evaluated into one scene target.
struct ScenePass<'a> {
    #[cfg(not(feature = "vello-gpu-webgpu"))]
    backend: &'a mut Option<GpuFilterBackend>,
    target: &'a OffscreenTarget,
    timeline: &'a Timeline,
    time_s: f64,
    /// Actor labels hidden during a FLIP transition (their shared element is
    /// drawn once in the floating overlay instead).
    suppressed: &'a [String],
}

/// Render the loaded document for `time_s`, returning the target holding the
/// finished frame. Single-scene draws into `primary`; multi-scene draws into
/// `primary`/`secondary` and blends into `composite` during a transition — the
/// same sequence the GUI preview and export path run. `scale` sizes the offscreen
/// targets; the timeline still evaluates against `dims`, so layout is scale-free.
pub(super) fn render_document(
    ctx: &mut FrameCtx<'_>,
    targets: Targets<'_>,
) -> Result<FrameTarget, String> {
    render_document_inner(ctx, targets, None).map(|(frame, _)| frame)
}

/// Render a document for the live surface. The plain-frame path appends its
/// presentation blit to Vello's submission; paths with later GPU compositing
/// keep the existing render-then-blit order.
fn render_document_for_surface(
    ctx: &mut FrameCtx<'_>,
    targets: Targets<'_>,
    surface: SurfaceTarget<'_>,
) -> Result<(), String> {
    let (frame, already_presented) = render_document_inner(ctx, targets, Some(surface))?;
    if !already_presented {
        #[cfg(feature = "vello-gpu-webgpu")]
        ctx.renderer.blit(&frame.view, surface.view, surface.format, false);
        #[cfg(not(feature = "vello-gpu-webgpu"))]
        ctx.renderer.blit_texture_to_format(
            ctx.device,
            ctx.queue,
            &frame.view,
            surface.view,
            surface.size[0],
            surface.size[1],
            1.0,
            surface.format,
        );
    }
    Ok(())
}

fn render_document_inner(
    ctx: &mut FrameCtx<'_>,
    targets: Targets<'_>,
    surface: Option<SurfaceTarget<'_>>,
) -> Result<(FrameTarget, bool), String> {
    let scene = raster_dims(ctx.dims, ctx.scale);
    #[cfg(not(feature = "vello-gpu-webgpu"))]
    let backend_usage = wgpu::TextureUsages::STORAGE_BINDING;
    #[cfg(feature = "vello-gpu-webgpu")]
    let backend_usage = wgpu::TextureUsages::COPY_DST;
    let from = ensure_offscreen(
        targets.primary,
        ctx.device,
        "animatix-web offscreen target",
        scene,
        backend_usage,
    );

    match ctx.target {
        Some(BuildTarget::MultiScene(composition)) => {
            let (scene_name, local_time_s, blend) = composition.evaluate(ctx.time_s);
            let Some(blend) = blend else {
                let timeline = composition
                    .scenes
                    .get(&scene_name)
                    .map(|scene| &scene.timeline)
                    .ok_or_else(|| format!("composition has no scene named '{scene_name}'"))?;
                // Reuse a transition backend for the active scene before
                // dropping the now-idle incoming slot.
                #[cfg(not(feature = "vello-gpu-webgpu"))]
                {
                    if timeline.needs_filter_backend() && targets.primary_backend.is_none() {
                        *targets.primary_backend = targets.secondary_backend.take();
                    }
                    *targets.secondary_backend = None;
                }
                let presented = render_timeline(
                    ctx,
                    ScenePass {
                        #[cfg(not(feature = "vello-gpu-webgpu"))]
                        backend: targets.primary_backend,
                        target: &from,
                        timeline,
                        time_s: local_time_s,
                        suppressed: &[],
                    },
                    surface,
                )?;
                return Ok((from.frame_target(), presented));
            };

            // Transition: both scenes render into their own targets (each with
            // its own filter backend so the two evaluations cannot stomp each
            // other's pass state) and the compositor blends them. Actors that
            // share a `shared_id` are suppressed in both scene rasters and
            // drawn once in a floating FLIP overlay above the blend.
            let to_local = composition
                .scene_start_times
                .get(&blend.to_scene)
                .map(|start| ctx.time_s - start)
                .unwrap_or(local_time_s);
            let from_timeline = &composition
                .scenes
                .get(&blend.from_scene)
                .ok_or_else(|| format!("composition has no scene named '{}'", blend.from_scene))?
                .timeline;
            let to_timeline = &composition
                .scenes
                .get(&blend.to_scene)
                .ok_or_else(|| format!("composition has no scene named '{}'", blend.to_scene))?
                .timeline;
            #[cfg(not(feature = "vello-gpu-webgpu"))]
            rebalance_filter_backends(
                from_timeline.needs_filter_backend(),
                to_timeline.needs_filter_backend(),
                &mut *targets.primary_backend,
                &mut *targets.secondary_backend,
            );
            let shared = plan_shared_actors(from_timeline, blend.from_local, to_timeline, to_local);
            let flip_items = collect_flip_pairs(
                &shared,
                from_timeline,
                blend.from_local,
                to_timeline,
                to_local,
                ctx.dims,
                DebugRenderOptions::default(),
            );
            let to = ensure_offscreen(
                targets.secondary,
                ctx.device,
                "animatix-web offscreen target (to)",
                scene,
                backend_usage,
            );
            let composite_target = ensure_offscreen(
                targets.composite,
                ctx.device,
                "animatix-web transition target",
                scene,
                backend_usage,
            );
            let compositor =
                ctx.compositor.ok_or_else(|| "transition compositor missing".to_string())?;

            render_timeline(
                ctx,
                ScenePass {
                    #[cfg(not(feature = "vello-gpu-webgpu"))]
                    backend: targets.primary_backend,
                    target: &from,
                    timeline: from_timeline,
                    time_s: blend.from_local,
                    suppressed: &shared.suppressed_from,
                },
                None,
            )?;
            render_timeline(
                ctx,
                ScenePass {
                    #[cfg(not(feature = "vello-gpu-webgpu"))]
                    backend: targets.secondary_backend,
                    target: &to,
                    timeline: to_timeline,
                    time_s: to_local,
                    suppressed: &shared.suppressed_to,
                },
                None,
            )?;
            compositor.render(
                ctx.device,
                ctx.queue,
                &from.view,
                &to.view,
                &composite_target.view,
                scene.0,
                scene.1,
                blend.progress as f32,
                &blend.id,
                blend.easing,
            )?;

            if !flip_items.is_empty() {
                composite_web_flip_overlay(
                    ctx,
                    &encode_flip_overlay(&flip_items, blend.progress as f32, blend.easing),
                    &from.view,
                    &composite_target.view,
                    #[cfg(not(feature = "vello-gpu-webgpu"))]
                    scene,
                )?;
            }

            Ok((composite_target.frame_target(), false))
        },
        Some(BuildTarget::SingleScene(timeline)) => {
            // Reuse a transition backend for this scene before dropping the
            // now-idle incoming slot.
            #[cfg(not(feature = "vello-gpu-webgpu"))]
            {
                if timeline.needs_filter_backend() && targets.primary_backend.is_none() {
                    *targets.primary_backend = targets.secondary_backend.take();
                }
                *targets.secondary_backend = None;
            }
            let presented = render_timeline(
                ctx,
                ScenePass {
                    #[cfg(not(feature = "vello-gpu-webgpu"))]
                    backend: targets.primary_backend,
                    target: &from,
                    timeline,
                    time_s: ctx.time_s,
                    suppressed: &[],
                },
                surface,
            )?;
            Ok((from.frame_target(), presented))
        },
        None => {
            #[cfg(not(feature = "vello-gpu-webgpu"))]
            {
                *targets.primary_backend = None;
                *targets.secondary_backend = None;
            }
            Err("no document loaded".to_string())
        },
    }
}

fn composite_web_flip_overlay(
    ctx: &mut FrameCtx<'_>,
    overlay: &Scene,
    from: &wgpu::TextureView,
    composite: &wgpu::TextureView,
    #[cfg(not(feature = "vello-gpu-webgpu"))] scene: (u32, u32),
) -> Result<(), String> {
    let scale = ctx.scale.clamp(MIN_RENDER_SCALE, MAX_RENDER_SCALE);
    #[cfg(feature = "vello-gpu-webgpu")]
    {
        ctx.renderer.render_into(overlay, from, kurbo::Affine::scale(scale as f64))?;
        ctx.renderer.over(composite, from, 1.0)
    }
    #[cfg(not(feature = "vello-gpu-webgpu"))]
    {
        let overlay_ref: &Scene = if (scale - 1.0).abs() < 1e-3 {
            overlay
        } else {
            ctx.scratch.reset();
            ctx.scratch.append(overlay, Some(kurbo::Affine::scale(scale as f64)));
            &*ctx.scratch
        };
        composite_flip_overlay(
            ctx.renderer,
            ctx.device,
            ctx.queue,
            from,
            composite,
            SceneDimensions {
                width: scene.0,
                height: scene.1,
            },
            overlay_ref,
        )
    }
}

/// Move a reusable backend out of a side that does not need one when the
/// other transition side does and has no backend yet.
#[cfg(not(feature = "vello-gpu-webgpu"))]
fn rebalance_filter_backends(
    from_needs: bool,
    to_needs: bool,
    from_backend: &mut Option<GpuFilterBackend>,
    to_backend: &mut Option<GpuFilterBackend>,
) {
    if from_needs && !to_needs && from_backend.is_none() {
        *from_backend = to_backend.take();
    } else if !from_needs && to_needs && to_backend.is_none() {
        *to_backend = from_backend.take();
    }
}

/// Evaluate the timeline for the target's raster scale and draw one frame.
fn render_timeline(
    ctx: &mut FrameCtx<'_>,
    pass: ScenePass<'_>,
    surface: Option<SurfaceTarget<'_>>,
) -> Result<bool, String> {
    let ScenePass {
        #[cfg(not(feature = "vello-gpu-webgpu"))]
        backend,
        target,
        timeline,
        time_s,
        suppressed,
    } = pass;
    #[cfg(not(feature = "vello-gpu-webgpu"))]
    let (raster_w, raster_h) = raster_dims(ctx.dims, ctx.scale);
    #[cfg(not(feature = "vello-gpu-webgpu"))]
    let mut fb: Option<&mut dyn FilterBackend> = {
        if timeline.needs_filter_backend() {
            if backend.is_none() {
                *backend =
                    Some(GpuFilterBackend::new(ctx.device.clone(), ctx.queue.clone(), ctx.dims)?);
            }
        } else {
            // A newly installed document may no longer need the effect targets.
            *backend = None;
        }
        backend.as_mut().map(|fb| {
            let achieved = (raster_w as f32 / ctx.dims.width.max(1) as f32)
                .min(raster_h as f32 / ctx.dims.height.max(1) as f32);
            fb.set_raster_scale(achieved);
            fb as &mut dyn FilterBackend
        })
    };
    #[cfg(feature = "vello-gpu-webgpu")]
    let mut recorder = animatix_render::frame_gpu::SceneRecorder;
    #[cfg(feature = "vello-gpu-webgpu")]
    let mut fb: Option<&mut dyn FilterBackend> = Some(&mut recorder);

    // FLIP overlays need the suppressed program. Other frames can take the
    // clone-free presentation path because the Web player already deduplicates.
    let presentation = suppressed.is_empty();
    let scene = if presentation {
        timeline
            .evaluate_for_presentation_with_inputs(time_s, ctx.dims, &mut fb, ctx.runtime_inputs)
            .map_err(|error| error.to_string())?
    } else {
        timeline
            .evaluate_program_with_suppressed(
                time_s,
                ctx.dims,
                DebugRenderOptions::default(),
                &mut fb,
                suppressed,
            )
            .scene
    };

    let scale = ctx.scale.clamp(MIN_RENDER_SCALE, MAX_RENDER_SCALE);
    #[cfg(feature = "vello-gpu-webgpu")]
    {
        let transform = kurbo::Affine::scale(f64::from(scale));
        if let Some(surface) = surface {
            ctx.renderer.render_and_present(
                &scene,
                &target.view,
                surface.view,
                transform,
                surface.format,
            )?;
            Ok(true)
        } else {
            ctx.renderer.render_frame_into(&scene, &target.view, transform)?;
            Ok(false)
        }
    }
    #[cfg(not(feature = "vello-gpu-webgpu"))]
    {
        let fit = if (scale - 1.0).abs() < 1e-3 {
            RasterFit::OneToOne
        } else {
            RasterFit::Scaled {
                scale,
                scratch: &mut *ctx.scratch,
            }
        };
        let raster = SceneDimensions {
            width: raster_w,
            height: raster_h,
        };
        let merged_presentation = surface.is_some()
            && presentation
            && !timeline.needs_filter_backend()
            && backend.is_none();
        let result = if let Some(surface) = surface.filter(|_| merged_presentation) {
            rasterize_scene_and_present(
                ctx.renderer,
                ctx.device,
                ctx.queue,
                &target.view,
                &scene,
                raster,
                fit,
                surface.view,
                surface.size,
                surface.format,
            )
        } else {
            rasterize_scene(
                ctx.renderer,
                ctx.device,
                ctx.queue,
                &target.view,
                &scene,
                raster,
                fit,
                backend.as_mut(),
                Some(&target.texture),
            )
        };
        if presentation {
            timeline.recycle_presentation_scene(scene);
        }
        result.map(|()| merged_presentation)
    }
}

impl OffscreenTarget {
    /// A rendered frame keyed to this target's texture and view.
    fn frame_target(&self) -> FrameTarget {
        FrameTarget {
            texture: self.texture.clone(),
            view: self.view.clone(),
        }
    }
}

/// Offscreen attachment for composition, readback, and the selected raster backend.
fn create_offscreen(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    backend_usage: wgpu::TextureUsages,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | backend_usage
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Create or resize an offscreen target and hand back the whole handle.
fn ensure_offscreen(
    slot: &mut Option<OffscreenTarget>,
    device: &wgpu::Device,
    label: &str,
    scene: (u32, u32),
    backend_usage: wgpu::TextureUsages,
) -> OffscreenTarget {
    if let Some(existing) = slot {
        if (existing.width, existing.height) == scene
            && existing.texture.usage().contains(backend_usage)
        {
            return existing.clone();
        }
    }
    let (texture, view) = create_offscreen(device, label, scene.0, scene.1, backend_usage);
    let target = OffscreenTarget {
        texture: texture.clone(),
        view: view.clone(),
        width: scene.0,
        height: scene.1,
    };
    *slot = Some(target.clone());
    target
}
