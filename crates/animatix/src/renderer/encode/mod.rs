//! Shared export types and audio muxing.
//!
//! This module defines:
//! - [`ExportError`] — unified error type for all export paths
//! - [`ExportSettings`], [`MaxRenderThreads`], [`VideoCodec`], [`H264Preset`] — configuration
//! - [`mux_audio_segments`] — ffmpeg CLI-based audio integration
//!
//! Sub-modules contain the actual encoding implementations for each format.

pub mod gif;
pub mod image;
pub mod video;

pub use self::video::{
    render_video, render_video_composition, render_video_composition_with_progress,
    render_video_composition_with_settings, render_video_timeline,
    render_video_timeline_with_debug, render_video_timeline_with_progress,
    render_video_timeline_with_settings,
};
pub use gif::{
    render_gif_composition, render_gif_composition_with_progress,
    render_gif_composition_with_settings, render_gif_timeline,
    render_gif_timeline_with_debug, render_gif_timeline_with_progress,
    render_gif_timeline_with_settings,
};
pub use image::{
    render_image, render_image_composition, render_image_timeline,
    render_image_timeline_with_debug, render_image_timeline_with_progress,
};
// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum ExportError {
    RendererCreation(String),
    FrameRender { frame: usize, message: String },
    ImageEncode(String),
    ImageSave(std::io::Error),
    VideoEncode(String),
    GifEncode(String),
    InvalidPath(std::ffi::NulError),
    ThreadPanicked,
    Cancelled,
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RendererCreation(msg) => write!(f, "Failed to create renderer: {msg}"),
            Self::FrameRender { frame, message } => {
                write!(f, "Failed to render frame {frame}: {message}")
            }
            Self::ImageEncode(msg) => write!(f, "Image encoding error: {msg}"),
            Self::ImageSave(err) => write!(f, "Failed to save image: {err}"),
            Self::VideoEncode(msg) => write!(f, "Video encoding error: {msg}"),
            Self::GifEncode(msg) => write!(f, "GIF encoding error: {msg}"),
            Self::InvalidPath(_) => write!(f, "Output path contains null bytes"),
            Self::ThreadPanicked => write!(f, "Render thread panicked"),
            Self::Cancelled => write!(f, "Export cancelled by user"),
        }
    }
}

impl std::error::Error for ExportError {}

impl From<std::io::Error> for ExportError {
    fn from(err: std::io::Error) -> Self {
        Self::ImageSave(err)
    }
}

impl From<std::ffi::NulError> for ExportError {
    fn from(err: std::ffi::NulError) -> Self {
        Self::InvalidPath(err)
    }
}

// ---------------------------------------------------------------------------
// Export settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct ExportSettings {
    /// Render thread limit. `Auto` picks based on format, resolution, and duration.
    pub max_render_threads: MaxRenderThreads,
    /// Video encoder selection. `Auto` probes hardware first.
    pub video_codec: VideoCodec,
    /// libx264 quality-speed preset. Ignored for hardware encoders.
    pub h264_preset: H264Preset,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            max_render_threads: MaxRenderThreads::Auto,
            video_codec: VideoCodec::Auto,
            h264_preset: H264Preset::Medium,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MaxRenderThreads {
    Auto,
    Fixed(usize),
}

impl std::fmt::Display for MaxRenderThreads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Fixed(n) => write!(f, "{n}"),
        }
    }
}

impl std::str::FromStr for MaxRenderThreads {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("auto") {
            Ok(Self::Auto)
        } else {
            let n = s.parse::<usize>().map_err(|e| format!("Invalid thread count: {e}"))?;
            Ok(Self::Fixed(n))
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum VideoCodec {
    Auto,
    Libx264,
    H264Nvenc,
    H264Vaapi,
}

impl std::fmt::Display for VideoCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "auto"),
            Self::Libx264 => write!(f, "libx264"),
            Self::H264Nvenc => write!(f, "h264_nvenc"),
            Self::H264Vaapi => write!(f, "h264_vaapi"),
        }
    }
}

impl std::str::FromStr for VideoCodec {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "libx264" => Ok(Self::Libx264),
            "h264_nvenc" | "nvenc" => Ok(Self::H264Nvenc),
            "h264_vaapi" | "vaapi" => Ok(Self::H264Vaapi),
            _ => Err(format!(
                "Unknown codec: {s}. Expected: auto, libx264, h264_nvenc, h264_vaapi"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum H264Preset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    Medium,
    Slow,
    Slower,
    Veryslow,
}

impl H264Preset {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ultrafast => "ultrafast",
            Self::Superfast => "superfast",
            Self::Veryfast => "veryfast",
            Self::Faster => "faster",
            Self::Fast => "fast",
            Self::Medium => "medium",
            Self::Slow => "slow",
            Self::Slower => "slower",
            Self::Veryslow => "veryslow",
        }
    }
}

impl std::fmt::Display for H264Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for H264Preset {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ultrafast" => Ok(Self::Ultrafast),
            "superfast" => Ok(Self::Superfast),
            "veryfast" => Ok(Self::Veryfast),
            "faster" => Ok(Self::Faster),
            "fast" => Ok(Self::Fast),
            "medium" => Ok(Self::Medium),
            "slow" => Ok(Self::Slow),
            "slower" => Ok(Self::Slower),
            "veryslow" => Ok(Self::Veryslow),
            _ => Err(format!(
                "Unknown preset: {s}. Expected: ultrafast, superfast, veryfast, faster, fast, medium, slow, slower, veryslow"
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Audio muxing via ffmpeg CLI
// ---------------------------------------------------------------------------

/// Mux audio segments into the rendered video using ffmpeg CLI.
/// Creates a temporary file and renames it on success.
///
/// For MVP, only a single audio segment is supported. Multiple segments
/// are concatenated via ffmpeg's concat filter.
pub fn mux_audio_segments(
    video_path: &std::path::Path,
    segments: &[crate::timeline::AudioSegment],
    output_path: &std::path::Path,
) -> Result<(), ExportError> {
    if segments.is_empty() {
        return Ok(());
    }

    let temp_path = output_path.with_extension("tmp_muxed.mp4");

    // For MVP: support a single audio file
    if segments.len() == 1 {
        let seg = &segments[0];
        let status = std::process::Command::new("ffmpeg")
            .arg("-y")
            .arg("-i")
            .arg(video_path)
            .arg("-i")
            .arg(&seg.source)
            .arg("-c:v").arg("copy")
            .arg("-c:a").arg("aac")
            .arg("-map").arg("0:v:0")
            .arg("-map").arg("1:a:0")
            .arg("-shortest")
            .arg(&temp_path)
            .status()
            .map_err(|e| ExportError::VideoEncode(format!("Failed to run ffmpeg for audio muxing: {e}")))?;

        if !status.success() {
            let _ = std::fs::remove_file(&temp_path);
            return Err(ExportError::VideoEncode(
                "ffmpeg audio muxing failed".into(),
            ));
        }
    } else {
        // Multiple audio segments: use concat filter
        let mut filter_parts: Vec<String> = Vec::new();
        let mut inputs_args: Vec<std::ffi::OsString> = Vec::new();
        for (i, seg) in segments.iter().enumerate() {
            inputs_args.push(std::ffi::OsString::from("-i"));
            inputs_args.push(std::ffi::OsString::from(&seg.source));
            filter_parts.push(format!("[{}:a:0]", i + 1));
        }
        let filter_desc = format!("{}concat=n={}:v=0:a=1[outa]", filter_parts.join(""), segments.len());

        let mut cmd = std::process::Command::new("ffmpeg");
        cmd.arg("-y");
        cmd.arg("-i").arg(video_path);
        for arg in inputs_args {
            cmd.arg(arg);
        }
        cmd.arg("-filter_complex").arg(&filter_desc);
        cmd.arg("-map").arg("0:v:0");
        cmd.arg("-map").arg("[outa]");
        cmd.arg("-c:v").arg("copy");
        cmd.arg("-shortest");
        cmd.arg(&temp_path);

        let status = cmd
            .status()
            .map_err(|e| ExportError::VideoEncode(format!("Failed to run ffmpeg for audio muxing: {e}")))?;

        if !status.success() {
            let _ = std::fs::remove_file(&temp_path);
            return Err(ExportError::VideoEncode(
                "ffmpeg audio muxing failed".into(),
            ));
        }
    }

    // Replace original with muxed version
    std::fs::rename(&temp_path, output_path)
        .map_err(|e| ExportError::VideoEncode(format!("Failed to replace video with muxed version: {e}")))?;

    tracing::info!("Audio muxed into {}", output_path.display());
    Ok(())
}