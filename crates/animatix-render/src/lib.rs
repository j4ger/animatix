//! GPU presentation layer: the Vello/wgpu renderer core, the filter backend
//! implementation, offscreen frame rendering, transition compositing, and
//! export encoders.
//!
//! One-way dependency: this crate drives the engine's public API
//! (`Timeline::evaluate`, the `FilterBackend` trait it defines, composition
//! types) and turns the produced scenes into pixels/video. The engine never
//! references this crate — the `FilterBackend` trait in `animatix::timeline`
//! is the seam.

pub mod core;
#[cfg(all(feature = "raster-encode", not(target_arch = "wasm32")))]
pub mod delivery;
pub mod effects_gpu;
pub mod encode;
mod filter_region;
pub mod frame_gpu;
pub mod fullscreen_blit;
pub mod instanced_line;
pub mod offscreen;
mod probe;
pub mod raster_gpu;
#[cfg(feature = "video")]
pub mod render_pipeline;
#[cfg(test)]
pub(crate) mod test_support;
pub mod testing;
pub mod transition;
#[cfg(not(target_arch = "wasm32"))]
pub mod video_assets;
