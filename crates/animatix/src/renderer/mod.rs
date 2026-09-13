//! Frame-evaluation rendering vocabulary.
//!
//! This module holds the *engine-owned* half of rendering: the error type
//! every primitive's `evaluate` returns, and the path/color value types that
//! appear in render commands. The GPU presentation stack (renderer core,
//! filter backend implementation, offscreen driver, blit, transition,
//! encoders) lives in the separate `animatix-render` crate — the engine never
//! references it, because the `FilterBackend` trait in `animatix::timeline`
//! is the seam.

/// Error types for rendering operations.
pub mod error;
/// Shared rendering types (paths and colors that cross the seam).
pub mod types;

pub use animatix_text as text;
