//! Frame presentation: evaluate a built document, rasterize it offscreen, and
//! blit the result to the canvas surface.

use animatix::composition::BuildTarget;
use animatix::timeline::effects::FilterBackend;
use animatix::timeline::frame_signature::FrameSignature;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use animatix_render::core::RendererCore;
use animatix_render::filter_backend::GpuFilterBackend;
use animatix_render::offscreen::{RasterFit, rasterize_scene};
use animatix_render::transition::{
    TransitionCompositor, collect_flip_pairs, composite_flip_overlay, encode_flip_overlay,
    plan_shared_actors,
};
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

        // Blit the offscreen frame onto the swapchain, scaling to canvas pixels
        // through the per-format pipeline (the browser chooses the surface format;
        // Firefox prefers Bgra8Unorm). Present only inside the success branch: an
        // undrawn swapchain image would, with dedup, become a permanent blank.
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
            ctx.queue.present(frame);
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

/// Everything a frame render needs that does not depend on the target slot: GPU
/// handles, renderer core, scene geometry, raster scale and the playhead. Bundling
/// lets [`render_document`]/[`render_timeline`] take a request plus a target.
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

/// The offscreen slots a document is drawn into. Each scene target has its own
/// filter backend: during a transition both scenes render in one frame and a
/// shared backend would let the evaluations stomp each other's pass state.
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
                        suppressed: &[],
                    },
                )?;
                return Ok(from.frame_target());
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
                    suppressed: &shared.suppressed_from,
                },
            )?;
            render_timeline(
                ctx,
                ScenePass {
                    backend: targets.secondary_backend,
                    target: &to,
                    timeline: to_timeline,
                    time_s: to_local,
                    suppressed: &shared.suppressed_to,
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

            if !flip_items.is_empty() {
                let overlay = encode_flip_overlay(&flip_items, blend.progress as f32, blend.easing);
                let scale = ctx.scale.clamp(MIN_RENDER_SCALE, MAX_RENDER_SCALE);
                let overlay_ref: &vello::Scene = if (scale - 1.0).abs() < 1e-3 {
                    &overlay
                } else {
                    // Same scaled-scene path `rasterize_scene` uses: append
                    // under one uniform transform so geometry and the overlay
                    // target share the raster scale.
                    ctx.scratch.reset();
                    ctx.scratch.append(&overlay, Some(kurbo::Affine::scale(scale as f64)));
                    &*ctx.scratch
                };
                composite_flip_overlay(
                    ctx.core,
                    ctx.device,
                    ctx.queue,
                    &from.view,
                    &composite_target.view,
                    SceneDimensions {
                        width: scene.0,
                        height: scene.1,
                    },
                    overlay_ref,
                )?;
            }

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
                    suppressed: &[],
                },
            )?;
            Ok(from.frame_target())
        },
        None => Err("no document loaded".to_string()),
    }
}

/// Evaluate one frame of `pass.timeline` with the GPU filter backend (so `Filter`
/// scopes render like the export path) and draw it into `pass.target`, including
/// pending zero-readback composites, via [`rasterize_scene`]. The target is sized
/// by [`raster_dims`], not `ctx.dims`: the scene is re-encoded scaled down while
/// the timeline evaluates at scene dimensions; filter composites are blitted into
/// the smaller target at `origin * scale`.
fn render_timeline(ctx: &mut FrameCtx<'_>, pass: ScenePass<'_>) -> Result<(), String> {
    let ScenePass {
        backend,
        target,
        timeline,
        time_s,
        suppressed,
    } = pass;
    if backend.is_none() {
        *backend = Some(GpuFilterBackend::new(ctx.device.clone(), ctx.queue.clone(), ctx.dims)?);
    }
    let (raster_w, raster_h) = raster_dims(ctx.dims, ctx.scale);
    // Tell the chain its real raster size (derived from the integer raster, not
    // `scale`, so chain and target cannot disagree by a rounding step).
    if let Some(fb) = backend.as_mut() {
        let achieved = (raster_w as f32 / ctx.dims.width.max(1) as f32)
            .min(raster_h as f32 / ctx.dims.height.max(1) as f32);
        fb.set_raster_scale(achieved);
    }
    let mut fb: Option<&mut dyn FilterBackend> = backend.as_mut().map(|b| b as _);
    // The web player has its own raster-level `FrameDedup`, so the engine's
    // `time_ms`-keyed frame cache only adds a full `vello::Scene` deep copy on
    // every animation frame. Use the clone-free presentation entry point on the
    // common single-scene path; FLIP transitions still need the suppressed
    // program and keep their existing call.
    let presentation = suppressed.is_empty();
    let scene = if presentation {
        timeline.evaluate_for_presentation(time_s, ctx.dims, &mut fb)
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
    // The shared rasterizer applies the scale to the encoding and drain, including
    // the backdrop region; the drain receives the factor, not a pre-scaled region.
    let result = rasterize_scene(
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
    );
    // Park the presentation scene back so the next frame reuses its encoding
    // allocation instead of allocating a fresh `vello::Scene`. Transition
    // frames keep their own (cloned) scene-buffer behaviour unchanged.
    if presentation {
        timeline.recycle_presentation_scene(scene);
    }
    result
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

/// Offscreen target usable by vello (STORAGE_BINDING), filter passes
/// (RENDER_ATTACHMENT), texture compositors (TEXTURE_BINDING) and readback (COPY_SRC).
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
