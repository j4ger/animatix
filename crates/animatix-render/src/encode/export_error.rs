//! [`ExportError`] — the unified error type for all export paths, and its
//! trait implementations.

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
    /// ffmpeg binary not found on PATH.
    FfmpegNotFound,
    /// Internal export error (e.g. target/format mismatch).
    Internal(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RendererCreation(msg) => write!(f, "Failed to create renderer: {msg}"),
            Self::FrameRender { frame, message } => {
                write!(f, "Failed to render frame {frame}: {message}")
            },
            Self::ImageEncode(msg) => write!(f, "Image encoding error: {msg}"),
            Self::ImageSave(err) => write!(f, "Failed to save image: {err}"),
            Self::VideoEncode(msg) => write!(f, "Video encoding error: {msg}"),
            Self::GifEncode(msg) => write!(f, "GIF encoding error: {msg}"),
            Self::InvalidPath(_) => write!(f, "Output path contains null bytes"),
            Self::ThreadPanicked => write!(f, "Render thread panicked"),
            Self::Cancelled => write!(f, "Export cancelled by user"),
            Self::FfmpegNotFound => write!(
                f,
                "ffmpeg is required for audio muxing. Install it from https://ffmpeg.org or via your package manager (e.g. `apt install ffmpeg`, `brew install ffmpeg`)."
            ),
            Self::Internal(msg) => write!(f, "Internal export error: {msg}"),
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
