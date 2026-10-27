//! Actor building logic: processes actor declarations, generates VelloPaths,
//! inserts keyframes, and dispatches primitive builds.

use super::*;

mod decl;
mod extensions;
mod helpers;
mod prepare;
