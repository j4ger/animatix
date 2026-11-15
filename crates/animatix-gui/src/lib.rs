pub mod app;
pub mod completion_popup;
#[cfg(feature = "dev-screenshots")]
pub mod dev;
pub mod editor;
pub mod error;
pub mod fonts;
pub mod highlighting;
pub mod hot_reload;
pub mod preview_surface;

pub use app::run_gui;
