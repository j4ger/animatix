//! Frame presentation: evaluate a built document, rasterize it offscreen, and
//! blit the result to the canvas surface.

use animatix::composition::BuildTarget;
use animatix::timeline::effects::FilterBackend;
use animatix::timeline::frame_signature::FrameSignature;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use animatix_render::core::RendererCore;
use animatix_render::filter_backend::GpuFilterBackend;
use animatix_render::offscreen::{RasterFit, rasterize_scene};
use animatix_render::transition::TransitionCompositor;
use wasm_bindgen::prelude::*;
use wgpu::CurrentSurfaceTexture;

use super::{
    AmxPlayer, EngineContext, FrameTarget, MAX_RENDER_SCALE, MIN_RENDER_SCALE, OffscreenTarget,
    raster_dims, with_engine,
};

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
        with_engine(|ctx| self.render_frame_inner(ctx, time_s))
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
            document_generation: self.document_generation,
            content_epoch: self.target.as_ref().map_or(0, BuildTarget::content_epoch),
            // The web player has no debug overlays; a driver that can toggle
            // them names them here instead.
            debug_bits: 0,
        }
    }

    fn render_frame_inner(
        &mut self,
        ctx: Result<&EngineContext, String>,
        time_s: f64,
    ) -> Result<(), JsError> {
        let ctx = ctx.map_err(|e| JsError::new(&e))?;
        let width = self.canvas.width().max(1);
        let height = self.canvas.height().max(1);
        if self.config.width != width || self.config.height != height {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(&ctx.device, &self.config);
        }

        // A loop rests on its finished frame for the whole `hold` window, and the
        // embed keeps asking for it 60 times a second. Evaluation is ~47 µs of
        // that; the raster is the milliseconds — so the check is worth doing
        // before anything is acquired, and a skipped tick does no GPU work at
        // all: the canvas holds the last presented image.
        let signature = self.frame_signature(time_s, width, height);
        if self.dedup.is_presented(&signature) {
            self.frames_deduped += 1;
            return Ok(());
        }

        // Vello draws its final pass through a compute pipeline that needs
        // STORAGE_BINDING on its target. A canvas configured the default way is
        // not given that usage (measured `GPUTextureUsage` = 16,
        // RENDER_ATTACHMENT only, for rgba8unorm and bgra8unorm, opaque and
        // premultiplied, at 64x64 and 1100x619 alike) — but *asking* works in
        // Chromium: passing a non-spec `usage` member to
        // `GPUCanvasContext.configure` yields a surface texture reporting 24, and
        // asking for more yields 30. So rendering straight into the canvas is
        // possible, and we still do not: the blit it would delete measures
        // 0.003 ms (`performance_evaluation.md` §3.7), and it is not overhead —
        // it *is* the upscaling step. Drawing into the surface means rasterizing
        // at canvas resolution every frame, which forfeits `set_render_scale`
        // and the adaptive quality controller, and Firefox/Safari are unverified
        // for the `usage` member (an ignored dictionary key reads as "accepts",
        // so a probe plus a blit fallback is mandatory, most sensibly as
        // "direct only when scale == 1").
        //
        // Mirror the GUI's PreviewSurface instead: render into an offscreen
        // texture we own at scene resolution (vello draws scene units 1:1 — no
        // camera scaling), then blit it scaled onto the surface view. During a
        // multi-scene transition the outgoing and incoming scenes render into two
        // of those targets and the compositor blends them into a third.
        let blending = matches!(self.target.as_ref(), Some(BuildTarget::MultiScene(_)));
        // The transition compositor compiles its WGSL pipeline on first use;
        // the player owns it so the lazy init has a home beside its targets.
        if blending && self.compositor.is_none() {
            self.compositor =
                Some(TransitionCompositor::new(&ctx.device).map_err(|e| JsError::new(&e))?);
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
                self.surface.configure(&ctx.device, &self.config);
                self.dedup.invalidate();
                return Ok(());
            },
            // Timeout/Occluded/Lost/Validation: skip this frame; the shell's
            // render loop simply tries again next tick.
            _ => return Ok(()),
        };
        let surface_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut core = ctx.core.borrow_mut();
        let mut frame_ctx = FrameCtx {
            core: &mut core,
            device: &ctx.device,
            queue: &ctx.queue,
            dims: self.dims,
            scale: self.render_scale,
            scratch: &mut self.scaled,
            compositor: self.compositor.as_ref(),
            target: self.target.as_ref(),
            time_s,
        };
        let frame_target = render_document(
            &mut frame_ctx,
            Targets {
                primary: &mut self.offscreen,
                primary_backend: &mut self.filter_backend,
                secondary: &mut self.offscreen_to,
                secondary_backend: &mut self.filter_backend_to,
                composite: &mut self.composite,
            },
        );

        // Blit the rendered frame onto the swapchain view and present. The
        // blit scales from the offscreen resolution to canvas pixels. The
        // surface's format is the browser's choice (Firefox orders Bgra8Unorm
        // first), so the blit runs through the per-format pipeline rather than
        // the internal Rgba8Unorm one.
        //
        // Presenting is inside the success branch on purpose. A swapchain image
        // acquired but never drawn into is not "the previous frame" — presenting
        // it puts undefined content on screen, which with the dedup in place
        // would turn one bad frame into a permanent blank: the failed tick
        // records nothing, so every identical tick after it is skipped and the
        // canvas never gets another present.
        if let Ok(offscreen) = &frame_target {
            frame_ctx.core.blit_texture_to_format(
                &ctx.device,
                &ctx.queue,
                &offscreen.view,
                &surface_view,
                width,
                height,
                1.0,
                self.config.format,
            );
            drop(surface_view);
            frame.present();
            self.frames_drawn += 1;
            // Only now: this is the frame the canvas is showing, and the
            // signature is the claim that redrawing it would change nothing.
            self.dedup.record(signature);
            Ok(())
        } else {
            // Never drawn into, never presented: the canvas keeps the last frame
            // that did make it. Release the view before the texture goes.
            drop(surface_view);
            frame_target
                .map(|_| ())
                .map_err(|e| JsError::new(&format!("render failed: {e}")))
        }
    }
}

/// Everything a frame render needs that does not depend on the target slot it
/// draws into: the shared GPU handles and renderer core, the scene geometry and
/// raster scale, and the document/playhead being drawn.
///
/// Bundling these is what lets [`render_document`] and [`render_timeline`] take
/// a request plus a target instead of a dozen positional arguments.
pub(super) struct FrameCtx<'a> {
    pub(super) core: &'a mut RendererCore,
    pub(super) device: &'a wgpu::Device,
    pub(super) queue: &'a wgpu::Queue,
    /// Scene dimensions the timeline is evaluated against (layout resolution).
    pub(super) dims: SceneDimensions,
    /// Raster scale applied to the encoded scene, `MIN_RENDER_SCALE..=MAX_RENDER_SCALE`.
    pub(super) scale: f32,
    /// Re-encode buffer for a scaled raster; keeps a scaled frame from
    /// allocating a scene per render.
    pub(super) scratch: &'a mut vello::Scene,
    pub(super) compositor: Option<&'a TransitionCompositor>,
    /// The document being drawn, or `None` when nothing is loaded.
    pub(super) target: Option<&'a BuildTarget>,
    pub(super) time_s: f64,
}

/// The offscreen slots a document is drawn into.
///
/// Each scene target has its own filter backend: during a multi-scene
/// transition both scenes render in the same frame, and a shared backend would
/// let the two evaluations stomp each other's pass state.
pub(super) struct Targets<'a> {
    /// Outgoing/single scene.
    pub(super) primary: &'a mut Option<OffscreenTarget>,
    pub(super) primary_backend: &'a mut Option<GpuFilterBackend>,
    /// Incoming scene of a transition.
    pub(super) secondary: &'a mut Option<OffscreenTarget>,
    pub(super) secondary_backend: &'a mut Option<GpuFilterBackend>,
    /// The transition compositor's blended output.
    pub(super) composite: &'a mut Option<OffscreenTarget>,
}

/// One timeline evaluated into one scene target.
struct ScenePass<'a> {
    backend: &'a mut Option<GpuFilterBackend>,
    target: &'a OffscreenTarget,
    timeline: &'a Timeline,
    time_s: f64,
}

/// Render the loaded document for `time_s`, returning the target that holds
/// the finished frame.
///
/// This is the one place that decides what a frame is. A single-scene document
/// renders into `primary`; a multi-scene document renders its scenes into
/// `primary`/`secondary` and, while a transition is active, blends the two into
/// `composite` with the GPU compositor — the same sequence the GUI preview and
/// the export path run. The canvas path blits the returned view to the surface
/// and the readback path copies the returned texture to a buffer: one frame
/// definition, two consumers.
///
/// `scale` sizes the offscreen targets relative to the scene's resolution
/// ([`raster_dims`]); the timeline is always evaluated against `dims`, so
/// layout is scale-independent.
pub(super) fn render_document(
    ctx: &mut FrameCtx<'_>,
    targets: Targets<'_>,
) -> Result<FrameTarget, String> {
    let scene = raster_dims(ctx.dims, ctx.scale);
    let from =
        ensure_offscreen(targets.primary, ctx.device, "animatix-web offscreen target", scene);

    match ctx.target {
        Some(BuildTarget::MultiScene(composition)) => {
            let (scene_name, local_time_s, blend) = composition.evaluate(ctx.time_s);
            let Some(blend) = blend else {
                let timeline = composition
                    .scenes
                    .get(&scene_name)
                    .map(|scene| &scene.timeline)
                    .ok_or_else(|| format!("composition has no scene named '{scene_name}'"))?;
                render_timeline(
                    ctx,
                    ScenePass {
                        backend: targets.primary_backend,
                        target: &from,
                        timeline,
                        time_s: local_time_s,
                    },
                )?;
                return Ok(from.frame_target());
            };

            // Transition: both scenes render into their own targets (each with
            // its own filter backend so the two evaluations cannot stomp each
            // other's pass state) and the compositor blends them.
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
            let to = ensure_offscreen(
                targets.secondary,
                ctx.device,
                "animatix-web offscreen target (to)",
                scene,
            );
            let composite_target = ensure_offscreen(
                targets.composite,
                ctx.device,
                "animatix-web transition target",
                scene,
            );
            let compositor =
                ctx.compositor.ok_or_else(|| "transition compositor missing".to_string())?;

            render_timeline(
                ctx,
                ScenePass {
                    backend: targets.primary_backend,
                    target: &from,
                    timeline: from_timeline,
                    time_s: blend.from_local,
                },
            )?;
            render_timeline(
                ctx,
                ScenePass {
                    backend: targets.secondary_backend,
                    target: &to,
                    timeline: to_timeline,
                    time_s: to_local,
                },
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

            Ok(composite_target.frame_target())
        },
        Some(BuildTarget::SingleScene(timeline)) => {
            render_timeline(
                ctx,
                ScenePass {
                    backend: targets.primary_backend,
                    target: &from,
                    timeline,
                    time_s: ctx.time_s,
                },
            )?;
            Ok(from.frame_target())
        },
        None => Err("no document loaded".to_string()),
    }
}

/// Evaluate one frame of `pass.timeline` (with the GPU filter backend so
/// `Filter` scopes render like the export path) and draw it into
/// `pass.target`, including the scope's pending zero-readback composites — the
/// same tail the GUI preview and the export path run, via
/// [`rasterize_scene`](animatix_render::offscreen::rasterize_scene).
///
/// The target is sized by [`raster_dims`], not by `ctx.dims`: at a scale below 1
/// the scene is re-encoded scaled down by the shared rasterizer while the
/// timeline still evaluates against the scene's own dimensions. Filter scopes
/// keep rendering at scene resolution (the backend's targets are allocated that
/// way), so their composites are blitted into the smaller target at
/// `origin * scale`.
fn render_timeline(ctx: &mut FrameCtx<'_>, pass: ScenePass<'_>) -> Result<(), String> {
    let ScenePass {
        backend,
        target,
        timeline,
        time_s,
    } = pass;
    if backend.is_none() {
        *backend = Some(GpuFilterBackend::new(ctx.device.clone(), ctx.queue.clone(), ctx.dims)?);
    }
    let (raster_w, raster_h) = raster_dims(ctx.dims, ctx.scale);
    // Tell the chain what it is being drawn into. It used to allocate and
    // dispatch at scene resolution whatever the raster ended up as, so the page's
    // quality step bought nothing on a filtered figure — and the filtered figures
    // are the ones that need it. Derived from the integer raster rather than from
    // `scale` so the chain and the target cannot disagree by a rounding step.
    if let Some(fb) = backend.as_mut() {
        let achieved = (raster_w as f32 / ctx.dims.width.max(1) as f32)
            .min(raster_h as f32 / ctx.dims.height.max(1) as f32);
        fb.set_raster_scale(achieved);
    }
    let mut fb: Option<&mut dyn FilterBackend> = backend.as_mut().map(|b| b as _);
    let scene =
        timeline.evaluate_with_debug(time_s, ctx.dims, DebugRenderOptions::default(), &mut fb);
    drop(fb);

    let scale = ctx.scale.clamp(MIN_RENDER_SCALE, MAX_RENDER_SCALE);
    let fit = if (scale - 1.0).abs() < 1e-3 {
        RasterFit::OneToOne
    } else {
        RasterFit::Scaled {
            scale,
            scratch: &mut *ctx.scratch,
        }
    };
    // The shared rasterizer applies the scale to the scene encoding and to the
    // drain, including the backdrop region — which is why the drain receives the
    // factor rather than a pre-scaled region.
    rasterize_scene(
        &mut *ctx.core,
        ctx.device,
        ctx.queue,
        &target.view,
        &scene,
        SceneDimensions {
            width: raster_w,
            height: raster_h,
        },
        fit,
        backend.as_mut(),
        Some(&target.texture),
    )
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

/// Create an offscreen target usable by vello (STORAGE_BINDING), the GPU
/// filter passes (RENDER_ATTACHMENT), texture-based compositors
/// (TEXTURE_BINDING) and pixel readback (COPY_SRC).
fn create_offscreen(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
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
            | wgpu::TextureUsages::STORAGE_BINDING
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
) -> OffscreenTarget {
    if let Some(existing) = slot {
        if (existing.width, existing.height) == scene {
            return existing.clone();
        }
    }
    let (texture, view) = create_offscreen(device, label, scene.0, scene.1);
    let target = OffscreenTarget {
        texture: texture.clone(),
        view: view.clone(),
        width: scene.0,
        height: scene.1,
    };
    *slot = Some(target.clone());
    target
}
