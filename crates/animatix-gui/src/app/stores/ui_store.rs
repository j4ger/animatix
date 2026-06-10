use crate::app::commands::ActionQueue;
use crate::app::components::toast::ToastQueue;
use crate::app::panels::SidebarTab;
use crate::app::panels::inspector::{KeyframeViewMode, PropertyViewMode};
use crate::app::preview::selection::SelectionState;
use crate::app::preview::{DragState, ToolMode};
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
    /// Pending source edits accumulated during a drag interaction.
    /// Flushed to source once the drag ends.
    /// Stored as a Vec so that list-property intermediates (e.g. child_order,
    /// points) are preserved rather than overwritten by later edits to the same
    /// (actor, property) pair.
    pub pending_drag_source_edits: Vec<crate::app::commands::PropertyEdit>,
}

impl InteractionStore {
    fn new() -> Self {
        Self {
            drag_state: DragState::None,
            drag_snapshot_taken: false,
            inspector_input_drag_active: false,
            pending_drag_source_edits: Vec::new(),
        }
    }

    /// Returns true if any drag interaction is active (canvas or inspector).
    /// This is the canonical check — callers should prefer this over inspecting
    /// individual flags.
    pub fn is_dragging(&self) -> bool {
        !matches!(self.drag_state, DragState::None) || self.inspector_input_drag_active
    }

    /// Reset all drag-related state. Called when any drag interaction ends.
    pub fn reset_drag_state(&mut self) {
        self.drag_state = DragState::None;
        self.inspector_input_drag_active = false;
        self.drag_snapshot_taken = false;
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
    pub shortcuts_open: bool,
    pub inspector_visible: bool,
    pub welcome_open: bool,
    pub workspace_switcher_open: bool,
    pub command_palette_open: bool,
    pub find_replace_open: bool,
    /// Currently active scene name (if in a multi-scene composition).
    pub active_scene: Option<String>,
    /// Timeline horizontal zoom factor.
    pub timeline_zoom: f32,
    /// Timeline horizontal scroll offset.
    pub timeline_scroll_offset: f32,
    /// Preview canvas zoom factor.
    pub preview_zoom: f32,
    /// Preview canvas pan offset.
    pub preview_pan: egui::Vec2,
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
            shortcuts_open: false,
            inspector_visible: false,
            welcome_open: false,
            workspace_switcher_open: false,
            command_palette_open: false,
            find_replace_open: false,
            active_scene: None,
            timeline_zoom: 1.0,
            timeline_scroll_offset: 0.0,
            preview_zoom: 1.0,
            preview_pan: egui::Vec2::ZERO,
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
    pub pending_actions: ActionQueue,
    pub toasts: ToastQueue,
    /// Path buffer for the workspace switcher dialog.
    pub workspace_switcher_path: String,
    /// Query string for the command palette.
    pub command_palette_query: String,
    /// Find/replace query string.
    pub find_query: String,
    /// Find/replace replacement string.
    pub replace_query: String,
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
            pending_actions: ActionQueue::default(),
            toasts: ToastQueue::default(),
            workspace_switcher_path: String::new(),
            command_palette_query: String::new(),
            find_query: String::new(),
            replace_query: String::new(),
        }
    }

    /// Capture current UI state as a snapshot.
    pub fn snapshot(&self) -> crate::app::document::history::UiSnapshot {
        use crate::app::document::history::UiSnapshot;
        UiSnapshot {
            active_scene: self.view.active_scene.clone(),
            selected_actors: self.selection.selected_actors.clone(),
            selected_keyframes: Vec::new(), // TODO: populate from keyframe selection
            playhead_time_s: 0.0,           // caller should set this from preview store
            loop_start_s: None,
            loop_end_s: None,
            timeline_zoom: self.view.timeline_zoom,
            timeline_scroll_offset: self.view.timeline_scroll_offset,
            preview_zoom: self.view.preview_zoom,
            preview_pan: (self.view.preview_pan.x, self.view.preview_pan.y),
            tool_mode: self.view.tool_mode,
        }
    }

    /// Restore UI state from a snapshot.
    pub fn restore_snapshot(&mut self, snapshot: crate::app::document::history::UiSnapshot) {
        self.view.active_scene = snapshot.active_scene;
        self.selection.selected_actors = snapshot.selected_actors;
        self.view.timeline_zoom = snapshot.timeline_zoom;
        self.view.timeline_scroll_offset = snapshot.timeline_scroll_offset;
        self.view.preview_zoom = snapshot.preview_zoom;
        self.view.preview_pan = egui::Vec2::new(snapshot.preview_pan.0, snapshot.preview_pan.1);
        self.view.tool_mode = snapshot.tool_mode;
        // Clear drag state on restore
        self.interaction.drag_state = crate::app::preview::DragState::None;
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
        assert!(!store.view.shortcuts_open);
        assert!(!store.view.debug_bounds);
        assert_eq!(store.view.tool_mode, ToolMode::Select);
    }
}
