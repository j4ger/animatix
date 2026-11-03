//! Build entry points: AST-to-`Timeline` lowering pass infrastructure.
//!
//! Public entry points: [`Timeline::build`], [`Timeline::build_with_diagnostics`],
//! [`Timeline::build_with_context`] and [`Timeline::build_with_options`]. The
//! first three are conveniences; all of them delegate to
//! [`Timeline::build_with_options`], which is the one lowering implementation.

use tracing::instrument;

use super::post::{seed_namespace_exports, synthesize_idle_tracks};
use super::*;
use crate::timeline::layout::ChildExtent;

mod builders;
mod finalize;
mod format_specs;
mod impl_mod;
mod layout;
mod options;
mod seed;
mod validate;
mod walk;

pub use options::{BuildOptions, CarryInput};
