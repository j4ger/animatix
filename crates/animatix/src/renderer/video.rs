//! Re-export module — delegates to sub-modules for backward compatibility.
//!
//! All public items are now defined in:
//! - [`super::encode`] — `ExportError`, `ExportSettings`, `MaxRenderThreads`, `VideoCodec`,
//!   `H264Preset`, and format-specific encoding functions
//! - [`super::render_pipeline`] — streaming frame rendering helpers
//!
//! Callers using `animatix::renderer::video::*` continue to work unchanged.

pub use super::encode::{
    ExportError, ExportSettings, H264Preset, MaxRenderThreads, VideoCodec, mux_audio_segments,
    render_gif_composition, render_gif_composition_with_progress,
    render_gif_composition_with_settings, render_gif_timeline, render_gif_timeline_with_debug,
    render_gif_timeline_with_progress, render_gif_timeline_with_settings, render_image,
    render_image_composition, render_image_timeline, render_image_timeline_with_debug,
    render_image_timeline_with_progress, render_video, render_video_composition,
    render_video_composition_with_progress, render_video_composition_with_settings,
    render_video_timeline, render_video_timeline_with_debug, render_video_timeline_with_progress,
    render_video_timeline_with_settings,
};
pub use super::render_pipeline::fill_rgba_frame;
