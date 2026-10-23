//! Taffy layout result types.

/// Constraints for min/max size clamping.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SizeConstraints {
    /// Minimum width in logical pixels.
    pub min_width: Option<f32>,
    /// Maximum width in logical pixels.
    pub max_width: Option<f32>,
    /// Minimum height in logical pixels.
    pub min_height: Option<f32>,
    /// Maximum height in logical pixels.
    pub max_height: Option<f32>,
}

/// Computed layout result for a single child.
#[derive(Clone, Debug, PartialEq)]
pub struct TaffyLayoutResult {
    /// The center-relative position [x, y] of the child
    pub position: [f32; 2],
    /// The computed layout size [width, height] of the child
    pub size: [f32; 2],
}

/// Result of a Taffy layout computation, including container size and child positions.
#[derive(Clone, Debug)]
pub struct TaffyLayoutOutput {
    /// Child positions relative to container center.
    pub positions: Vec<TaffyLayoutResult>,
    /// Container total size [width, height].
    pub container_size: [f32; 2],
}
