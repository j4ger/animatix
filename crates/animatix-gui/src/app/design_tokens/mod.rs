//! Layered design token system for the Animatix GUI.
//!
//! ## Module layout
//!
//! ```text
//! design_tokens/
//!   ├── mod.rs       ← this file: re-exports public submodules
//!   ├── primitive.rs ← raw palette (pub(crate)) — never imported externally
//!   ├── semantic.rs  ← public color role tokens
//!   ├── spatial.rs   ← spacing, row heights, radii, domain dimensions
//!   ├── typography.rs← TextRole enum and font-size constants
//!   ├── motion.rs    ← duration and easing primitives
//!   └── util.rs      ← lerp_color, multiply_alpha
//! ```
//!
//! ## Import convention
//!
//! UI code imports from the semantic/spatial/typography submodules directly:
//!
//! ```ignore
//! use crate::app::design_tokens::semantic::{accent, surface, text};
//! use crate::app::design_tokens::spatial;
//! use crate::app::design_tokens::typography::TextRole;
//! ```

mod primitive;
pub mod motion;
pub mod semantic;
pub mod spatial;
pub mod typography;
pub mod util;

pub use util::{lerp_color, multiply_alpha};
