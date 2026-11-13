//! Shared export types and audio muxing.
//!
//! This module defines:
//! - [`ExportError`] — unified error type for all export paths
//! - [`ExportOptions`], [`ExportSettings`], [`MaxRenderThreads`], [`VideoCodec`],
//!   [`H264Preset`] — configuration, progress and cancellation
//! - `mux_audio_segments` — ffmpeg CLI-based audio integration
//!
//! Sub-modules contain the actual encoding implementations for each format.
//! Each format exposes one `*_with_options` entry point per target shape plus a
//! convenience wrapper that defaults the debug options and [`ExportOptions`].
//!
//! The error type's trait implementations live in `export_error`, the config
//! types' implementations in `impls`, and `ffmpeg` discovery plus audio muxing
//! in `ffmpeg`; this file only declares them and holds the type definitions.

#[cfg(feature = "video")]
pub mod gif;
#[cfg(feature = "raster-encode")]
pub mod image;
#[cfg(feature = "raster-encode")]
pub mod storyboard;
#[cfg(feature = "video")]
pub mod video;

mod export_error;
mod ffmpeg;
mod impls;
#[cfg(feature = "raster-encode")]
pub mod raster;
#[cfg(test)]
mod tests;

#[cfg(feature = "video")]
pub use gif::{
    render_gif_composition, render_gif_composition_with_options, render_gif_timeline,
    render_gif_timeline_with_options,
};
#[cfg(feature = "raster-encode")]
pub use image::{
    render_image_composition, render_image_timeline, render_image_timeline_with_options,
};
#[cfg(feature = "raster-encode")]
pub use storyboard::{
    StoryboardSettings, render_storyboard_composition, render_storyboard_timeline,
};

#[cfg(feature = "video")]
pub use video::{
    render_video_composition, render_video_composition_with_options, render_video_timeline,
    render_video_timeline_with_options,
};

pub use export_error::ExportError;
pub use ffmpeg::{is_ffmpeg_available, mux_audio_segments, require_ffmpeg};

use std::sync::atomic::{AtomicBool, AtomicU32};

/// Export configuration settings.
#[derive(Debug, Clone, Copy)]
pub struct ExportSettings {
    /// Render thread limit. `Auto` picks based on format, resolution, and duration.
    pub max_render_threads: MaxRenderThreads,
    /// Video encoder selection. `Auto` probes hardware first.
    pub video_codec: VideoCodec,
    /// libx264 quality-speed preset. Ignored for hardware encoders.
    pub h264_preset: H264Preset,
}

/// Everything an export needs besides the source, geometry and output path.
///
/// `progress` and `cancel` are borrowed from the caller's worker state: the GUI
/// shares them with the UI thread, the CLI passes `None`. `None` means the
/// export does not observe that channel, not that a default one is created.
#[derive(Debug, Clone, Copy, Default)]
pub struct ExportOptions<'a> {
    /// Encoder and thread configuration.
    pub settings: ExportSettings,
    /// Number of frames completed so far, updated by the render threads.
    pub progress: Option<&'a AtomicU32>,
    /// Set by the caller to stop the export; the frame loop checks it per frame.
    pub cancel: Option<&'a AtomicBool>,
}

/// A named resolution/fps/codec preset shared by CLI and GUI export paths.
#[derive(Debug, Clone, Copy)]
pub struct ExportPreset {
    /// Stable lowercase preset name, e.g. `"720p30"`.
    pub name: &'static str,
    /// Output width in pixels.
    pub width: u32,
    /// Output height in pixels.
    pub height: u32,
    /// Output framerate.
    pub fps: u32,
    /// Encoder selection.
    pub video_codec: VideoCodec,
    /// libx264 speed preset.
    pub h264_preset: H264Preset,
}

/// Render thread limit for exports.
#[derive(Debug, Clone, Copy)]
pub enum MaxRenderThreads {
    /// Automatically choose thread count based on workload.
    Auto,
    /// Use a fixed number of threads.
    Fixed(usize),
}

/// Video encoder selection for MP4/WebM exports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    /// Software H.264 (`libx264`) — the portable default. `find_encoder` only
    /// proves a hardware encoder *exists* in the FFmpeg build, not that it can
    /// initialize (h264_nvenc registers fine and then fails on machines without
    /// CUDA), so "auto" means the encoder that always works.
    Auto,
    /// Software H.264 encoder (libx264).
    Libx264,
    /// NVIDIA hardware H.264 encoder (h264_nvenc).
    H264Nvenc,
    /// VAAPI hardware H.264 encoder (h264_vaapi).
    H264Vaapi,
    /// Software VP9 encoder (libvpx-vp9) for WebM.
    Vp9,
}

/// libx264 quality-speed preset.
///
/// Slower presets produce smaller files at the cost of encoding time.
/// Ignored for hardware encoders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum H264Preset {
    /// Fastest, largest file size.
    Ultrafast,
    /// Very fast encode.
    Superfast,
    /// Fast encode.
    Veryfast,
    /// Slightly faster than default.
    Faster,
    /// Fast preset.
    Fast,
    /// Default balance of speed and quality.
    Medium,
    /// Slower, better compression.
    Slow,
    /// Even slower, better compression.
    Slower,
    /// Slowest, best compression.
    Veryslow,
}
