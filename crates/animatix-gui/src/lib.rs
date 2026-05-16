pub mod app;
pub mod completion_popup;
pub mod cell_editor;
pub mod document;
pub mod editor;
pub mod highlighting;
pub mod hot_reload;
pub mod preview_surface;
pub mod source_edit;

#[cfg(feature = "dev-screenshots")]
pub mod dev;

pub use app::run_gui;
