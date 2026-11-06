//! GPU-backed offscreen frame rendering.

use animatix::timeline::effects::FilterBackend;
use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};

use super::core::RendererCore;
use super::filter_backend::GpuFilterBackend;
use super::transition::TransitionCompositor;

mod raster;
mod renderer;
#[cfg(test)]
mod tests;
mod timeline_render;
mod transitions;

pub use raster::{drain_pending_layers, rasterize_scene};
pub(crate) use timeline_render::render_timeline_into_view;

/// A single frame rendered to CPU-accessible RGBA memory.
///
/// The pixel buffer is shared (`Arc`): the renderer keeps its own handle to
/// the previous frame's allocation and reuses it in place on the next
/// readback (`Arc::make_mut` is a no-op clone once the encoder drops its
/// reference), so a steady-state video export performs zero per-frame
/// allocations for the multi-megabyte readback (a 720p RGBA frame is
/// 3.7 MB — 93% of the export path's churn).

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
    /// Two rotating MAP_READ buffers for the pipelined readback: while
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
    /// Recycled CPU readback buffer: parked between frames and
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

/// How a scene is mapped onto its raster target.
///
/// Vello draws a scene 1:1 — `RenderParams` carries no transform in the pinned
/// revision — so a target smaller than the scene is produced by re-encoding the
/// scene with an affine scale. `append` is the only place that transform can be
/// applied, and it costs one O(paths) copy against a per-frame fill-rate cost
/// that is several times larger (see `docs/performance_evaluation.md`, "Web
/// playback").
pub enum RasterFit<'a> {
    /// Scene units are target pixels: draw the scene exactly as encoded.
    OneToOne,
    /// Re-encode the scene at `scale` into `scratch` (reused across frames) and
    /// draw that.
    Scaled {
        /// Target pixels per scene pixel.
        scale: f32,
        /// Re-encode buffer, reset by the rasterizer.
        scratch: &'a mut vello::Scene,
    },
}
