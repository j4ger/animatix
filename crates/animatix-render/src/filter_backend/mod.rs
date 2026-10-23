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

mod backend;
mod readback;
mod render;
mod trait_impl;
