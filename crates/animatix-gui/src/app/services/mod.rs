//! Service abstractions that decouple the shell from WGPU/Audio backends.
//!
//! These traits are implemented by eframe-specific adapters and mocked
//! for tests, allowing the shell to be tested without GPU/audio hardware.

pub(crate) mod renderer;
pub(crate) mod audio;