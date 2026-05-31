use crate::app::components::toast::ToastQueue;
use crate::app::panels::SidebarTab;
use crate::app::panels::inspector::{PropertyViewMode, KeyframeViewMode};
use crate::app::preview::{DragState, ToolMode};
use crate::app::preview::selection::SelectionState;
use crate::app::commands::CommandQueue;
use egui_tiles::Tree;
use std::collections::{HashMap, HashSet};

/// Selection state for the UI.
pub struct SelectionStore {
    pub selected_actors: HashSet<String>,
    pub hit_regions: Vec<(String, kurbo::Rect)>,
    pub selection: SelectionState,
}

impl SelectionStore {
    fn new() -> Self {
        Self {
            selected_actors: HashSet::new(),
            hit_regions: Vec::new(),
            selection: SelectionState::default(),
        }
    }
}

/// Interaction state (drag, inspector input, snapshots).
pub struct InteractionStore {
    pub drag_state: DragState,
    pub drag_snapshot_taken: bool,
    pub inspector_input_drag_active: bool,
}

impl InteractionStore {
    fn new() -> Self {
        Self {
            drag_state: DragState::None,
            drag_snapshot_taken: false,
            inspector_input_drag_active: false,
        }
    }
}

/// Clipboard buffer for copy/paste.
pub struct ClipboardStore {
    pub clipboard_actors: Vec<String>,
}

impl ClipboardStore {
    fn new() -> Self {
        Self {
            clipboard_actors: Vec::new(),
        }
    }
}

/// View settings and panel state.
pub struct ViewStore {
    pub tree: Tree<crate::app::WorkspaceTab>,
    pub collapsed_actors: HashSet<String>,
    pub diagnostics_panel_visible: bool,
    pub settings_open: bool,
    pub tool_mode: ToolMode,
    pub debug_bounds: bool,
    pub action_palette_open: bool,
    pub shortcuts_open: bool,
    pub inspector_visible: bool,
    pub welcome_open: bool,
    pub workspace_switcher_open: bool,
}

impl ViewStore {
    fn new(tree: Tree<crate::app::WorkspaceTab>) -> Self {
        Self {
            tree,
            collapsed_actors: HashSet::new(),
            diagnostics_panel_visible: false,
            settings_open: false,
            tool_mode: ToolMode::Select,
            debug_bounds: false,
            action_palette_open: false,
            shortcuts_open: false,
            inspector_visible: false,
            welcome_open: false,
            workspace_switcher_open: false,
        }
    }
}

/// Owns all UI-specific state that does not affect the document or runtime.
pub struct UiStore {
    pub selection: SelectionStore,
    pub interaction: InteractionStore,
    pub clipboard: ClipboardStore,
    pub view: ViewStore,
    pub editor_sync_enabled: bool,
    pub keyframe_mode: bool,
    pub cursor_time_s: Option<f64>,
    pub keyframe_merge_window_s: f64,
    pub pivot_offsets: HashMap<String, [f32; 2]>,
    pub sidebar_tab: SidebarTab,
    pub property_view_mode: PropertyViewMode,
    pub keyframe_view_mode: KeyframeViewMode,
    pub rebuild_debounce_ms: u64,
    pub scrub_step_s: f64,
    pub nudge_step_px: f32,
    pub nudge_step_shift_px: f32,
    pub rotation_snap_degrees: f32,
    pub pending_commands: CommandQueue,
    pub toasts: ToastQueue,
    /// Path buffer for the workspace switcher dialog.
    pub workspace_switcher_path: String,
}

impl UiStore {
    pub fn new(tree: Tree<crate::app::WorkspaceTab>) -> Self {
        Self {
            selection: SelectionStore::new(),
            interaction: InteractionStore::new(),
            clipboard: ClipboardStore::new(),
            view: ViewStore::new(tree),
            editor_sync_enabled: true,
            keyframe_mode: true,
            cursor_time_s: None,
            keyframe_merge_window_s: 0.05,
            pivot_offsets: HashMap::new(),
            sidebar_tab: SidebarTab::Explorer,
            property_view_mode: PropertyViewMode::Semantic,
            keyframe_view_mode: KeyframeViewMode::List,
            rebuild_debounce_ms: 150,
            scrub_step_s: 0.1,
            nudge_step_px: 1.0,
            nudge_step_shift_px: 10.0,
            rotation_snap_degrees: 15.0,
            pending_commands: CommandQueue::default(),
            toasts: ToastQueue::default(),
            workspace_switcher_path: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::persistence::default_tree;

    #[test]
    fn ui_store_new_creates_valid_store() {
        let tree = default_tree();
        let store = UiStore::new(tree);

        assert!(store.editor_sync_enabled);
        assert!(store.keyframe_mode);
        assert_eq!(store.cursor_time_s, None);
        assert_eq!(store.sidebar_tab, SidebarTab::Explorer);
        assert_eq!(store.property_view_mode, PropertyViewMode::Semantic);
        assert_eq!(store.keyframe_view_mode, KeyframeViewMode::List);
        assert_eq!(store.scrub_step_s, 0.1);
        assert_eq!(store.nudge_step_px, 1.0);
        assert_eq!(store.rotation_snap_degrees, 15.0);
    }

    #[test]
    fn selection_store_select_actor_adds_to_selected_actors() {
        let tree = default_tree();
        let mut store = UiStore::new(tree);

        store.selection.selected_actors.insert("box".to_string());
        store.selection.selected_actors.insert("circle".to_string());

        assert_eq!(store.selection.selected_actors.len(), 2);
        assert!(store.selection.selected_actors.contains("box"));
        assert!(store.selection.selected_actors.contains("circle"));
    }

    #[test]
    fn selection_store_clear_selection_empties_selected_actors() {
        let tree = default_tree();
        let mut store = UiStore::new(tree);

        store.selection.selected_actors.insert("box".to_string());
        store.selection.selected_actors.insert("circle".to_string());
        assert_eq!(store.selection.selected_actors.len(), 2);

        store.selection.selected_actors.clear();

        assert!(store.selection.selected_actors.is_empty());
    }

    #[test]
    fn view_store_defaults() {
        let tree = default_tree();
        let store = UiStore::new(tree);

        assert!(store.view.collapsed_actors.is_empty());
        assert!(!store.view.diagnostics_panel_visible);
        assert!(!store.view.settings_open);
        assert!(!store.view.action_palette_open);
        assert!(!store.view.shortcuts_open);
        assert!(!store.view.debug_bounds);
        assert_eq!(store.view.tool_mode, ToolMode::Select);
    }
}