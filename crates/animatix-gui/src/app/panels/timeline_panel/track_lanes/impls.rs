//! Lane grouping, group colors, and action category colors.

use egui::Color32;

use crate::app::design_tokens::semantic::category;

use super::{PROPERTY_GROUPS, PropertyGroup};

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
