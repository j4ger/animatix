//! Opt-in Vello GPU player using only wgpu's GL backend in the browser.

mod effects;
mod encode;
mod gpu;
mod images;
mod player;

pub(super) use gpu::Gpu;
pub use player::create_webgl_player;
pub(super) use player::draw_document;
