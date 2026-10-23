//!
//! # Taffy-Backed Layout Backend
//!
//! This module provides layout computation via the Taffy layout engine while preserving
//! the existing center-relative coordinate semantics and manual-child handling.
//!
//! ## Key Design Points
//!
//! 1. **Center-relative coordinates**: Taffy uses top-left origin with absolute positioning. We
//!    translate by offsetting by container size/2 to maintain center-origin semantics.
//!
//! 2. **Manual child exclusion**: Children with `PlacementMode::Manual` are excluded from position
//!    assignment but their sizes still participate in container layout via the `available_size`
//!    mechanism (pre-computed with manual children included).
//!
//! 3. **Stack special-casing**: Stack layout places all children at origin (0, 0) regardless of
//!    their sizes. We keep this behavior unchanged for semantic compatibility.
//!
//! 4. **Gap and alignment**: Translated to Taffy's flexbox `gap` and
//!    `align_items`/`justify_content`.
//!
//! 5. **Build-time vs dynamic**: The same Taffy computation path is used for both build-time
//!    (`apply_container_layout`) and dynamic (`compute_layout_for_time`) layouts.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use taffy::prelude::*;

use crate::timeline::LayoutType;
use crate::timeline::layout::ChildExtent;
#[cfg(test)]
use crate::timeline::layout::compute_container_size;

mod grid;
mod linear;
mod specs;
mod style;
#[cfg(test)]
mod tests;
mod types;

pub use grid::{compute_taffy_grid_layout, compute_taffy_grid_layout_with_specs};
pub use linear::{
    compute_taffy_linear_layout, compute_taffy_linear_layout_with_baselines,
    compute_taffy_linear_layout_with_specs,
};
pub use specs::{ChildSizeSpec, SizeSpec, parse_size_spec};
pub use types::{SizeConstraints, TaffyLayoutOutput, TaffyLayoutResult};
