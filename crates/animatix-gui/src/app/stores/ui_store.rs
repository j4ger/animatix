use crate::app::preview::{DragState, ToolMode};
use crate::app::preview::selection::SelectionState;
use crate::app::commands::CommandQueue;
use egui_tiles::Tree;
use std::collections::{HashMap, HashSet};

/// Owns all UI-specific state that does not affect the document or runtime.
pub struct UiStore {
    pub tree: Tree<crate::app::WorkspaceTab>,
    pub selected_actors: HashSet<String>,
    pub clipboard_actors: Vec<String>,
    pub hit_regions: Vec<(String, kurbo::Rect)>,
    pub drag_state: DragState,
    pub selection: SelectionState,
    pub drag_snapshot_taken: bool,
    pub inspector_input_drag_active: bool,
    pub editor_sync_enabled: bool,
    pub keyframe_mode: bool,
    pub cursor_time_s: Option<f64>,
    pub collapsed_actors: HashSet<String>,
    pub diagnostics_panel_visible: bool,
    pub settings_open: bool,
    pub tool_mode: ToolMode,
    pub debug_bounds: bool,
    pub keyframe_merge_window_s: f64,
    pub pivot_offsets: HashMap<String, [f32; 2]>,
    pub rebuild_debounce_ms: u64,
    pub scrub_step_s: f64,
    pub nudge_step_px: f32,
    pub nudge_step_shift_px: f32,
    pub rotation_snap_degrees: f32,
    pub pending_commands: CommandQueue,
}

impl UiStore {
    pub fn new(tree: Tree<crate::app::WorkspaceTab>) -> Self {
        Self {
            tree,
            selected_actors: HashSet::new(),
            clipboard_actors: Vec::new(),
            hit_regions: Vec::new(),
            drag_state: DragState::None,
            selection: SelectionState::default(),
            drag_snapshot_taken: false,
            inspector_input_drag_active: false,
            editor_sync_enabled: true,
            keyframe_mode: true,
            cursor_time_s: None,
            collapsed_actors: HashSet::new(),
            diagnostics_panel_visible: false,
            settings_open: false,
            tool_mode: ToolMode::Select,
            debug_bounds: false,
            keyframe_merge_window_s: 0.05,
            pivot_offsets: HashMap::new(),
            rebuild_debounce_ms: 150,
            scrub_step_s: 0.1,
            nudge_step_px: 1.0,
            nudge_step_shift_px: 10.0,
            rotation_snap_degrees: 15.0,
            pending_commands: CommandQueue::default(),
        }
    }
}