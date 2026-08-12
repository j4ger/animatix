//! Generic design tokens — the domain-free half of the Animatix design system.
//!
//! These tokens are safe to use from any egui application. They carry no
//! animatix-domain semantics (no scene graph, no timeline, no diagnostic phases).

pub mod motion;
pub mod primitive;
pub mod semantic;
pub mod spatial;
pub mod theme;
#[cfg(feature = "theme-json")]
pub mod theme_json;
#[cfg(feature = "theme-json")]
pub mod theme_watcher;
pub mod typography;
pub mod util;

pub use theme::{Theme, set_theme, theme, theme_from_ctx};
#[cfg(feature = "theme-json")]
pub use theme_json::{ThemeFile, theme_schema_json};
#[cfg(feature = "theme-json")]
pub use theme_watcher::{ThemeWatcher, ThemeWatcherEvent};
pub use util::{lerp_color, multiply_alpha};
