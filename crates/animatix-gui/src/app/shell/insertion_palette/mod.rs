//! Unified insertion palette — fuzzy-searchable overlay for primitives, actions, and snippets.

use egui::{Color32, Pos2, Rect, RichText, Stroke, Vec2};
use eparts::{Button, Checkbox, NumberField, TextField};

use crate::app::GuiShell;
use crate::app::commands::Command;
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

mod impls;
mod palette_impl;
mod shell_ui;
#[cfg(test)]
mod tests;

use impls::*;
