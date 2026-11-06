use animatix::renderer::text as animatix_text;
use animatix::timeline::{
    ActorField, EntityId, PROPERTY_REGISTRY, PropertyValue, ShapeType, SumVariant, ValueType,
    allowed_property_indices,
};
use egui::{Color32, Stroke, Vec2};
use eparts::widget::{Select, UiExt, color_to_hex_rgba};
use eparts::{Checkbox, NumberField, Slider, TextField};

use super::property_groups_legend::legend_style_entries;
use crate::app::commands::{
    ActionQueue, Command, DragEvent, PropertyEdit, PropertyValue as GuiPropertyValue, ShellAction,
};
use crate::app::components::button::Button;
use crate::app::components::{Badge, ColorPicker, row, text_tooltip};
use crate::app::design_tokens::spatial::inspector::{
    COL_GAP as INSPECTOR_COL_GAP, KF_BTN_WIDTH as INSPECTOR_KF_BTN_WIDTH,
    KF_COL_WIDTH as INSPECTOR_KF_COL_WIDTH, LABEL_MAX_WIDTH as INSPECTOR_LABEL_MAX_WIDTH,
    LABEL_MIN_WIDTH as INSPECTOR_LABEL_MIN_WIDTH, LABEL_WIDTH_FRAC as INSPECTOR_LABEL_WIDTH_FRAC,
};
use crate::app::design_tokens::spatial::{RADIUS_S, STROKE_WIDTH, spatial};
use crate::app::design_tokens::typography::TextRole;

// ─── Data Structures ──────────────────────────────────────────────────────

pub(crate) struct PropertyGroup {
    pub name: &'static str,
    pub icon: &'static str,
    pub properties: Vec<PropertyEntry>,
}

pub(crate) struct PropertyEntry {
    pub name: String,
    pub kind: PropertyKind,
    pub has_keyframes: bool,
    pub has_keyframe_at_current_time: bool,
    pub keyframe_count: usize,
}

#[derive(Debug, PartialEq)]
pub(crate) enum PropertyKind {
    Vec2 {
        x: f32,
        y: f32,
    },
    Float(f32),
    U32(u32),
    Bool(bool),
    Color([f32; 4]),
    Text(String),
    Union {
        variants: &'static [ValueType],
        value: PropertyValue,
    },
    Sum {
        variants: &'static [SumVariant],
        value: PropertyValue,
    },
    Enum {
        variants: &'static [&'static str],
        value: String,
    },
    EnumOwned {
        variants: Vec<String>,
        value: String,
    },
}

mod api;
mod build;
mod inputs;
mod render;
#[cfg(test)]
mod tests;

use api::*;
pub(crate) use build::build_property_groups;
pub(crate) use render::render_property_group;
