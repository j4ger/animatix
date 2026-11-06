use std::collections::{HashMap, HashSet};

use animatix::timeline::{EntityId, Timeline, collect_all_keyframe_times};
use egui::{Color32, Pos2, RichText, ScrollArea, Vec2};
use eparts::widget::Select;
use eparts::{Button, NumberField};

use crate::app::PreviewPaneState;
use crate::app::commands::{ActionQueue, Command};
use crate::app::components::easing_curve_editor::EasingCurveState;
use crate::app::components::{TabBar, easing_curve_editor, layout, text_tooltip, timeline};
use crate::app::design_tokens::spatial::inspector::INPUT_WIDTH_FLOAT as INSPECTOR_INPUT_WIDTH_FLOAT;
use crate::app::design_tokens::spatial::{RADIUS_S, spatial};
use crate::app::design_tokens::typography::TextRole;

pub(crate) mod graph_editor;
pub(crate) mod keyframe_table;
pub(crate) mod property_groups;
mod property_groups_legend;
pub(crate) mod spreadsheet;

use self::keyframe_table::{count_keyframes, render_dope_sheet};
use self::property_groups::*;
use self::spreadsheet::render_property_spreadsheet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PropertyViewMode {
    Semantic,
    Intensity,
    Spreadsheet,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum KeyframeViewMode {
    List,
    Curve,
}

pub(crate) struct InspectorContext<'a> {
    pub preview: &'a mut PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub active_scene: Option<&'a str>,
    pub selected_actors: &'a mut HashSet<String>,
    pub commands: &'a mut ActionQueue,
    pub keyframe_mode: bool,
    pub scene_dimensions: animatix::timeline::SceneDimensions,
    pub pivot_offsets: &'a mut HashMap<String, [f32; 2]>,
    pub property_view_mode: &'a mut PropertyViewMode,
    pub keyframe_view_mode: &'a mut KeyframeViewMode,
}

mod impls;
mod scene;

pub(crate) use impls::inspector_panel_ui;
use impls::{
    render_actor_header, render_container_children, render_parent_card, render_property_stream,
};
use scene::{inspector_ui, render_scene_inspector};
