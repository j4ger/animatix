#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::timeline::property_track::PropertyTrack;

// ─────────────────────────────────────────────────────────────
// HighlightTracks
// ─────────────────────────────────────────────────────────────

/// Per-actor highlight property tracks (for equation fragment highlights).
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct HighlightTracks {
    /// Highlight background color (RGBA) for equation fragments.
    pub highlight_color: Option<PropertyTrack<[f32; 4]>>,
    /// Highlight opacity for equation fragments.
    pub highlight_opacity: Option<PropertyTrack<f32>>,
    /// Highlight padding (in logical pixels) around equation fragments.
    pub highlight_padding: Option<PropertyTrack<f32>>,
    /// Highlight corner radius for equation fragments.
    pub highlight_radius: Option<PropertyTrack<f32>>,
    /// Highlight blend mode for equation fragments (non-animated configuration).
    #[cfg_attr(
        feature = "serde",
        serde(skip, default = "HighlightTracks::default_blend")
    )]
    pub highlight_blend: vello::peniko::Mix,
}

impl Default for HighlightTracks {
    fn default() -> Self {
        Self {
            highlight_color: None,
            highlight_opacity: None,
            highlight_padding: None,
            highlight_radius: None,
            highlight_blend: vello::peniko::Mix::Difference,
        }
    }
}

impl HighlightTracks {
    #[allow(dead_code)] // Used as serde `default` attribute target; not directly called.
    fn default_blend() -> vello::peniko::Mix {
        vello::peniko::Mix::Difference
    }
}
