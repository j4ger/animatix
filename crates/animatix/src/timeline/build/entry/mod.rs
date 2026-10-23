//! Build entry points: AST-to-Timeline lowering pass infrastructure.
//!
//! Public entry points: `build()`, `build_with_font_context()`,
//! `build_with_diagnostics()`, `build_with_diagnostics_and_font_context()`.

use tracing::instrument;

use super::post::{seed_namespace_exports, synthesize_idle_tracks};
use super::*;
use crate::timeline::layout::ChildExtent;

mod builders;
mod impl_mod;
mod layout;
