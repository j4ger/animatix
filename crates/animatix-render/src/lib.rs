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
pub mod encode;
pub mod filter_backend;
pub mod fullscreen_blit;
pub mod offscreen;
#[cfg(feature = "video")]
pub mod render_pipeline;
pub mod transition;
#[cfg(feature = "video")]
pub mod video;
