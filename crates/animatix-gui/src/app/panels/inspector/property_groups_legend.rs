//! Legend-property entries for the inspector property-group builder.

use super::{PropertyEntry, PropertyKind};
pub(super) fn legend_style_entries(
    legend: Option<&animatix::timeline::LegendTracks>,
) -> Vec<PropertyEntry> {
    fn entry(name: &str, kind: PropertyKind) -> PropertyEntry {
        PropertyEntry {
            name: name.to_string(),
            kind,
            has_keyframes: false,
            has_keyframe_at_current_time: false,
            keyframe_count: 0,
        }
    }
    let legend = legend.cloned().unwrap_or_default();
    let label_color = legend.label_color.map(color_display).unwrap_or_else(|| "auto".to_string());
    vec![
        entry("title", PropertyKind::Text(legend.title.clone())),
        entry("font_size", PropertyKind::Float(legend.font_size)),
        entry("label_color", PropertyKind::Text(label_color)),
        entry("swatch_size", PropertyKind::Float(legend.swatch_size)),
        entry("gap", PropertyKind::Float(legend.gap)),
        entry("text_max_width", PropertyKind::Float(legend.text_max_width)),
    ]
}

pub(super) fn color_display(color: [f32; 4]) -> String {
    let r = (color[0] * 255.0).round() as u8;
    let g = (color[1] * 255.0).round() as u8;
    let b = (color[2] * 255.0).round() as u8;
    let a = color[3];
    if a >= 0.999 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r},{g},{b},{a:.2})")
    }
}
