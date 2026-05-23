//! Structured error types for the Animatix GUI.

use std::path::PathBuf;

/// Errors that can occur in the Animatix GUI application.
#[derive(Debug, thiserror::Error)]
pub enum GuiError {
    /// Failed to read or write a file.
    #[error("I/O error for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Failed to parse source code.
    #[error("Parse error: {message}")]
    Parse { message: String },

    /// Timeline build failed.
    #[error("Build failed: {message}")]
    Build { message: String },

    /// Preview surface initialization failed.
    #[error("Preview surface error: {message}")]
    PreviewSurface { message: String },

    /// Export operation failed.
    #[error("Export failed: {message}")]
    Export { message: String },

    /// A generic operation failed with a descriptive message.
    #[error("{0}")]
    Other(String),
}

impl GuiError {
    /// Create a generic error from a message.
    pub fn msg<S: Into<String>>(message: S) -> Self {
        Self::Other(message.into())
    }
}

impl From<std::io::Error> for GuiError {
    fn from(err: std::io::Error) -> Self {
        Self::Io {
            path: PathBuf::from("<unknown>"),
            source: err,
        }
    }
}
