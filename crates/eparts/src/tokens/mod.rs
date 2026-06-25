//! Generic design tokens — the domain-free half of the Animatix design system.
//!
//! These tokens are safe to use from any egui application. They carry no
//! animatix-domain semantics (no scene graph, no timeline, no diagnostic phases).

pub mod primitive;
pub mod semantic;
pub mod spatial;
pub mod theme;
pub mod typography;
pub mod motion;
pub mod util;

pub use util::{lerp_color, multiply_alpha};
pub use theme::{theme, theme_from_ctx, set_theme, Theme};
