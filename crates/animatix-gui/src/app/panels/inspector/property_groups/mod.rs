use super::property_groups_legend::legend_style_entries;
use animatix::timeline::{
    ActorField, EntityId, PROPERTY_REGISTRY, PropertyValue, SumVariant, ValueType,
    allowed_property_indices,
};

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
#[cfg(test)]
mod tests;

use api::*;
pub(crate) use build::build_property_groups;
