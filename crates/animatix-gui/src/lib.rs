pub mod app;
pub mod cell_editor;
pub mod completion_popup;
#[cfg(feature = "dev-screenshots")]
pub mod dev;
pub mod document;
pub mod editor;
pub mod error;
mod fonts;
pub mod highlighting;
pub mod hot_reload;
pub mod preview_surface;
pub mod source_edit;
pub mod text_diff;
pub mod validation;

pub use app::review::run_review;
pub use app::run_gui;
