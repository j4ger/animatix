pub mod app;
pub mod cell_editor;
pub mod completion_popup;
pub mod document;
pub mod editor;
pub mod error;
pub mod highlighting;
pub mod hot_reload;
pub mod preview_surface;
pub mod source_edit;
pub mod validation;
pub mod text_diff;

pub use app::run_gui;
