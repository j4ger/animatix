//! Timeline track lanes rendering: static action blocks and speech/transcript review.

use animatix::timeline::Timeline;
use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke};

use super::TimelineCoords;
use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command};
use crate::app::design_tokens::spatial::{RADIUS_S, STROKE_WIDTH, Spatial};
use crate::app::design_tokens::typography::TextRole;

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

/// Render static, colored action blocks with their duration labels.
mod action;
mod speech;

pub(crate) use action::render_action_blocks;
pub(crate) use impls::{action_category_color, property_group_color, property_group_for_prop};
pub(crate) use speech::{render_speech_track, speech_row_count};
