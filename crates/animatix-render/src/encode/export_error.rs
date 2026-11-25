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

/// Export preflight intent and source values for presentation by a UI.
#[derive(Clone, Debug)]
pub enum ExportWarning {
    /// The export finishes before an audio segment begins.
    AudioStartsAfterExport {
        /// Authored media source.
        source: String,
        /// End of the exported time range, in seconds.
        cutoff_s: f64,
        /// Start of the audio segment, in seconds.
        start_s: f64,
    },
    /// The export clips the end of an audio segment.
    AudioCutOff {
        /// Authored media source.
        source: String,
        /// End of the exported time range, in seconds.
        cutoff_s: f64,
        /// End of the audio segment, in seconds.
        end_s: f64,
    },
    /// The export clips captions at the end of a timeline or scene.
    CaptionsCutOff {
        /// Authored scene name for composition exports.
        scene: Option<String>,
        /// Time where export clips the captions, in seconds.
        cutoff_s: f64,
        /// End of the last caption, in seconds.
        end_s: f64,
    },
}

impl std::fmt::Display for ExportWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AudioStartsAfterExport {
                source,
                cutoff_s,
                start_s,
            } => write!(
                f,
                "Export ends at {cutoff_s:.3}s before audio '{source}' starts at {start_s:.3}s."
            ),
            Self::AudioCutOff {
                source,
                cutoff_s,
                end_s,
            } => write!(
                f,
                "Export ends at {cutoff_s:.3}s and will cut off audio '{source}' ending at {end_s:.3}s."
            ),
            Self::CaptionsCutOff {
                scene,
                cutoff_s,
                end_s,
            } => {
                write!(f, "Export will cut off captions")?;
                if let Some(scene) = scene {
                    write!(f, " in scene '{scene}'")?;
                }
                write!(f, " at {cutoff_s:.3}s; the last caption ends at {end_s:.3}s.")
            },
        }
    }
}
