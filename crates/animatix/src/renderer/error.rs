/** Errors that can occur during renderer initialization or frame rendering. */
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Failed to create render surface: {0}")]
    SurfaceCreation(String),
    #[error("No compatible GPU adapter found")]
    AdapterNotFound,
    #[error("Failed to request GPU device: {0}")]
    DeviceRequestFailed(String),
    #[error("Failed to initialize Vello renderer: {0}")]
    VelloInit(String),
    #[error("Frame render failed: {0}")]
    FrameRender(String),
    #[error("Failed to create window: {0}")]
    WindowCreation(String),
    #[error("Failed to create event loop: {0}")]
    EventLoopCreation(String),
    #[error("Text compilation failed: {0}")]
    TextCompilation(String),
}
