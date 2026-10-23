//! Unified insertion palette — fuzzy-searchable overlay for primitives, actions, and snippets.

use egui::{Color32, Pos2, Rect, RichText, Stroke, Vec2};
use eparts::{Button, Checkbox, NumberField, TextField};

use crate::app::GuiShell;
use crate::app::commands::UndoLabel;
use crate::app::components::{TabBar, text_tooltip};
use crate::app::design_tokens::semantic::category;
use crate::app::design_tokens::semantic::editor::SNIPPET_BLUE;
use crate::app::design_tokens::spatial::{RADIUS_S, RADIUS_XL, STROKE_WIDTH};
use crate::app::design_tokens::typography::TextRole;
use crate::app::insertion::{InsertionContext, InsertionRequest};

/// Mode filter for the palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteMode {
    Universal,
    Primitives,
    Effects,
    Actions,
    Snippets,
    Components,
}

/// A single item in the palette list.
#[derive(Debug, Clone)]
pub struct PaletteItem {
    pub label: String,
    pub detail: String,
    pub icon: String,
    pub color: Color32,
    pub kind: ItemKind,
}

#[derive(Debug, Clone)]
pub enum ItemKind {
    Primitive {
        type_name: String,
    },
    /// A post-processing effect stage for a `Filter` scope.
    Effect {
        type_name: String,
    },
    Action {
        verb: String,
    },
    #[allow(dead_code)] // Reserved for future snippet insertion type in palette
    Snippet {
        text: String,
    },
    Component {
        type_name: String,
        params: Vec<ParamInfo>,
    },
}

/// Parameter info with type annotation and default value.
#[derive(Debug, Clone)]
pub(crate) struct ParamInfo {
    pub name: String,
    pub param_type: Option<String>,
    pub default_str: Option<String>,
}

/// State for the insertion palette overlay.
pub struct InsertionPalette {
    pub open: bool,
    pub query: String,
    pub selected_index: usize,
    pub mode: PaletteMode,
    items: Vec<PaletteItem>,
    filtered: Vec<usize>, // indices into items
    param_form: Option<ParamFormState>,
}

#[derive(Debug, Clone)]
struct ParamFormState {
    type_name: String,
    /// (name, optional type name, current value)
    params: Vec<(String, Option<String>, String)>,
}

impl Default for InsertionPalette {
    fn default() -> Self {
        Self {
            open: false,
            query: String::new(),
            selected_index: 0,
            mode: PaletteMode::Universal,
            items: Vec::new(),
            filtered: Vec::new(),
            param_form: None,
        }
    }
}

fn category_color(category: animatix::timeline::ActorCategory, theme: eparts::Theme) -> Color32 {
    match category {
        animatix::timeline::ActorCategory::Shape => theme.palette.accent.primary,
        animatix::timeline::ActorCategory::Container => theme.palette.status.success,
        animatix::timeline::ActorCategory::Text => theme.palette.status.warning,
        animatix::timeline::ActorCategory::Media => category::ACTION,
        animatix::timeline::ActorCategory::Plot => theme.palette.accent.cyan,
        animatix::timeline::ActorCategory::Annotation => theme.palette.accent.cyan,
    }
}

fn action_category_color(category: &str, theme: eparts::Theme) -> Color32 {
    match category {
        "Entrance" => theme.palette.status.success,
        "Exit" => theme.palette.status.error,
        "Motion" => theme.palette.accent.primary,
        "Effects" => theme.palette.status.warning,
        "Reveal" => category::ACTION,
        "Reorder" => theme.palette.status.success,
        _ => theme.palette.text.secondary,
    }
}

fn format_num(val: f64) -> String {
    if val.fract() == 0.0 && val.abs() < 1_000_000.0 {
        format!("{}", val as i64)
    } else if (val * 100.0).fract() == 0.0 {
        format!("{:.2}", val)
    } else {
        format!("{}", val)
    }
}

/// Parse a Vec2 value string like "(100, 200)" or "100, 200".
fn parse_vec2_value(s: &str) -> (f64, f64) {
    let s = s.trim().trim_start_matches('(').trim_end_matches(')');
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() >= 2 {
        let x = parts[0].trim().parse::<f64>().unwrap_or(0.0);
        let y = parts[1].trim().parse::<f64>().unwrap_or(0.0);
        (x, y)
    } else {
        (0.0, 0.0)
    }
}

mod palette_impl;
mod shell_ui;
#[cfg(test)]
mod tests;
