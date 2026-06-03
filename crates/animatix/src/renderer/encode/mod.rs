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

/// Unified error type for all export/encoding operations.
#[derive(Debug)]
pub enum ExportError {
    /// Renderer initialization failed.
    RendererCreation(String),
    /// A specific frame failed to render.
    FrameRender {
        /// Frame index that failed.
        frame: usize,
        /// Error message describing the failure.
        message: String,
    },
    /// Image encoding failed (e.g. buffer creation).
    ImageEncode(String),
    /// Failed to write image file to disk.
    ImageSave(std::io::Error),
    /// Video encoding failed (ffmpeg/rsmpeg error).
    VideoEncode(String),
    /// GIF encoding failed.
    GifEncode(String),
    /// Output path contains invalid characters (null bytes).
    InvalidPath(std::ffi::NulError),
    /// A render thread panicked during export.
    ThreadPanicked,
    /// Export was cancelled by the user.
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

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            max_render_threads: MaxRenderThreads::Auto,
            video_codec: VideoCodec::Auto,
            h264_preset: H264Preset::Medium,
        }
    }
}

/// Render thread limit for exports.
#[derive(Debug, Clone, Copy)]
pub enum MaxRenderThreads {
    /// Automatically choose thread count based on workload.
    Auto,
    /// Use a fixed number of threads.
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

/// Video encoder selection for MP4 exports.
#[derive(Debug, Clone, Copy)]
pub enum VideoCodec {
    /// Auto-detect: try hardware encoders first, fall back to libx264.
    Auto,
    /// Software H.264 encoder (libx264).
    Libx264,
    /// NVIDIA hardware H.264 encoder (h264_nvenc).
    H264Nvenc,
    /// VAAPI hardware H.264 encoder (h264_vaapi).
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

/// libx264 quality-speed preset.
///
/// Slower presets produce smaller files at the cost of encoding time.
/// Ignored for hardware encoders.
#[derive(Debug, Clone, Copy)]
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

impl H264Preset {
    /// Returns the ffmpeg preset name.
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
///
/// Each segment is positioned on the global timeline via `adelay`, optionally
/// trimmed to `duration_s` via `atrim`, and its volume scaled.  All segments
/// are then mixed together with `amix` so overlapping audio (e.g. background
/// music + voiceover) blends correctly.
///
/// Creates a temporary file and renames it on success.
pub fn mux_audio_segments(
    video_path: &std::path::Path,
    segments: &[crate::timeline::AudioSegment],
    output_path: &std::path::Path,
) -> Result<(), ExportError> {
    if segments.is_empty() {
        return Ok(());
    }

    let temp_path = output_path.with_extension("tmp_muxed.mp4");

    // Build a filter_complex that positions, trims, and volumes each segment,
    // then mixes them all together.
    let mut filter_chains: Vec<String> = Vec::with_capacity(segments.len());
    let mut mix_labels: Vec<String> = Vec::with_capacity(segments.len());

    for (i, seg) in segments.iter().enumerate() {
        let input_idx = i + 1; // 0 is the video file
        let label = format!("[a{i}]");
        let delay_ms = (seg.start_time_s * 1000.0).round() as i64;
        let delay_ms = delay_ms.max(0);

        let mut chain = format!("[{input_idx}:a:0]");

        // Trim to declared duration if specified.
        if seg.duration_s > 0.0 {
            chain.push_str(&format!("atrim=end={:.3},", seg.duration_s));
        }

        // Apply per-segment volume.
        chain.push_str(&format!("volume={:.3},", seg.volume));

        // Delay to global timeline position.
        chain.push_str(&format!("adelay={delay_ms}|{delay_ms}"));

        chain.push_str(&label);
        filter_chains.push(chain);
        mix_labels.push(label);
    }

    let mix_filter = format!(
        "{}amix=inputs={}:duration=longest:normalize=0[outa]",
        mix_labels.join(""),
        segments.len()
    );
    filter_chains.push(mix_filter);
    let filter_complex = filter_chains.join(";");

    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.arg("-y");
    cmd.arg("-i").arg(video_path);
    for seg in segments {
        cmd.arg("-i").arg(&seg.source);
    }
    cmd.arg("-filter_complex").arg(&filter_complex);
    cmd.arg("-map").arg("0:v:0");
    cmd.arg("-map").arg("[outa]");
    cmd.arg("-c:v").arg("copy");
    cmd.arg("-c:a").arg("aac");
    cmd.arg("-shortest");
    cmd.arg(&temp_path);

    tracing::debug!("ffmpeg audio mux command: {:?}", cmd);

    let status = cmd
        .status()
        .map_err(|e| ExportError::VideoEncode(format!("Failed to run ffmpeg for audio muxing: {e}")))?;

    if !status.success() {
        let _ = std::fs::remove_file(&temp_path);
        return Err(ExportError::VideoEncode(
            "ffmpeg audio muxing failed".into(),
        ));
    }

    // Replace original with muxed version
    std::fs::rename(&temp_path, output_path)
        .map_err(|e| ExportError::VideoEncode(format!("Failed to replace video with muxed version: {e}")))?;

    tracing::info!("Audio muxed {} segment(s) into {}", segments.len(), output_path.display());
    Ok(())
}