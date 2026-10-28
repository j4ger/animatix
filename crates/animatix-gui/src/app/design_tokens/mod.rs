//! Layered design token system for the Animatix GUI.
//!
//! ## Module layout
//!
//! ```text
//! design_tokens/
//!   ├── mod.rs       ← this file: eparts token re-exports + public submodules
//!   ├── semantic.rs  ← public color role tokens (generic re-exports + app-specific)
//!   └── spatial.rs   ← spacing, row heights, radii, domain dimensions (generic + app-specific)
//! ```
//!
//! `primitive`, `typography`, `motion`, and `util` have no local source file:
//! they are re-exported from `eparts::tokens` below, so their historical
//! `crate::app::design_tokens::…` paths keep resolving.
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

/// Raw palette — re-exported from `eparts::tokens::primitive`.
/// Also provides the `p::*` alias used by semantic.rs app-specific submodules.
pub use eparts::tokens::primitive;
pub use eparts::tokens::{motion, typography, util};
pub mod semantic;
pub mod spatial;

pub use util::{lerp_color, multiply_alpha};
