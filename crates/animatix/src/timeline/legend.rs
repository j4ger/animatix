//! Legend track storage

/// Stores legend configuration and auto-generated entries
#[derive(Clone, Debug, Default)]
pub struct LegendTracks {
    /// Auto-generated legend entries (label, color) pairs
    pub entries: Vec<(String, [f32; 4])>,
}
