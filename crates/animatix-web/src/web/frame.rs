//! Frame presentation: evaluate a built document, rasterize it offscreen, and
//! blit the result to the canvas surface.

use animatix::composition::BuildTarget;
use animatix::timeline::effects::FilterBackend;
use animatix::timeline::frame_signature::FrameSignature;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use animatix_render::core::RendererCore;
use animatix_render::filter_backend::GpuFilterBackend;
use animatix_render::offscreen::drain_pending_layers;
use animatix_render::transition::TransitionCompositor;
use wasm_bindgen::prelude::*;
use wgpu::CurrentSurfaceTexture;

use super::{
    AmxPlayer, EngineContext, FrameTarget, MAX_RENDER_SCALE, MIN_RENDER_SCALE, OffscreenTarget,
    raster_dims, with_engine,
};

/// The scene to rasterize at `scale`.
///
/// Vello renders a scene 1:1 into its target — `RenderParams` carries no
/// transform in the pinned revision — so rasterizing below the scene's own
/// resolution means re-encoding the scene with an affine scale. `append` is
/// the only place that transform can be applied, and it costs one O(paths)
/// copy against a per-frame fill-rate cost that is several times larger (see
/// `docs/performance_evaluation.md`, "Web playback").
fn scaled_scene<'s>(
    scratch: &'s mut vello::Scene,
    scene: &'s vello::Scene,
    scale: f32,
) -> &'s vello::Scene {
    let s = scale.clamp(MIN_RENDER_SCALE, MAX_RENDER_SCALE);
    if (s - 1.0).abs() < 1e-3 {
        return scene;
    }
    scratch.reset();
    scratch.append(scene, Some(vello::kurbo::Affine::scale(s as f64)));
    scratch
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
        let frame_target = render_document(
            &mut core,
            &mut self.filter_backend,
            &mut self.filter_backend_to,
            &ctx.device,
            &ctx.queue,
            &mut self.offscreen,
            &mut self.offscreen_to,
            &mut self.composite,
            self.target.as_ref(),
            time_s,
            self.dims,
            self.render_scale,
            &mut self.scaled,
            self.compositor.as_ref(),
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
            core.blit_texture_to_format(
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
#[allow(clippy::too_many_arguments)]
pub(super) fn render_document(
    core: &mut RendererCore,
    primary_backend: &mut Option<GpuFilterBackend>,
    secondary_backend: &mut Option<GpuFilterBackend>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    primary: &mut Option<OffscreenTarget>,
    secondary: &mut Option<OffscreenTarget>,
    composite: &mut Option<OffscreenTarget>,
    target: Option<&BuildTarget>,
    time_s: f64,
    dims: SceneDimensions,
    scale: f32,
    scratch: &mut vello::Scene,
    compositor: Option<&TransitionCompositor>,
) -> Result<FrameTarget, String> {
    let scene = raster_dims(dims, scale);
    let from = ensure_offscreen(primary, device, "animatix-web offscreen target", scene);

    match target {
        Some(BuildTarget::MultiScene(composition)) => {
            let (scene_name, local_time_s, blend) = composition.evaluate(time_s);
            let Some(blend) = blend else {
                let timeline = composition
                    .scenes
                    .get(&scene_name)
                    .map(|scene| &scene.timeline)
                    .ok_or_else(|| format!("composition has no scene named '{scene_name}'"))?;
                render_timeline(
                    core,
                    primary_backend,
                    device,
                    queue,
                    &from.view,
                    &from.texture,
                    timeline,
                    local_time_s,
                    dims,
                    scale,
                    scratch,
                )?;
                return Ok(from.frame_target());
            };

            // Transition: both scenes render into their own targets (each with
            // its own filter backend so the two evaluations cannot stomp each
            // other's pass state) and the compositor blends them.
            let to_local = composition
                .scene_start_times
                .get(&blend.to_scene)
                .map(|start| time_s - start)
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
            let to =
                ensure_offscreen(secondary, device, "animatix-web offscreen target (to)", scene);
            let composite_target =
                ensure_offscreen(composite, device, "animatix-web transition target", scene);
            let compositor =
                compositor.ok_or_else(|| "transition compositor missing".to_string())?;

            render_timeline(
                core,
                primary_backend,
                device,
                queue,
                &from.view,
                &from.texture,
                from_timeline,
                blend.from_local,
                dims,
                scale,
                scratch,
            )?;
            render_timeline(
                core,
                secondary_backend,
                device,
                queue,
                &to.view,
                &to.texture,
                to_timeline,
                to_local,
                dims,
                scale,
                scratch,
            )?;
            compositor.render(
                device,
                queue,
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
                core,
                primary_backend,
                device,
                queue,
                &from.view,
                &from.texture,
                timeline,
                time_s,
                dims,
                scale,
                scratch,
            )?;
            Ok(from.frame_target())
        },
        None => Err("no document loaded".to_string()),
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

/// Evaluate one frame of `timeline` (with the GPU filter backend so `Filter`
/// scopes render like the export path) and draw it into `view`, including the
/// scope's pending zero-readback composites — the same tail the GUI preview
/// and the export path run.
///
/// `view` is sized by [`raster_dims`], not by `dims`: at a scale below 1 the
/// scene is re-encoded scaled down ([`scaled_scene`]) while the timeline still
/// evaluates against the scene's own dimensions. Filter scopes keep rendering
/// at scene resolution (the backend's targets are allocated that way), so their
/// composites are blitted into the smaller target at `origin * scale`.
#[allow(clippy::too_many_arguments)]
fn render_timeline(
    core: &mut RendererCore,
    filter_backend: &mut Option<GpuFilterBackend>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    view: &wgpu::TextureView,
    target: &wgpu::Texture,
    timeline: &Timeline,
    time_s: f64,
    dims: SceneDimensions,
    scale: f32,
    scratch: &mut vello::Scene,
) -> Result<(), String> {
    if filter_backend.is_none() {
        *filter_backend = Some(GpuFilterBackend::new(device.clone(), queue.clone(), dims)?);
    }
    let (raster_w, raster_h) = raster_dims(dims, scale);
    // Tell the chain what it is being drawn into. It used to allocate and
    // dispatch at scene resolution whatever the raster ended up as, so the page's
    // quality step bought nothing on a filtered figure — and the filtered figures
    // are the ones that need it. Derived from the integer raster rather than from
    // `scale` so the chain and the target cannot disagree by a rounding step.
    if let Some(fb) = filter_backend.as_mut() {
        let achieved = (raster_w as f32 / dims.width.max(1) as f32)
            .min(raster_h as f32 / dims.height.max(1) as f32);
        fb.set_raster_scale(achieved);
    }
    let mut fb: Option<&mut dyn FilterBackend> = filter_backend.as_mut().map(|b| b as _);
    let scene = timeline.evaluate_with_debug(time_s, dims, DebugRenderOptions::default(), &mut fb);
    let scene = scaled_scene(scratch, &scene, scale);
    core.render_vello_scene(device, queue, view, raster_w, raster_h, scene)
        .map_err(|e| e.to_string())?;

    let pending = filter_backend
        .as_mut()
        .map(|fb| fb.take_pending_composites())
        .unwrap_or_default();
    // The same tail the GUI and the export path run: filter composites, plus a
    // `Glass` scope's backdrop (read out of `target`) and its children above it.
    // The web target is rasterised at `dims * scale` while the timeline evaluated
    // against `dims`, so the drain is what applies the scale — including to the
    // backdrop region, which is why it receives `scale` rather than being
    // pre-scaled here.
    let s = scale.clamp(MIN_RENDER_SCALE, MAX_RENDER_SCALE);
    if let Some(fb) = filter_backend.as_mut() {
        drain_pending_layers(core, device, queue, fb, pending, Some(target), view, s);
    }
    Ok(())
}
