use crate::app::PanelState;
use crate::app::preview::{DragState, ToolMode};
use crate::app::preview::selection::SelectionState;
use crate::app::theme::Theme;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Owns all UI-specific state that does not affect the document or runtime.
pub struct UiStore {
    pub panel_state: PanelState,
    pub selected_actors: HashSet<String>,
    pub hit_regions: Vec<(String, kurbo::Rect)>,
    pub drag_state: DragState,
    pub selection: SelectionState,
    pub collapsed_actors: HashSet<String>,
    pub grid_enabled: bool,
    pub grid_size: f32,
    pub pivot_offsets: HashMap<String, [f32; 2]>,
    pub tool_mode: ToolMode,
    pub cursor_time_s: Option<f64>,
    pub editor_sync_enabled: bool,
    pub keyframe_mode: bool,
    pub inspector_input_drag_active: bool,
    pub settings_open: bool,
    pub export_dialog_open: bool,
    pub export_state: crate::app::ExportState,
    pub diagnostics_panel_visible: bool,
    pub last_error: Option<String>,
    pub clipboard_actors: Vec<String>,
}
