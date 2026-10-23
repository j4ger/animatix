#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::easing::Easing;

// ─────────────────────────────────────────────────────────────
// Action event metadata (for GUI timeline visualization)
// ─────────────────────────────────────────────────────────────

/// A recorded action event in the timeline.
///
/// Actions are processed at build time into keyframes, but their metadata
/// is retained for GUI visualization (colored blocks in the timeline).
#[derive(Clone, Debug, PartialEq)]
pub struct ActionEvent {
    /// Action verb (e.g. "fade-in", "move", "rotate").
    pub verb: String,
    /// Target actor labels.
    pub targets: Vec<String>,
    /// Start time in milliseconds.
    pub start_time_ms: u64,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// Easing curve used.
    pub easing: Easing,
    /// Action category for UI color coding.
    pub category: ActionCategory,
}

/// Category of an action for UI color coding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ActionCategory {
    /// Entrance actions (fade-in, wipe-in, etc.) - green.
    Entrance,
    /// Motion actions (move, shift, rotate, scale) - blue.
    Motion,
    /// Exit actions (fade-out, wipe-out) - red.
    Exit,
    /// Effect actions (bounce, pulse, shake) - amber.
    Effect,
    /// Reorder actions (swap, reorder) - purple.
    Reorder,
    /// Reveal actions (draw-in, reveal-in, draw-out, reveal-out) - cyan.
    Reveal,
}
