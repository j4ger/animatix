//! Rendering pipeline: offscreen frames, windowed preview, transitions, and exports.

/// Error types for rendering operations.
pub mod error;
/// GPU-based compositor for scene transition effects.
pub mod transition;
/// Shared rendering types.
pub mod types;

#[cfg(feature = "render")]
/// Core Vello renderer wrapper.
pub mod core;
#[cfg(feature = "render")]
/// Offscreen renderer for CPU-readable frame output.
pub mod offscreen;
#[cfg(feature = "render")]
/// Shared GPU filter backend for preview and export.
pub mod filter_backend;
#[cfg(feature = "text")]
/// Text rendering support.
pub mod text;
#[cfg(feature = "video")]
/// Video/GIF export rendering.
pub mod video;
#[cfg(feature = "render")]
/// High-level render pipeline orchestration.
pub mod render_pipeline;
#[cfg(feature = "video")]
/// Video encoding helpers.
pub mod encode;
#[cfg(feature = "render")]
/// Windowed live-preview renderer.
pub mod window;

#[cfg(feature = "render")]
pub use offscreen::{OffscreenRenderer, RenderedFrame};
pub use transition::TransitionCompositor;

#[cfg(feature = "video")]
pub use video::{
    render_gif_composition, render_gif_composition_with_settings,
    render_gif_composition_with_progress,
    render_gif_timeline, render_gif_timeline_with_debug, render_gif_timeline_with_settings,
    render_gif_timeline_with_progress,
    render_image, render_image_composition, render_image_timeline,
    render_image_timeline_with_debug,
    render_image_timeline_with_progress,
    render_video, render_video_composition, render_video_composition_with_settings,
    render_video_composition_with_progress,
    render_video_timeline, render_video_timeline_with_debug, render_video_timeline_with_settings,
    render_video_timeline_with_progress,
    ExportSettings, H264Preset, MaxRenderThreads, VideoCodec,
};

#[cfg(feature = "render")]
pub use window::{run, run_composition_with_options, run_timeline, run_timeline_with_options};
