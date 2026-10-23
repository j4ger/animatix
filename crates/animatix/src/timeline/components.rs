//! ECS Components for Animatix Timeline.
//!
//! This module defines strongly-typed data components attached to entities in
//! the timeline world, separating domain concerns (coordinates, layout, shapes,
//! styling) from monolithic tracks.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use animatix_text::transcript::SpeechTranscript;

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

/// Whether an actor is carried into the successor scene.
///
/// Written by the `persist` / `remove` actions. An entity without this
/// component has never been targeted by either action, which resolves exactly
/// like a `false` flag (the pre-ECS `persistence_flags` map default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PersistenceComponent {
    /// `true` = carry the actor across a scene boundary; `false` = `remove`d.
    pub persistent: bool,
}

/// Auto-color slot assigned by `color: auto`, indexing the colorscheme cycle.
///
/// Presence means the actor was declared with `color: auto`; the slot is
/// carried across scenes so the actor keeps the same cycle color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct AutoColorComponent {
    /// Slot index into the resolved colorscheme's `auto_cycle`.
    pub slot: usize,
}

/// Parsed speech transcript attached to an `Audio` / `Captions` actor.
///
/// Holds the word-level timing data used by the captions renderer and the GUI
/// speech lanes.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TranscriptComponent(pub SpeechTranscript);
