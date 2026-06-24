//! Legend track storage

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Stores legend configuration and auto-generated entries
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LegendTracks {
    /// Auto-generated legend entries (label, color) pairs
    pub entries: Vec<(String, [f32; 4])>,
}
