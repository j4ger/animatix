/// Errors that can occur during renderer initialization or frame rendering.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// Failed to create a WGPU render surface.
    #[error("Failed to create render surface: {0}")]
    SurfaceCreation(String),
    /// No compatible GPU adapter was found.
    #[error("No compatible GPU adapter found")]
    AdapterNotFound,
    /// Failed to request a GPU device.
    #[error("Failed to request GPU device: {0}")]
    DeviceRequestFailed(String),
    /// Failed to initialize the Vello renderer.
    #[error("Failed to initialize Vello renderer: {0}")]
    VelloInit(String),
    /// Rendering a single frame failed.
    #[error("Frame render failed: {0}")]
    FrameRender(String),
    /// Failed to create the application window.
    #[error("Failed to create window: {0}")]
    WindowCreation(String),
    /// Failed to create the event loop.
    #[error("Failed to create event loop: {0}")]
    EventLoopCreation(String),
    /// Text or math compilation via Typst failed.
    #[error("Text compilation failed: {0}")]
    TextCompilation(String),
}
