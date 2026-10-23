use kurbo::{BezPath, CubicBez, ParamCurve, PathEl, Point};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::property_track::{Interpolate, PropertyTrack};
use crate::easing::apply_easing;
use crate::renderer::types::{TextPath, VelloPath};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
/// Strategy for aligning paths before morphing.
pub enum MorphStrategy {
    /// Automatic alignment (index-based pairing).
    Auto,
    /// Sort paths by centroid before pairing.
    Match,
    /// Fade-based transition (cross-fade overlay).
    Fade,
    /// Pair each source path to its spatially nearest target path.
    Nearest,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
/// Options controlling path morphing behavior.
pub struct MorphOptions {
    /// Alignment strategy to use.
    pub strategy: MorphStrategy,
    /// Arc amount for curved interpolation (radians).
    pub path_arc: f64,
    /// Whether to stretch paths to match bounding boxes.
    pub stretch: bool,
}

impl Default for MorphOptions {
    fn default() -> Self {
        Self {
            strategy: MorphStrategy::Auto,
            path_arc: 0.0,
            stretch: false,
        }
    }
}

impl MorphOptions {
    /// Stable display summary used when this compound option crosses the
    /// generic property-value boundary.
    pub fn summary(&self) -> String {
        format!(
            "strategy={:?}, path_arc={:.2}, stretch={}",
            self.strategy, self.path_arc, self.stretch
        )
    }
}

mod align;
mod geometry;
mod interpolate;
#[cfg(test)]
mod tests;
mod text;

pub use align::{
    align_path_lists, align_path_lists_with_strategy, align_segments, align_subpaths,
    align_subpaths_with_strategy,
};
pub use interpolate::{evaluate_paths_with_options, morph_paths, morph_paths_with_options};
pub use text::{interpolate_text_paths, interpolate_vello_paths};
