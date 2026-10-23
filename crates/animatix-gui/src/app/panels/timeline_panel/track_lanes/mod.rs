//! Timeline track lanes rendering: scene tracks, speech/transcript tracks, action blocks,
//! and empty property lane affordances.

use std::collections::HashMap;

use animatix::composition::Composition;
use animatix::timeline::Timeline;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

use super::{TimelineCoords, draw_loop_region};
use crate::app::PreviewPaneState;
use crate::app::commands::{
    ActionQueue, Command, DocumentCommand, PlaybackCommand, PropertyEdit, ShellAction,
};
use crate::app::components::text_tooltip;
use crate::app::design_tokens::semantic::{category, timeline};
use crate::app::design_tokens::spatial::timeline::KF_HALF as KF_DIAMOND_HALF;
use crate::app::design_tokens::spatial::{RADIUS_S, STROKE_WIDTH, Spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::document::timeline_diff::lane_schema;

/// Property groups for per-property lanes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropertyGroup {
    Transform,
    Style,
    Shape,
    Text,
    Effects,
}

pub(crate) const PROPERTY_GROUPS: &[(PropertyGroup, &str, &[&str])] = &[
    (
        PropertyGroup::Transform,
        "Transform",
        &[
            "position",
            "motion_offset",
            "rotation",
            "scale",
            "size",
            "layout_size",
            "placement_mode",
            "position_binding",
        ],
    ),
    (
        PropertyGroup::Style,
        "Style",
        &[
            "color",
            "opacity",
            "stroke_width",
            "stroke_color",
            "stroke_progress",
            "fill_opacity",
            "line_cap",
            "line_join",
            "morph_options",
        ],
    ),
    (
        PropertyGroup::Shape,
        "Shape",
        &[
            "shape_type",
            "line_from",
            "line_to",
            "head_size",
            "arc_angles",
            "points",
            "commands",
            "vector_paths",
        ],
    ),
    (
        PropertyGroup::Text,
        "Text",
        &["text_content", "font_family", "font_size", "text_paths"],
    ),
];

pub(crate) fn property_group_for_prop(prop: &str) -> Option<PropertyGroup> {
    // Effect stage parameters (`stage.param`) form their own group; their lane
    // names are runtime strings, not registry properties.
    if prop.contains('.') {
        return Some(PropertyGroup::Effects);
    }
    for (group, _, props) in PROPERTY_GROUPS {
        if props.contains(&prop) {
            return Some(*group);
        }
    }
    None
}

pub(crate) fn property_group_color(group: PropertyGroup, theme: eparts::Theme) -> Color32 {
    match group {
        PropertyGroup::Transform => theme.palette.accent.primary,
        PropertyGroup::Style => theme.palette.status.success,
        PropertyGroup::Shape => theme.palette.status.warning,
        PropertyGroup::Text => theme.palette.accent.cyan,
        PropertyGroup::Effects => theme.palette.accent.cyan,
    }
}

pub(crate) fn action_category_color(
    cat: animatix::timeline::ActionCategory,
    theme: eparts::Theme,
) -> Color32 {
    use animatix::timeline::ActionCategory;
    match cat {
        ActionCategory::Entrance => theme.palette.status.success,
        ActionCategory::Motion => theme.palette.accent.primary,
        ActionCategory::Exit => theme.palette.status.error,
        ActionCategory::Effect => theme.palette.status.warning,
        ActionCategory::Reorder => category::ACTION,
        ActionCategory::Reveal => theme.palette.accent.cyan,
    }
}

/// Action block edge for resizing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Edge {
    Left,
    Right,
}

/// Action block drag state tuple:
/// `(track_idx, event_start_ms, edge, initial_pointer_x, original_start_s, original_duration_s)`
pub(crate) type ActionDragState = (usize, u64, Edge, f32, f64, f64);

/// Render colored block segments for each scene in a composition track,
/// with a keyframe density strip and duration label inside each block.
mod action;
mod empty;
mod scene;
mod speech;

pub(crate) use action::render_action_blocks;
pub(crate) use empty::render_empty_property_lane;
pub(crate) use scene::render_scene_track;
pub(crate) use speech::render_speech_track;
