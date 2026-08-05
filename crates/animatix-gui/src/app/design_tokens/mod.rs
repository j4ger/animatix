//! Layered design token system for the Animatix GUI.
//!
//! ## Module layout
//!
//! ```text
//! design_tokens/
//!   ├── mod.rs       ← this file: re-exports public submodules + eparts shims
//!   ├── primitive.rs ← re-exported from eparts::tokens::primitive (bridge)
//!   ├── semantic.rs  ← public color role tokens (generic re-exports + app-specific)
//!   ├── spatial.rs   ← spacing, row heights, radii, domain dimensions (generic + app-specific)
//!   ├── typography.rs← re-exported from eparts::tokens::typography
//!   ├── motion.rs    ← re-exported from eparts::tokens::motion
//!   └── util.rs      ← re-exported from eparts::tokens::util
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

// ── Re-export shims: generic tokens now live in eparts ──────────────────────
// The `pub use` makes them reachable under their original `crate::app::design_tokens::…`
// paths, so zero call-site changes are needed.

/// Raw palette — bridge to eparts where primitive.rs now lives.
/// Also provides the `p::*` alias used by semantic.rs app-specific submodules.
pub use eparts::tokens::primitive;
pub use eparts::tokens::{motion, typography, util};
pub mod semantic;
pub mod spatial;

pub use util::{lerp_color, multiply_alpha};
