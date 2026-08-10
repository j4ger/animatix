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
    /// Explicit color captured from the actor declaration, if any.
    pub color: Option<[f32; 4]>,
}

fn legend_eligible(kind: &super::ActorKindId) -> bool {
    use super::ActorKindId::*;
    matches!(kind, Shape(_) | PlotCurve | VectorField | Heatmap | ContourSet | BarChart)
}

fn prettify_label(label: &str) -> String {
    label
        .split(['_', '-', '.'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Scan completed actor tracks for color-bearing legend candidates.
///
/// Runs after the full timeline is built so forward declarations and generated
/// actors are visible. Structural actors are excluded by kind, and actors can
/// opt out or supply an explicit label via `legend`.
pub fn scan_legend_entries(
    tracks: &std::collections::BTreeMap<String, super::AnimationTrack>,
) -> Vec<(String, [f32; 4])> {
    let mut candidates = Vec::new();
    for (label, track) in tracks {
        if !legend_eligible(&track.kind) || track.legend.mode == LegendMode::Hidden {
            continue;
        }
        let Some(color) = track.legend.color else {
            continue;
        };
        let display_label = match &track.legend.mode {
            LegendMode::Label(label) => label.clone(),
            LegendMode::Auto | LegendMode::Hidden => prettify_label(label),
        };
        candidates.push((track.first_seen_ms, label.clone(), display_label, color));
    }

    candidates.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut entries: Vec<(String, [f32; 4])> = Vec::new();
    for (_, _, display_label, color) in candidates {
        if entries
            .iter()
            .any(|(label, existing)| label == &display_label && *existing == color)
        {
            continue;
        }
        entries.push((display_label, color));
    }
    entries
}
