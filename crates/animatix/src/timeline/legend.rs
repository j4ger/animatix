//! Legend track storage

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// User-controlled legend participation for an actor.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LegendMode {
    /// Include this actor when it matches the automatic legend heuristics.
    Auto,
    /// Never include this actor.
    Hidden,
    /// Include this actor with an explicit label.
    Label(String),
}

impl Default for LegendMode {
    fn default() -> Self {
        LegendMode::Auto
    }
}

impl LegendMode {
    /// Convert a parsed `legend` union value into a participation mode.
    pub fn from_property_value(value: &super::property_engine::PropertyValue) -> Option<Self> {
        match value {
            super::property_engine::PropertyValue::Bool(false) => Some(LegendMode::Hidden),
            super::property_engine::PropertyValue::Bool(true) => Some(LegendMode::Auto),
            super::property_engine::PropertyValue::String(label) => {
                Some(LegendMode::Label(label.clone()))
            },
            _ => None,
        }
    }
}

/// Stores legend configuration and auto-generated entries
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LegendTracks {
    /// Auto-generated legend entries (label, color) pairs
    pub entries: Vec<(String, [f32; 4])>,
    /// Per-actor legend participation mode, updated when `legend` is declared.
    pub mode: LegendMode,
}
