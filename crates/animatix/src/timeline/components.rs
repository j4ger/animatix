//! ECS Components for Animatix Timeline.
//!
//! This module defines strongly-typed data components attached to entities in
//! the timeline world, separating domain concerns (coordinates, layout, shapes,
//! styling) from monolithic tracks.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::build::utils::ScaleType;
use super::entity::EntityId;

/// 2D Mathematical Coordinate Frame Component.
///
/// Attached to coordinate host entities (such as `Graph`). Holds domains,
/// axis scales, padding, and screen transformation properties.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CoordinateFrameComponent {
    /// Domain of the horizontal math axis `[min_x, max_x]`.
    pub x_domain: [f64; 2],
    /// Domain of the vertical math axis `[min_y, max_y]`.
    pub y_domain: [f64; 2],
    /// Scaling algorithm for the horizontal axis.
    pub x_scale: ScaleType,
    /// Scaling algorithm for the vertical axis.
    pub y_scale: ScaleType,
    /// Internal padding `[left, right, top, bottom]` in pixels.
    pub padding: [f64; 4],
    /// Whether to display background grid lines.
    pub grid: bool,
    /// Whether to display axis tick marks.
    pub ticks: bool,
    /// Axis tick label display mode (`"auto"`, `"x"`, `"y"`, `"both"`, `"false"`).
    pub tick_labels: String,
}

impl Default for CoordinateFrameComponent {
    fn default() -> Self {
        Self {
            x_domain: [-10.0, 10.0],
            y_domain: [-10.0, 10.0],
            x_scale: ScaleType::Linear,
            y_scale: ScaleType::Linear,
            padding: [0.0; 4],
            grid: false,
            ticks: false,
            tick_labels: "auto".to_string(),
        }
    }
}

/// Resolved Coordinate Context passed to child plot entities during building/sampling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoordinateContext {
    /// The EntityId of the parent host providing this coordinate context.
    pub host_id: EntityId,
    /// Domain of the horizontal math axis `[min_x, max_x]`.
    pub x_domain: [f64; 2],
    /// Domain of the vertical math axis `[min_y, max_y]`.
    pub y_domain: [f64; 2],
    /// Scaling algorithm for the horizontal axis.
    pub x_scale: ScaleType,
    /// Scaling algorithm for the vertical axis.
    pub y_scale: ScaleType,
    /// Internal padding `[left, right, top, bottom]` in pixels.
    pub padding: [f64; 4],
    /// Full width and height of the host plot area in pixels.
    pub full_size: [f64; 2],
}

impl CoordinateContext {
    /// Create a new coordinate context from a frame component and full size.
    pub fn new(host_id: EntityId, frame: &CoordinateFrameComponent, full_size: [f64; 2]) -> Self {
        Self {
            host_id,
            x_domain: frame.x_domain,
            y_domain: frame.y_domain,
            x_scale: frame.x_scale,
            y_scale: frame.y_scale,
            padding: frame.padding,
            full_size,
        }
    }
}
