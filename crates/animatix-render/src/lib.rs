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
#[cfg(feature = "vello-gpu")]
pub mod effects_gpu;
pub mod encode;
pub mod filter_backend;
#[cfg(feature = "vello-gpu")]
pub mod frame_gpu;
pub mod fullscreen_blit;
pub mod instanced_line;
mod legacy_scene;
pub mod offscreen;
#[cfg(feature = "vello-gpu")]
pub mod raster_gpu;
#[cfg(feature = "video")]
pub mod render_pipeline;
#[cfg(test)]
pub(crate) mod test_support;
pub mod testing;
pub mod transition;
