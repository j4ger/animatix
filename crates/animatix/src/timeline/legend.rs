//! Legend track storage

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// User-controlled legend participation for an actor.
#[derive(Clone, Debug, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LegendMode {
    /// Include this actor when it matches the automatic legend heuristics.
    #[default]
    Auto,
    /// Never include this actor.
    Hidden,
    /// Include this actor with an explicit label.
    Label(String),
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
            super::property_engine::PropertyValue::Variant { name, value } => match name.as_str() {
                "auto" => Some(LegendMode::Auto),
                "hidden" => Some(LegendMode::Hidden),
                "label" => match value.as_ref() {
                    super::property_engine::PropertyValue::String(label) => {
                        Some(LegendMode::Label(label.clone()))
                    },
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }
}

/// Stores legend configuration and auto-generated entries
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LegendTracks {
    /// Auto-generated legend entries (label, color) pairs
    pub entries: Vec<(String, [f32; 4])>,
    /// Explicit color captured from the actor declaration, if any.
    pub color: Option<[f32; 4]>,
    /// Whether `legend` was explicitly declared on this actor.
    pub legend_declared: bool,
    /// Optional title rendered above the entries.
    pub title: String,
    /// Label font size in points.
    pub font_size: f32,
    /// Explicit label color. `None` selects contrast against the background.
    pub label_color: Option<[f32; 4]>,
    /// Color swatch size in scene units.
    pub swatch_size: f32,
    /// Vertical gap between rows and title/entries in scene units.
    pub gap: f32,
    /// Maximum label width before wrapping; `0` disables wrapping.
    pub text_max_width: f32,
}

/// Derive the current legend participation mode from the tagged `legend` property.
pub fn legend_mode_for_track(track: &super::AnimationTrack) -> LegendMode {
    let time_ms = track.first_seen_ms;
    let value =
        super::dispatch::read_property_value(track, super::ActorField::Tagged("legend"), time_ms);
    value
        .as_ref()
        .and_then(LegendMode::from_property_value)
        .unwrap_or(LegendMode::Auto)
}

impl Default for LegendTracks {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            color: None,
            legend_declared: false,
            title: String::new(),
            font_size: 14.0,
            label_color: None,
            swatch_size: 16.0,
            gap: 8.0,
            text_max_width: 240.0,
        }
    }
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

/// Returns `true` when a shape is sized to fill the full viewport.
fn is_full_viewport_background(track: &super::AnimationTrack) -> bool {
    use crate::timeline::taffy_layout::SizeSpec;
    let full = |spec: SizeSpec| match spec {
        SizeSpec::Fill => true,
        SizeSpec::Percent(p) => (p - 1.0).abs() < f32::EPSILON,
        _ => false,
    };
    matches!(track.geometry.size_spec, Some(spec) if full(spec.width) && full(spec.height))
}

fn source_order(
    tracks: &std::collections::BTreeMap<String, super::AnimationTrack>,
    roots: &[String],
) -> std::collections::HashMap<String, usize> {
    let mut order = std::collections::HashMap::new();
    let mut visited = std::collections::HashSet::new();
    let mut next = 0usize;
    fn visit(
        label: &str,
        tracks: &std::collections::BTreeMap<String, super::AnimationTrack>,
        _roots: &[String],
        order: &mut std::collections::HashMap<String, usize>,
        visited: &mut std::collections::HashSet<String>,
        next: &mut usize,
    ) {
        if !visited.insert(label.to_string()) {
            return;
        }
        order.insert(label.to_string(), *next);
        *next += 1;
        if let Some(track) = tracks.get(label) {
            for child in &track.children {
                visit(child, tracks, _roots, order, visited, next);
            }
        }
    }
    for root in roots {
        visit(root, tracks, roots, &mut order, &mut visited, &mut next);
    }
    order
}

/// Scan completed actor tracks for color-bearing legend candidates.
///
/// Runs after the full timeline is built so forward declarations and generated
/// actors are visible. Structural actors are excluded by kind, and actors can
/// opt out or supply an explicit label via `legend`.
pub fn scan_legend_entries(
    tracks: &std::collections::BTreeMap<String, super::AnimationTrack>,
    roots: &[String],
) -> Vec<(String, [f32; 4])> {
    let order = source_order(tracks, roots);
    let mut candidates = Vec::new();
    for (label, track) in tracks {
        let mode = legend_mode_for_track(track);
        if !legend_eligible(&track.kind)
            || mode == LegendMode::Hidden
            || (is_full_viewport_background(track) && !track.legend.legend_declared)
        {
            continue;
        }
        let Some(color) = track.legend.color else {
            continue;
        };
        let display_label = match &mode {
            LegendMode::Label(label) => label.clone(),
            LegendMode::Auto | LegendMode::Hidden => prettify_label(label),
        };
        let source_position = order.get(label).copied().unwrap_or(usize::MAX);
        candidates.push((
            source_position,
            track.first_seen_ms,
            label.clone(),
            display_label,
            color,
        ));
    }

    candidates
        .sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)).then_with(|| a.2.cmp(&b.2)));
    let mut entries: Vec<(String, [f32; 4])> = Vec::new();
    for (_, _, _, display_label, color) in candidates {
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
