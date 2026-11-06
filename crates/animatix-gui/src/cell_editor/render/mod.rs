use egui::{Color32, Frame, Margin, Pos2, RichText, ScrollArea, Stroke, Vec2};

use crate::app::components::{anim, text_tooltip};
use crate::app::design_tokens::motion;
use crate::app::design_tokens::spatial::{
    RADIUS_M, RADIUS_S, ROW_S, STROKE_WIDTH, Spatial, editor, spatial,
};
use crate::app::design_tokens::typography::TextRole;
use crate::app::design_tokens::util::lerp_color;
use crate::cell_editor::{Cell, CellAction, CellDiagnostic, CellEditorState, CellType};
use crate::highlighting::highlight_source;

mod body;
mod chrome;
mod frames;
mod impls;
#[cfg(test)]
mod tests;

use body::*;
use chrome::*;
pub use impls::render_cell_editor;
use impls::{apply_find_highlights, cell_analyzer_diagnostics, diagnostic_border_color};
