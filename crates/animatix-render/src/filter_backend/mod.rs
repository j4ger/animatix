//! GPU effect backend for preview and export renderers.
//!
//! Both [`crate::offscreen::OffscreenRenderer`] and the GUI's
//! `PreviewSurface` need identical offscreen → effect → composite behaviour for
//! `Filter` scopes. This module provides a single [`GpuFilterBackend`] that runs
//! an [`EffectChain`] as an ordered list of compute passes over host-owned
//! ping-pong textures.
//!
//! The pass/uniform contract lives in `docs/effects.md`. Each pass is submitted
//! in its own encoder: back-to-back compute passes sharing ping-pong textures in
//! one encoder do not make a storage write visible to the next pass on every
//! driver (probe 009), and a submit boundary is a portable synchronisation
//! point.

use std::cell::RefCell;
use std::collections::HashMap;

use animatix::timeline::SceneDimensions;
use animatix::timeline::effects::{
    Effect, EffectChain, EffectRegion, FilterBackend, PendingBackdrop, PendingComposite,
    PendingLayer, effect,
};
use animatix::timeline::image::SceneImage;
use animatix_core::effect::EffectParamSpec;

use crate::core::RendererCore;

// ── `ANIMATIX_FILTER_TIMING` probe clock ────────────────────────────────────

/// Whether this process asked for the probe. Read once: these calls sit on the
/// per-scope render path, and `std::env::var_os` locks the environment on every
/// lookup, so deciding per frame would put a mutex on the hot path for a knob
/// that cannot change after start-up.
static TIMING_ON: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    !cfg!(target_arch = "wasm32") && std::env::var_os("ANIMATIX_FILTER_TIMING").is_some()
});

/// Start a probe timer, or return `None` when the probe cannot run.
///
/// The printing at every call site was env-gated, but the *clock* was not:
/// `std::time::Instant::now()` traps on wasm32-unknown-unknown with "time not
/// implemented on this platform", and the scopes these probes sit on —
/// `Filter`, `Mask`, `Glass` — are exactly the ones the web player renders, so
/// a browser frame with a backdrop scope panicked inside the engine and left
/// the player unable to render for the rest of the page's life. A probe is
/// additive or it does not land: the clock is now as conditional as the output.
pub(crate) fn timing_probe() -> Option<std::time::Instant> {
    if *TIMING_ON {
        Some(std::time::Instant::now())
    } else {
        None
    }
}

/// Milliseconds since a [`timing_probe`] start; `0.0` for a probe that was off,
/// which is never printed.
pub(crate) fn probe_ms(start: Option<std::time::Instant>) -> f64 {
    start.map(|t| t.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0)
}

mod pipeline;
#[cfg(test)]
mod tests;

use pipeline::*;

/// dedicated [`RendererCore`] so it never contends with the main renderer.
pub struct GpuFilterBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    core: RendererCore,
    // Render target (Vello draws here)
    render_texture: wgpu::Texture,
    render_view: wgpu::TextureView,
    // Ping-pong textures for compute shaders
    tex_a: wgpu::Texture,
    tex_a_view: wgpu::TextureView,
    tex_b: wgpu::Texture,
    tex_b_view: wgpu::TextureView,
    /// The pre-chain pixels, bound at §4.2 binding 5 for every pass of the
    /// scope. Bloom-style effects add light back onto the original instead of
    /// onto whatever the previous pass left, and a chain of effects can mix the
    /// two. Copied once per scope, before the first pass runs.
    tex_original: wgpu::Texture,
    tex_original_view: wgpu::TextureView,
    // Readback buffer
    output_buffer: wgpu::Buffer,
    bytes_per_row: u32,
    _dimensions: SceneDimensions,
    /// What the caller told us the frame is rasterized at, as a fraction of
    /// `_dimensions`. `1.0` until a runtime calls `set_raster_scale`.
    raster_scale: f32,
    // Shared effect binding layout (bindings 0-4) and pipeline layout.
    bind_group_layout: wgpu::BindGroupLayout,
    pipeline_layout: wgpu::PipelineLayout,
    /// Lazily built pipelines, keyed by the effect's authored type name.
    ///
    /// The key is owned because plugin effect names come from the registry, not
    /// from string literals.
    pipelines: HashMap<Box<str>, EffectPipeline>,
    /// Linear clamp sampler (binding 4).
    sampler: wgpu::Sampler,
    /// Per-pass host context uniform (binding 3).
    context_buffer: wgpu::Buffer,
    /// Which internal texture holds the most recent filtered result.
    last_filtered_source: FilteredSource,
    /// Effect-space dimensions of the most recent run (region size when a
    /// region of interest was used, otherwise the full scene).
    last_effect_dims: SceneDimensions,
    /// The region the caller will harvest (full canvas when `None`); the chain
    /// always runs over the full canvas, this only scopes the readback/copy.
    last_region: Option<EffectRegion>,
    /// Pending zero-readback filter textures to be composited after scene render.
    pending_composites: Vec<PendingLayer>,
    /// Region-scoped scratch targets, keyed by quantized size. Region paths
    /// render, seed, ping-pong, and harvest entirely inside these textures so
    /// per-scope cost scales with the scope, not the canvas (PF-7's
    /// no-reallocation guarantee is preserved by quantizing sizes and capping
    /// the cache: steady-state scenes reuse one entry per distinct ROI).
    region_scratch: HashMap<(u32, u32), RegionScratch>,
    /// Ping-pong texture of the most recent *region-scoped* run, when one was
    /// used. Its contents cover `last_effect_dims` starting at (0, 0).
    last_region_pp: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Texture pool for pending zero-readback composites to avoid per-frame texture allocation churn.
    pending_texture_pool: RefCell<TexturePool>,
}

type PooledTexture = (wgpu::Texture, wgpu::TextureView);
type TexturePool = HashMap<(u32, u32), Vec<PooledTexture>>;

/// Request to fuse texture copying directly into a filter chain's command encoder.
#[derive(Clone, Copy, Debug)]
struct PendingHarvestReq {
    dimensions: SceneDimensions,
    alpha: f32,
    origin: [f32; 2],
    source_origin: wgpu::Origin3d,
}

/// Region-scoped scratch targets for one quantized ROI size.
#[derive(Clone)]
struct RegionScratch {
    render_texture: wgpu::Texture,
    render_view: wgpu::TextureView,
    pp_a: wgpu::Texture,
    pp_a_view: wgpu::TextureView,
    pp_b: wgpu::Texture,
    pp_b_view: wgpu::TextureView,
    pp_original: wgpu::Texture,
    pp_original_view: wgpu::TextureView,
}

/// The textures one chain run reads and writes.
///
/// `seed_texture` is whatever the chain starts from: the rendered sub-scene of a
/// `Filter` scope, or a copy of the render target behind a `Glass` panel.
struct ChainSeed {
    seed_texture: wgpu::Texture,
    pp_a: wgpu::Texture,
    pp_a_view: wgpu::TextureView,
    pp_b: wgpu::Texture,
    pp_b_view: wgpu::TextureView,
    pp_original: wgpu::Texture,
    pp_original_view: wgpu::TextureView,
}

/// Identifies which internal texture holds the filtered result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilteredSource {
    /// The render texture (fast path, no effects applied).
    Render,
    /// Ping-pong texture A.
    TexA,
    /// Ping-pong texture B.
    TexB,
}

impl GpuFilterBackend {
    /// TEMPORARY stage dump for debugging the multi-scope video loss.
    fn dump_stage(&self, label: &str, texture: &wgpu::Texture, dims: SceneDimensions) {
        if std::env::var_os("ANIMATIX_DUMP_STAGES").is_none() {
            return;
        }
        let img = self
            .readback_to_scene_image_at(texture, wgpu::Origin3d::ZERO, dims)
            .expect("stage dump readback");
        let data = img.data.data.data();
        let n = data.len() / 4;
        let step = (n / 300).max(1);
        let (mut opaque, mut sum_r, mut sum_g, mut sum_b) = (0u32, 0u64, 0u64, 0u64);
        for i in (0..n).step_by(step) {
            let (r, g, b, a) = (
                data[i * 4] as u32,
                data[i * 4 + 1] as u32,
                data[i * 4 + 2] as u32,
                data[i * 4 + 3] as u32,
            );
            if a > 100 {
                opaque += 1;
                sum_r += r as u64;
                sum_g += g as u64;
                sum_b += b as u64;
            }
        }
        let total = (n as u32).div_ceil(step as u32);
        let _ = std::fs::create_dir_all("/tmp/stages");
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        let path = format!("/tmp/stages/{seq:03}_{label}.png");
        #[cfg(feature = "raster-encode")]
        let _ = image::save_buffer(&path, data, dims.width, dims.height, image::ColorType::Rgba8);
        #[cfg(not(feature = "raster-encode"))]
        let _ = (&path, data);
        eprintln!(
            "[stages] {seq:03} {label}: opaque={opaque}/{total} avg_rgb=({},{},{})",
            if opaque > 0 { sum_r / opaque as u64 } else { 0 },
            if opaque > 0 { sum_g / opaque as u64 } else { 0 },
            if opaque > 0 { sum_b / opaque as u64 } else { 0 },
        );
    }

    /// Create a new backend from a cloned device/queue pair.
    ///
    /// # Errors
    /// Returns an error if the inner [`RendererCore`] fails to initialise.
    pub fn new(
        device: wgpu::Device,
        queue: wgpu::Queue,
        dimensions: SceneDimensions,
    ) -> Result<Self, String> {
        let core = RendererCore::new(&device, &queue)
            .map_err(|e| format!("Failed to create filter renderer core: {e}"))?;
        let bytes_per_row = (dimensions.width * 4 + 255) & !255;

        let texture_size = wgpu::Extent3d {
            width: dimensions.width,
            height: dimensions.height,
            depth_or_array_layers: 1,
        };

        // Render target: needs RENDER_ATTACHMENT for Vello + STORAGE_BINDING for compute
        let render_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            label: Some("Animatix Filter Render Texture"),
            view_formats: &[],
        });
        let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let ping_pong_usage = wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST;
        let tex_a = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: ping_pong_usage,
            label: Some("Animatix Filter PingPong A"),
            view_formats: &[],
        });
        let tex_a_view = tex_a.create_view(&wgpu::TextureViewDescriptor::default());

        let tex_b = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: ping_pong_usage,
            label: Some("Animatix Filter PingPong B"),
            view_formats: &[],
        });
        let tex_b_view = tex_b.create_view(&wgpu::TextureViewDescriptor::default());

        let tex_original = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: ping_pong_usage,
            label: Some("Animatix Filter Original"),
            view_formats: &[],
        });
        let tex_original_view = tex_original.create_view(&wgpu::TextureViewDescriptor::default());

        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            size: (bytes_per_row * dimensions.height) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            label: Some("Animatix Filter Output Buffer"),
            mapped_at_creation: false,
        });

        // Fixed layout shared by every effect (docs/effects.md §4.2).
        let bind_group_layout = create_bind_group_layout(&device);
        let pipeline_layout = create_pipeline_layout(&device, &bind_group_layout);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Animatix Effect Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let context_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Animatix Effect Context Uniforms"),
            size: u64::from(CONTEXT_STRIDE * MAX_CHAIN_PASSES),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            core,
            render_texture,
            render_view,
            tex_a,
            tex_a_view,
            tex_b,
            tex_b_view,
            tex_original,
            tex_original_view,
            output_buffer,
            bytes_per_row,
            _dimensions: dimensions,
            raster_scale: 1.0,
            bind_group_layout,
            pipeline_layout,
            pipelines: HashMap::new(),
            sampler,
            context_buffer,
            last_filtered_source: FilteredSource::Render,
            last_effect_dims: dimensions,
            last_region: None,
            pending_composites: Vec::new(),
            region_scratch: HashMap::new(),
            last_region_pp: None,
            pending_texture_pool: RefCell::new(HashMap::new()),
        })
    }

    /// Build (once) the pipelines and uniform buffer for `effect`.
    fn ensure_effect_pipeline(&mut self, effect: &dyn Effect) {
        if self.pipelines.contains_key(effect.type_name()) {
            return;
        }
        let pipeline = create_effect_pipeline(&self.device, &self.pipeline_layout, effect);
        self.pipelines.insert(effect.type_name().into(), pipeline);
    }

    /// Bind group for one pass: views plus the context uniform at `slot`s
    /// dynamic offset in the shared context buffer.
    fn effect_bind_group_for_slot(
        &self,
        src_view: &wgpu::TextureView,
        dst_view: &wgpu::TextureView,
        original_view: &wgpu::TextureView,
        uniform_buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        create_effect_bind_group(
            &self.device,
            &self.bind_group_layout,
            &self.context_buffer,
            &self.sampler,
            src_view,
            dst_view,
            original_view,
            uniform_buffer,
        )
    }

    /// Encode one effect pass into `encoder` (dispatch at `width`×`height`,
    /// context uniform at the bind group's dynamic offset).
    fn encode_effect_pass(
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        pass_pipeline: &wgpu::ComputePipeline,
        width: u32,
        height: u32,
        slot_offset: u32,
    ) {
        pipeline::encode_effect_pass(
            encoder,
            bind_group,
            pass_pipeline,
            width,
            height,
            slot_offset,
        );
    }

    /// Render a scene and apply `chain`, keeping the result on the GPU.
    ///
    /// Returns the [`wgpu::TextureView`] holding the final image. The result
    /// location is recorded on `self`: `last_region_pp` for the region-scoped
    /// path (contents cover `last_effect_dims` from (0, 0)), or
    /// `last_filtered_source` + `last_effect_dims` for the full-canvas path.
    ///
    /// Region-scoped scopes render, seed, and dispatch entirely at region
    /// size, so per-scope cost scales with the scope instead of the canvas.
    /// Dispatch extent == seed extent == the shaders' UV space, which makes
    /// the stale-texel hazard that originally forced full-canvas dispatch (a
    /// cropped seed read through region-independent UVs; found by the
    /// effects-wave1 dogfood when a moving `MotionBlur` card dragged opaque
    /// backdrop garbage with it) impossible by construction. Shaders address
    /// `tex_size`-relative pixels, so pixel-denominated parameters (blur
    /// radius, offsets) keep their meaning at region size.
    fn render_and_filter_scene_to_view(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
    ) -> Result<wgpu::TextureView, String> {
        self.render_and_filter_scene_fused(scene, dimensions, region, chain, None)
            .map(|(v, _)| v)
    }

    fn render_and_filter_scene_fused(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
        harvest_req: Option<PendingHarvestReq>,
    ) -> Result<(wgpu::TextureView, Option<PendingComposite>), String> {
        // The chain follows the raster it is being drawn into. A radius is a
        // distance in scene pixels, so the dispatch extent, the encode of the
        // scope's own sub-scene and every `Pixel` parameter (see
        // `scale_pixel_params`) shrink by the same factor — which is what makes
        // the result *look* the same at a lower raster instead of blurrier.
        let s = self.raster_scale;
        let mut wrapped;
        let (dispatch_dims, scene_ref, scratch) = match region {
            Some(r) => {
                let (width, height) = Self::quantize_region_size(r.size, dimensions);
                let (width, height) = (scaled_texels(width, s), scaled_texels(height, s));
                wrapped = vello::Scene::new();
                let shift = kurbo::Affine::translate(kurbo::Vec2::new(
                    -f64::from(r.origin[0]),
                    -f64::from(r.origin[1]),
                ));
                wrapped.append(
                    scene,
                    Some(if s == 1.0 {
                        shift
                    } else {
                        kurbo::Affine::scale(f64::from(s)) * shift
                    }),
                );
                (
                    SceneDimensions { width, height },
                    &wrapped,
                    Some(self.region_scratch_for(width, height)),
                )
            },
            None if s != 1.0 => {
                let width = scaled_texels(dimensions.width, s);
                let height = scaled_texels(dimensions.height, s);
                wrapped = vello::Scene::new();
                wrapped.append(scene, Some(kurbo::Affine::scale(f64::from(s))));
                (
                    SceneDimensions { width, height },
                    &wrapped,
                    Some(self.region_scratch_for(width, height)),
                )
            },
            None => (dimensions, scene, None),
        };
        let scratch = scratch.map(std::sync::Arc::new);
        let (
            render_view,
            seed_texture,
            pp_a,
            pp_a_view,
            pp_b,
            pp_b_view,
            pp_original,
            pp_original_view,
        ) = match &scratch {
            Some(s) => (
                s.render_view.clone(),
                s.render_texture.clone(),
                s.pp_a.clone(),
                s.pp_a_view.clone(),
                s.pp_b.clone(),
                s.pp_b_view.clone(),
                s.pp_original.clone(),
                s.pp_original_view.clone(),
            ),
            None => (
                self.render_view.clone(),
                self.render_texture.clone(),
                self.tex_a.clone(),
                self.tex_a_view.clone(),
                self.tex_b.clone(),
                self.tex_b_view.clone(),
                self.tex_original.clone(),
                self.tex_original_view.clone(),
            ),
        };

        // TEMPORARY perf probe (env-gated): per-stage cost of one scope.
        let t_all = timing_probe();

        self.core
            .render_vello_scene_with_background(
                &self.device,
                &self.queue,
                &render_view,
                dispatch_dims.width,
                dispatch_dims.height,
                scene_ref,
                vello::peniko::Color::TRANSPARENT,
            )
            .map_err(|e| e.to_string())?;
        let t_render_ms = probe_ms(t_all);

        self.dump_stage("render_view", &seed_texture, dispatch_dims);

        // Harvest extent: the true region size (dispatch may be quantized
        // larger; the padding is never harvested).
        // Texels, not scene pixels: everything that copies out of the textures
        // just rendered reads this, and at a reduced raster they are smaller.
        self.last_effect_dims = match region {
            Some(r) => SceneDimensions {
                width: scaled_texels(r.size.width, s),
                height: scaled_texels(r.size.height, s),
            },
            None => SceneDimensions {
                width: scaled_texels(dimensions.width, s),
                height: scaled_texels(dimensions.height, s),
            },
        };
        self.last_region = region;
        self.last_region_pp = None;

        if chain.is_empty() {
            self.last_filtered_source = FilteredSource::Render;
            if region.is_some() {
                // The unfiltered result lives in the region render target.
                self.last_region_pp = Some((seed_texture.clone(), render_view.clone()));
            }
            return Ok((render_view, None));
        }

        let seed = ChainSeed {
            seed_texture: seed_texture.clone(),
            pp_a: pp_a.clone(),
            pp_a_view: pp_a_view.clone(),
            pp_b: pp_b.clone(),
            pp_b_view: pp_b_view.clone(),
            pp_original: pp_original.clone(),
            pp_original_view: pp_original_view.clone(),
        };
        // "Region scoped" means "the result is in the scratch we were handed",
        // which a reduced raster also is — the full-frame textures are not what
        // the chain just wrote.
        let (view, composite) = self.run_chain_passes_fused(
            seed,
            dispatch_dims,
            chain,
            scratch.is_some(),
            harvest_req,
        )?;
        if t_all.is_some() {
            eprintln!(
                "[filter-timing] dims={}x{} stages={} render={:.2}ms seed+passes={:.2}ms",
                dispatch_dims.width,
                dispatch_dims.height,
                chain.instances.len(),
                t_render_ms,
                probe_ms(t_all) - t_render_ms,
            );
        }
        Ok((view, composite))
    }

    /// Copy the rendered seed into the ping-pong and run every chain pass —
    /// all in ONE command encoder and ONE submit. Same-queue submissions are
    /// ordered, and each pass reads its own context through a dynamic offset,
    /// so the batch is correct and the chain costs one submit instead of one
    /// per stage.
    fn run_chain_passes_fused(
        &mut self,
        seed: ChainSeed,
        dispatch_dims: SceneDimensions,
        chain: &EffectChain,
        region_scoped: bool,
        harvest_req: Option<PendingHarvestReq>,
    ) -> Result<(wgpu::TextureView, Option<PendingComposite>), String> {
        let ChainSeed {
            seed_texture,
            pp_a,
            pp_a_view,
            pp_b,
            pp_b_view,
            pp_original,
            pp_original_view,
        } = seed;
        let width = dispatch_dims.width.max(1);
        let height = dispatch_dims.height.max(1);

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Effect Chain Encoder"),
        });
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &seed_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &pp_a,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.dump_stage("tex_a_seed", &pp_a, dispatch_dims);
        // The original is the *pre-chain* frame: copied here, before pass 0
        // runs, so a bloom-style pass can add onto what the scene looked like
        // rather than onto whatever the previous pass left behind.
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &seed_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &pp_original,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        // Plan the passes, write every context once (one slot per pass), then
        // encode seed + passes into a single encoder.
        // Owns cloned wgpu handles so plan entries never borrow `self` across
        // the mutable pipeline-ensure calls.
        let mut pass_plan: Vec<(wgpu::ComputePipeline, wgpu::Buffer, u32, u32)> = Vec::new();
        let mut current = FilteredSource::TexA;
        for instance in chain.instances.iter().filter(|instance| instance.enabled) {
            let Some(effect) = effect(&instance.id) else {
                tracing::warn!("chain effect has no registered descriptor; skipping");
                continue;
            };
            self.ensure_effect_pipeline(effect.as_effect());
            let Some(pipeline) = self.pipelines.get(effect.type_name()) else {
                continue;
            };

            let mut uniforms = vec![0u8; effect.author_uniform_size() as usize];
            effect.pack(&instance.params, &mut uniforms);
            // A scene-pixel parameter has to shrink with the raster or the
            // effect visually grows as the frame gets smaller: half the pixels
            // with the same radius is a twice-as-wide blur. Done generically off
            // the declared unit rather than per effect, so a new pixel parameter
            // needs only `.pixel()` on its spec to be covered.
            if self.raster_scale != 1.0 {
                scale_pixel_params(effect.params(), self.raster_scale, &mut uniforms);
            }
            // Author-parameter writes land before the encoder's commands on
            // submit; each effect has its own uniform buffer, so writes cannot
            // collide across effects.
            self.queue.write_buffer(&pipeline.uniform_buffer, 0, &uniforms);

            let pass_count = pipeline.passes.len() as u32;
            for (index, pass_pipeline) in pipeline.passes.iter().enumerate() {
                pass_plan.push((
                    pass_pipeline.clone(),
                    pipeline.uniform_buffer.clone(),
                    index as u32,
                    pass_count,
                ));
            }
        }

        let mut context_bytes = vec![0u8; (CONTEXT_STRIDE * MAX_CHAIN_PASSES) as usize];
        for (slot, (pass_pipeline, uniform_buffer, pass_index, pass_count)) in
            pass_plan.iter().enumerate()
        {
            let context = EffectContextUniform {
                tex_size: [width, height],
                _pad0: [0, 0],
                inv_size: [1.0 / width as f32, 1.0 / height as f32],
                _pad1: [0.0, 0.0],
                pass_index: *pass_index,
                pass_count: *pass_count,
                time_ms: chain.time_ms,
                _pad2: 0.0,
            };
            let base = (slot as u32 * CONTEXT_STRIDE) as usize;
            context_bytes[base..base + EFFECT_CONTEXT_SIZE as usize]
                .copy_from_slice(bytemuck::bytes_of(&context));
            let bind_group = self.effect_bind_group_for_slot(
                if matches!(current, FilteredSource::TexA) {
                    &pp_a_view
                } else {
                    &pp_b_view
                },
                if matches!(current, FilteredSource::TexA) {
                    &pp_b_view
                } else {
                    &pp_a_view
                },
                &pp_original_view,
                uniform_buffer,
            );
            Self::encode_effect_pass(
                &mut encoder,
                &bind_group,
                pass_pipeline,
                width,
                height,
                slot as u32 * CONTEXT_STRIDE,
            );
            current = match current {
                FilteredSource::TexA => FilteredSource::TexB,
                _ => FilteredSource::TexA,
            };
        }
        let (result_texture, result_view) = match current {
            FilteredSource::TexA => (pp_a, pp_a_view),
            FilteredSource::TexB => (pp_b, pp_b_view),
            FilteredSource::Render => unreachable!("effects always write a ping-pong texture"),
        };
        let pending_composite = harvest_req.map(|req| {
            self.copy_texture_to_pending_composite(
                &mut encoder,
                &result_texture,
                req.source_origin,
                req.dimensions,
                req.alpha,
                req.origin,
            )
        });
        self.queue.write_buffer(&self.context_buffer, 0, &context_bytes);
        self.queue.submit(std::iter::once(encoder.finish()));

        self.dump_stage("tex_b_fx", &result_texture, dispatch_dims);

        self.last_filtered_source = current;
        if region_scoped {
            // Region results live in the region scratch ping-pong, covering
            // `last_effect_dims` from (0, 0).
            self.last_region_pp = Some((result_texture, result_view.clone()));
        }
        Ok((result_view, pending_composite))
    }

    /// Quantize a region size up to 64-px steps (clamped to the canvas) so a
    /// moving scope reuses one scratch allocation instead of churning.
    fn quantize_region_size(size: SceneDimensions, canvas: SceneDimensions) -> (u32, u32) {
        let step = |v: u32, max: u32| (v.div_ceil(64).max(1) * 64).min(max.max(1));
        (step(size.width, canvas.width), step(size.height, canvas.height))
    }

    /// Get (creating on first use) the region-scoped scratch targets for a
    /// quantized size. Textures are cloned out (refcount bumps) so the cache
    /// stays owned by the backend across the mutable render call.
    fn region_scratch_for(&mut self, width: u32, height: u32) -> RegionScratch {
        const MAX_REGION_SCRATCH_ENTRIES: usize = 12;
        if self.region_scratch.len() > MAX_REGION_SCRATCH_ENTRIES {
            // A scene uses a handful of distinct ROIs; if churn ever exceeds
            // the cap, start over rather than growing unbounded.
            self.region_scratch.clear();
        }
        if let Some(scratch) = self.region_scratch.get(&(width, height)) {
            return scratch.clone();
        }
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST;
        let make = |label: &str| {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage,
                label: Some(label),
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        };
        let (render_texture, render_view) = make("Animatix Region Render Target");
        let (pp_a, pp_a_view) = make("Animatix Region Ping A");
        let (pp_b, pp_b_view) = make("Animatix Region Ping B");
        let (pp_original, pp_original_view) = make("Animatix Region Original");
        let scratch = RegionScratch {
            render_texture,
            render_view,
            pp_a,
            pp_a_view,
            pp_b,
            pp_b_view,
            pp_original,
            pp_original_view,
        };
        self.region_scratch.insert((width, height), scratch.clone());
        scratch
    }

    /// Debug instrumentation: read a texture region back as a [`SceneImage`].
    pub fn debug_readback_texture(
        &self,
        texture: &wgpu::Texture,
        origin: wgpu::Origin3d,
        dims: SceneDimensions,
    ) -> SceneImage {
        self.readback_to_scene_image_at(texture, origin, dims)
            .expect("debug texture readback")
    }

    /// Read a texture back into a [`SceneImage`].
    fn readback_to_scene_image_at(
        &self,
        texture: &wgpu::Texture,
        origin: wgpu::Origin3d,
        dimensions: SceneDimensions,
    ) -> Result<SceneImage, String> {
        // This path blocks: `map_async` hands its result to a channel and
        // `device.poll(Wait)` then drains it. Neither can complete on wasm,
        // where the callback only runs when control returns to the browser event
        // loop — so the first `recv()` hangs the tab outright, and the poisoned
        // device left in the shared context takes every later render in the page
        // down with it. The engine's caller degrades to unfiltered rendering on
        // `Err`, which is the honest outcome in a browser until this becomes
        // async end to end.
        if cfg!(target_arch = "wasm32") {
            return Err(
                "synchronous texture readback cannot complete on wasm; the filtered region is \
                 rendered unfiltered"
                    .into(),
            );
        }

        let output_buffer = &self.output_buffer;

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Filter Readback Encoder"),
        });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin,
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
            .map_err(|err| format!("Failed to map filter frame: {err}"))?;

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

        let data = vello::peniko::ImageData {
            data: rgba.into(),
            format: vello::peniko::ImageFormat::Rgba8,
            alpha_type: vello::peniko::ImageAlphaType::Alpha,
            width: dimensions.width,
            height: dimensions.height,
        };

        Ok(SceneImage {
            data,
            natural_size: [dimensions.width as f32, dimensions.height as f32],
        })
    }

    /// Acquire a pending composite texture from the pool or allocate a new one.
    fn acquire_pending_texture(
        &self,
        dimensions: SceneDimensions,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let key = (dimensions.width, dimensions.height);
        if let Some(entry) =
            self.pending_texture_pool.borrow_mut().get_mut(&key).and_then(|v| v.pop())
        {
            entry
        } else {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                size: wgpu::Extent3d {
                    width: dimensions.width,
                    height: dimensions.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                label: Some("Animatix Pending Filter Composite Texture"),
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            (texture, view)
        }
    }

    /// Record a copy from `source` into a pooled pending composite texture inside `encoder`.
    fn copy_texture_to_pending_composite(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        source_origin: wgpu::Origin3d,
        dimensions: SceneDimensions,
        alpha: f32,
        origin: [f32; 2],
    ) -> PendingComposite {
        let (texture, view) = self.acquire_pending_texture(dimensions);
        let copy_width = dimensions.width.min(source.width().saturating_sub(source_origin.x));
        let copy_height = dimensions.height.min(source.height().saturating_sub(source_origin.y));

        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: source_origin,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: copy_width,
                height: copy_height,
                depth_or_array_layers: 1,
            },
        );

        PendingComposite {
            texture,
            view,
            alpha,
            origin,
            texel_scale: self.raster_scale,
            corner_radius: 0.0,
            src_rect: None,
            clip_rect: None,
        }
    }

    /// Copy the most recent filtered result to a dedicated texture for
    /// deferred compositing.
    fn copy_last_filtered_to_pending(
        &self,
        dimensions: SceneDimensions,
        alpha: f32,
        origin: [f32; 2],
        source: &wgpu::Texture,
        source_origin: wgpu::Origin3d,
    ) -> Result<PendingComposite, String> {
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Animatix Pending Filter Composite Copy Encoder"),
        });
        let composite = self.copy_texture_to_pending_composite(
            &mut encoder,
            source,
            source_origin,
            dimensions,
            alpha,
            origin,
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        Ok(composite)
    }
}

/// Texel count for a scene-pixel extent at raster scale `s`, never zero.
fn scaled_texels(extent: u32, s: f32) -> u32 {
    ((extent as f32 * s).round() as u32).max(1)
}

/// Texel coordinate for a scene-pixel origin at raster scale `s` (can be zero).
fn scaled_origin(coord: f32, s: f32) -> u32 {
    (coord.max(0.0) * s).round() as u32
}

/// Multiply every `Pixel`-unit `f32` lane in a packed author uniform by `scale`.
///
/// `size` is the byte width the spec declared, so a `Vec2` offset covers two
/// lanes. Non-`F32`-sized lanes cannot appear here: the only pixel parameters in
/// the catalog are floats and float pairs.
fn scale_pixel_params(params: &[EffectParamSpec], scale: f32, uniforms: &mut [u8]) {
    for spec in params.iter().filter(|spec| spec.is_pixel()) {
        let start = spec.offset as usize;
        let end = (spec.offset as usize + spec.size as usize).min(uniforms.len());
        for lane in uniforms[start..end].chunks_exact_mut(4) {
            let arr: [u8; 4] = lane.try_into().expect("chunks_exact(4) yields 4 bytes");
            let v = f32::from_le_bytes(arr) * scale;
            lane.copy_from_slice(&v.to_le_bytes());
        }
    }
}

impl FilterBackend for GpuFilterBackend {
    fn set_raster_scale(&mut self, scale: f32) {
        self.raster_scale = scale.clamp(0.05, 1.0);
    }

    fn render_scene_to_image_gpu_filtered(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
    ) -> Result<SceneImage, String> {
        self.render_and_filter_scene_to_view(scene, dimensions, region, chain)?;

        if let Some((texture, _)) = &self.last_region_pp {
            // Region-scoped result: contents cover `last_effect_dims` from
            // (0, 0) inside the scratch texture.
            let t = timing_probe();
            let image = self.readback_to_scene_image_at(
                texture,
                wgpu::Origin3d::ZERO,
                self.last_effect_dims,
            );
            if t.is_some() {
                eprintln!(
                    "[filter-timing] readback dims={}x{} took={:.2}ms",
                    self.last_effect_dims.width,
                    self.last_effect_dims.height,
                    probe_ms(t),
                );
            }
            return image;
        }

        let texture = match self.last_filtered_source {
            FilteredSource::Render => &self.render_texture,
            FilteredSource::TexA => &self.tex_a,
            FilteredSource::TexB => &self.tex_b,
        };
        let (origin, dims) = match self.last_region {
            Some(region) => (
                wgpu::Origin3d {
                    x: scaled_origin(region.origin[0], self.raster_scale),
                    y: scaled_origin(region.origin[1], self.raster_scale),
                    z: 0,
                },
                self.last_effect_dims,
            ),
            None => (wgpu::Origin3d::ZERO, self.last_effect_dims),
        };
        self.readback_to_scene_image_at(texture, origin, dims)
    }

    fn render_scene_to_pending_composite(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        region: Option<EffectRegion>,
        chain: &EffectChain,
        alpha: f32,
    ) -> Result<(), String> {
        // The composite is emitted in target pixels — origin included, because
        // the texture it carries is `raster_scale` of a scene pixel wide. The
        // drain divides its frame scale by `texel_scale` to land the whole thing
        // without shrinking it a second time.
        let origin = region.map_or([0.0, 0.0], |region| {
            [
                region.origin[0] * self.raster_scale,
                region.origin[1] * self.raster_scale,
            ]
        });
        let t_copy = timing_probe();

        if chain.is_empty() {
            self.render_and_filter_scene_to_view(scene, dimensions, region, chain)?;
            let harvest = self.last_effect_dims;
            let (source, source_origin) = if let Some((texture, _)) = &self.last_region_pp {
                (texture, wgpu::Origin3d::ZERO)
            } else {
                let texture = match self.last_filtered_source {
                    FilteredSource::Render => &self.render_texture,
                    FilteredSource::TexA => &self.tex_a,
                    FilteredSource::TexB => &self.tex_b,
                };
                let source_origin = match self.last_region {
                    Some(region) => wgpu::Origin3d {
                        x: scaled_origin(region.origin[0], self.raster_scale),
                        y: scaled_origin(region.origin[1], self.raster_scale),
                        z: 0,
                    },
                    None => wgpu::Origin3d::ZERO,
                };
                (texture, source_origin)
            };
            let composite =
                self.copy_last_filtered_to_pending(harvest, alpha, origin, source, source_origin)?;
            self.pending_composites.push(PendingLayer::Composite(composite));
            return Ok(());
        }

        // When chain has effect passes, fuse compute pass encoding and pending composite copying:
        let harvest = match region {
            Some(r) => SceneDimensions {
                width: scaled_texels(r.size.width, self.raster_scale),
                height: scaled_texels(r.size.height, self.raster_scale),
            },
            None => SceneDimensions {
                width: scaled_texels(dimensions.width, self.raster_scale),
                height: scaled_texels(dimensions.height, self.raster_scale),
            },
        };
        let harvest_req = PendingHarvestReq {
            dimensions: harvest,
            alpha,
            origin,
            source_origin: wgpu::Origin3d::ZERO,
        };
        let (_view, composite) = self.render_and_filter_scene_fused(
            scene,
            dimensions,
            region,
            chain,
            Some(harvest_req),
        )?;
        if let Some(comp) = composite {
            self.pending_composites.push(PendingLayer::Composite(comp));
        }
        if t_copy.is_some() {
            eprintln!(
                "[filter-timing] pending dims={}x{} render+chain+fused_copy={:.2}ms",
                harvest.width,
                harvest.height,
                probe_ms(t_copy),
            );
        }
        Ok(())
    }

    fn take_pending_composites(&mut self) -> Vec<PendingLayer> {
        std::mem::take(&mut self.pending_composites)
    }

    fn recycle_pending_composite(&mut self, composite: PendingComposite) {
        let size = composite.texture.size();
        let key = (size.width, size.height);
        let mut pool = self.pending_texture_pool.borrow_mut();
        let list = pool.entry(key).or_default();
        if list.len() < 8 {
            list.push((composite.texture, composite.view));
        }
    }

    /// Queue the panel's own children as a plain composite: the scene renders
    /// into the region scratch with a transparent clear and an empty chain, then
    /// the copy is queued. It lands *after* whatever is already in the queue, so
    /// a `Glass` scope's children composite above its backdrop.
    fn enqueue_scene_composite(
        &mut self,
        scene: &vello::Scene,
        dimensions: SceneDimensions,
        origin: [f32; 2],
        alpha: f32,
    ) -> Result<(), String> {
        let view =
            self.render_and_filter_scene_to_view(scene, dimensions, None, &EffectChain::default())?;
        let harvest = self.last_effect_dims;
        let source = match self.last_filtered_source {
            FilteredSource::Render => &self.render_texture,
            FilteredSource::TexA => &self.tex_a,
            FilteredSource::TexB => &self.tex_b,
        };
        let composite = self.copy_last_filtered_to_pending(
            harvest,
            alpha,
            origin,
            source,
            wgpu::Origin3d::ZERO,
        )?;
        let _ = view;
        self.pending_composites.push(PendingLayer::Composite(composite));
        Ok(())
    }

    fn enqueue_backdrop(&mut self, backdrop: &PendingBackdrop) -> Result<(), String> {
        self.pending_composites.push(PendingLayer::Backdrop(backdrop.clone()));
        Ok(())
    }

    /// Blur the panel's own slice of the finished frame.
    ///
    /// Runs at drain time because the pixels only exist after the main scene
    /// render: the target's contents are copied into the region scratch (which
    /// `Filter` scopes already own and reuse), run through the scope's chain, and
    /// handed back as a composite clipped to the panel's rect and corners.
    fn run_backdrop(
        &mut self,
        target: &wgpu::Texture,
        backdrop: &PendingBackdrop,
    ) -> Result<PendingComposite, String> {
        let size = target.size();
        let canvas = SceneDimensions {
            width: size.width,
            height: size.height,
        };
        let region = backdrop.region;
        let (w, h) = Self::quantize_region_size(region.size, canvas);
        let dispatch = SceneDimensions {
            width: w,
            height: h,
        };
        let scratch = self.region_scratch_for(w, h);

        // Clear the seed first. The copy below writes only the panel's rect; the
        // quantized padding ring would otherwise hold the previous frame's
        // texels, and a blur reaching into them smears that garbage inward.
        self.core
            .render_vello_scene_with_background(
                &self.device,
                &self.queue,
                &scratch.render_view,
                w,
                h,
                &vello::Scene::new(),
                vello::peniko::Color::TRANSPARENT,
            )
            .map_err(|e| e.to_string())?;

        // The copy has to stay inside both textures. The region is already
        // clamped to the canvas by `region_from_rect`, but its quantized dispatch
        // size is rounded *up*, so the padded region can hang off the right or
        // bottom edge.
        let ox = (region.origin[0].max(0.0).round() as u32).min(canvas.width);
        let oy = (region.origin[1].max(0.0).round() as u32).min(canvas.height);
        let copy_w = region.size.width.min(w).min(canvas.width - ox);
        let copy_h = region.size.height.min(h).min(canvas.height - oy);
        if copy_w > 0 && copy_h > 0 {
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Animatix Backdrop Copy Encoder"),
            });
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: target,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: ox, y: oy, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &scratch.render_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: copy_w,
                    height: copy_h,
                    depth_or_array_layers: 1,
                },
            );
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        let seed = ChainSeed {
            seed_texture: scratch.render_texture.clone(),
            pp_a: scratch.pp_a.clone(),
            pp_a_view: scratch.pp_a_view.clone(),
            pp_b: scratch.pp_b.clone(),
            pp_b_view: scratch.pp_b_view.clone(),
            pp_original: scratch.pp_original.clone(),
            pp_original_view: scratch.pp_original_view.clone(),
        };
        self.last_effect_dims = region.size;
        self.last_region = Some(region);
        self.last_region_pp = None;
        let harvest_req = PendingHarvestReq {
            dimensions: region.size,
            alpha: backdrop.alpha,
            origin: backdrop.clip.origin,
            source_origin: wgpu::Origin3d::ZERO,
        };
        let (_view, composite_opt) =
            self.run_chain_passes_fused(seed, dispatch, &backdrop.chain, true, Some(harvest_req))?;
        let mut composite =
            composite_opt.ok_or_else(|| "backdrop chain produced no composite".to_string())?;

        // The panel rect lives *inside* the padded result: the composite samples
        // just that window and paints it back where it came from, shaped by the
        // panel's own corners.
        let px0 = (backdrop.clip.origin[0] - region.origin[0]).max(0.0);
        let py0 = (backdrop.clip.origin[1] - region.origin[1]).max(0.0);
        let px1 = px0 + backdrop.clip.size.width as f32;
        let py1 = py0 + backdrop.clip.size.height as f32;
        composite.src_rect = Some([px0, py0, px1, py1]);
        composite.clip_rect = Some([
            backdrop.clip.origin[0],
            backdrop.clip.origin[1],
            backdrop.clip.origin[0] + backdrop.clip.size.width as f32,
            backdrop.clip.origin[1] + backdrop.clip.size.height as f32,
        ]);
        composite.corner_radius = backdrop.corner_radius;
        Ok(composite)
    }
}
