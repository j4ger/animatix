//! Timeline track lanes rendering: scene tracks, speech/transcript tracks, action blocks,
//! and empty property lane affordances.

use animatix::timeline::Timeline;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};

use super::TimelineCoords;
use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command, PropertyEdit, ShellAction};
use crate::app::components::text_tooltip;
use crate::app::design_tokens::spatial::timeline::KF_HALF as KF_DIAMOND_HALF;
use crate::app::design_tokens::spatial::{RADIUS_S, STROKE_WIDTH, Spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::document::timeline_diff::lane_schema;

mod impls;

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
mod speech;

pub(crate) use action::render_action_blocks;
pub(crate) use empty::render_empty_property_lane;
pub(crate) use impls::{action_category_color, property_group_color, property_group_for_prop};
pub(crate) use speech::{render_speech_track, speech_row_count};
