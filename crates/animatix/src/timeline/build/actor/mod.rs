//! Actor building logic: processes actor declarations, generates VelloPaths,
//! inserts keyframes, and dispatches primitive builds.

use super::*;

mod decl;
pub(super) use decl::bind_mask_clips;
mod extensions;
mod helpers;
mod prepare;
