/// Errors that can occur during renderer initialization or frame rendering.
#[derive(Debug)]
pub enum RenderError {
    SurfaceCreation(String),
    AdapterNotFound,
    DeviceRequestFailed(String),
    VelloInit(String),
    FrameRender(String),
    WindowCreation(String),
    EventLoopCreation(String),
    TextCompilation(String),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::SurfaceCreation(msg) => write!(f, "Failed to create render surface: {msg}"),
            RenderError::AdapterNotFound => write!(f, "No compatible GPU adapter found"),
            RenderError::DeviceRequestFailed(msg) => {
                write!(f, "Failed to request GPU device: {msg}")
            }
            RenderError::VelloInit(msg) => write!(f, "Failed to initialize Vello renderer: {msg}"),
            RenderError::FrameRender(msg) => write!(f, "Frame render failed: {msg}"),
            RenderError::WindowCreation(msg) => write!(f, "Failed to create window: {msg}"),
            RenderError::EventLoopCreation(msg) => {
                write!(f, "Failed to create event loop: {msg}")
            }
            RenderError::TextCompilation(msg) => {
                write!(f, "Text compilation failed: {msg}")
            }
        }
    }
}

impl std::error::Error for RenderError {}
