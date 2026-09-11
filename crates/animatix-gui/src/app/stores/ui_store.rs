use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_tiles::Tree;

use super::PreviewStore;
use crate::app::commands::{ActionQueue, ShellAction};
use crate::app::components::toast::ToastQueue;
use crate::app::document::timeline_diff::KeyframeId;
use crate::app::panels::SidebarTab;
use crate::app::panels::inspector::{KeyframeViewMode, PropertyViewMode};
use crate::app::preview::selection::SelectionState;
use crate::app::preview::{DragState, ToolMode};

/// Selection state for the UI.
pub struct SelectionStore {
    pub selected_actors: HashSet<String>,
    pub hit_regions: Vec<(String, kurbo::Rect)>,
    pub selection: SelectionState,
    /// Canonical keyframe multi-selection, scene-qualified.
    pub selected_keyframes: Vec<KeyframeId>,
}

impl SelectionStore {
    fn new() -> Self {
        Self {
            selected_actors: HashSet::new(),
            hit_regions: Vec::new(),
            selection: SelectionState::default(),
            selected_keyframes: Vec::new(),
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

/// Default autosave interval when the user has no persisted preference.
pub const DEFAULT_AUTOSAVE_INTERVAL_S: f64 = 20.0;
/// Lower/upper bounds applied to a persisted or user-entered interval.
pub const MIN_AUTOSAVE_INTERVAL_S: f64 = 1.0;
pub const MAX_AUTOSAVE_INTERVAL_S: f64 = 3600.0;

/// Crash-recovery autosave preference plus timer bookkeeping.
///
/// The write itself is driven from `GuiShell::prepare_frame`; this only records
/// whether it is on, how often it should fire, and when it last did. Defaults
/// match `AppState`'s serde defaults so a fresh profile autosaves.
#[derive(Debug, Clone)]
pub struct AutosaveState {
    pub enabled: bool,
    pub interval: Duration,
    /// Source path targeted by the most recent recovery write. A change here
    /// means a different document is open, so the timer restarts and a stale
    /// sidecar never gets attributed to the new source.
    pub last_source_path: Option<PathBuf>,
    /// When the most recent recovery write happened.
    pub last_write: Option<Instant>,
}

impl Default for AutosaveState {
    fn default() -> Self {
        Self::new()
    }
}

impl AutosaveState {
    pub fn new() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs_f64(DEFAULT_AUTOSAVE_INTERVAL_S),
            last_source_path: None,
            last_write: None,
        }
    }

    /// Build from persisted preferences, clamping the interval to sane bounds.
    /// Non-finite input (corrupt RON, hand-edited NaN) falls back to the default
    /// rather than panicking in `Duration::from_secs_f64`.
    pub fn from_prefs(enabled: bool, interval_s: f64) -> Self {
        let interval_s = if interval_s.is_finite() {
            interval_s.clamp(MIN_AUTOSAVE_INTERVAL_S, MAX_AUTOSAVE_INTERVAL_S)
        } else {
            DEFAULT_AUTOSAVE_INTERVAL_S
        };
        Self {
            enabled,
            interval: Duration::from_secs_f64(interval_s),
            last_source_path: None,
            last_write: None,
        }
    }

    /// Preference pair for persistence.
    pub fn prefs(&self) -> crate::app::persistence::AutosavePrefs {
        crate::app::persistence::AutosavePrefs {
            enabled: self.enabled,
            interval_s: self.interval.as_secs_f64(),
        }
    }

    /// Whether a write is due. `last_write == None` means the document just
    /// became dirty (or the source path changed), so the first write is due
    /// immediately — a crash within the first interval must still recover.
    pub fn is_due(&self, now: Instant) -> bool {
        self.last_write
            .is_none_or(|last| now.saturating_duration_since(last) >= self.interval)
    }

    /// Time until the next write is due (zero when already due).
    pub fn remaining(&self, now: Instant) -> Duration {
        self.last_write.map_or(Duration::ZERO, |last| {
            self.interval.saturating_sub(now.saturating_duration_since(last))
        })
    }

    /// Point the timer at `source_path`, resetting it when the open document
    /// changed so the first write for the new document is due immediately.
    pub fn track_source(&mut self, source_path: &Path) {
        if self.last_source_path.as_deref() != Some(source_path) {
            self.last_source_path = Some(source_path.to_path_buf());
            self.last_write = None;
        }
    }

    /// Record that a recovery write was attempted at `now`.
    pub fn note_write(&mut self, now: Instant) {
        self.last_write = Some(now);
    }
}

/// Startup prompt offering to restore a newer crash-recovery sidecar.
///
/// There is deliberately no default-close path: Escape and backdrop clicks are
/// ignored by the renderer so the user must pick Recover or Discard. Until then
/// the sidecar is the only copy of the previous session's edits, so autosave
/// and save commands are held off (see `GuiShell::recovery_prompt_pending`).
#[derive(Debug, Clone, Default)]
pub struct RecoveryPrompt {
    pub is_open: bool,
    /// Document the sidecar belongs to (also the path the recovery will be
    /// written back to on the next autosave).
    pub source_path: Option<PathBuf>,
    /// Sidecar holding the recovered text.
    pub recovery_path: Option<PathBuf>,
    pub message: String,
}

impl RecoveryPrompt {
    pub fn open(&mut self, source_path: PathBuf, recovery_path: PathBuf) {
        self.is_open = true;
        self.message = format!(
            "Animatix found unsaved changes from a previous session for \"{}\".\n\n\
             Recover them into the editor, or discard the recovery file?",
            source_path.display()
        );
        self.source_path = Some(source_path);
        self.recovery_path = Some(recovery_path);
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.message.clear();
        self.source_path = None;
        self.recovery_path = None;
    }
}

/// View settings and panel state.
pub struct ViewStore {
    pub tree: Tree<crate::app::WorkspaceTab>,
    pub collapsed_actors: HashSet<String>,
    pub expanded_properties: HashSet<String>,
    pub diagnostics_panel_visible: bool,
    pub settings_open: bool,
    pub tool_mode: ToolMode,
    pub debug_bounds: bool,
    pub debug_layout: bool,
    pub debug_spacing: bool,
    pub shortcuts_open: bool,
    pub welcome_open: bool,
    pub workspace_switcher_open: bool,
    pub command_palette_open: bool,
    pub plugin_status_open: bool,
    pub find_replace_open: bool,
    /// Currently active scene name (if in a multi-scene composition).
    pub active_scene: Option<String>,
    /// Timeline horizontal scroll offset (zoom/pan live in PreviewPaneState.viewport).
    pub timeline_scroll_offset: f32,
    /// IDE appearance preference (Auto follows the OS light/dark setting).
    pub app_theme: eparts::AppThemeChoice,
    /// Optional JSON theme directory loaded through eparts `theme-json`.
    pub theme_dir: Option<PathBuf>,
    /// Selected theme name inside [`Self::theme_dir`].
    pub theme_name: Option<String>,
    /// Names available in the loaded JSON theme registry.
    #[cfg(feature = "theme-json")]
    pub theme_names: Vec<String>,
    /// Last JSON theme load/watch error, if any.
    #[cfg(feature = "theme-json")]
    pub theme_error: Option<String>,
    /// True when the timeline panel or any of its children received pointer interaction this
    /// frame.
    pub timeline_focused: bool,
    /// True when the user prefers reduced motion (animations snap to instant).
    pub reduce_motion: bool,
    /// Density preference for UI spacing.
    pub density: eparts::Density,
    /// Last observed window size, used to size layout presets/reset.
    pub layout_size: (f32, f32),
    /// Active layout preset, used to derive per-frame pixel bounds.
    pub layout_preset: crate::app::LayoutPreset,
    /// Crash-recovery autosave preference and timer state.
    pub autosave: AutosaveState,
}

impl ViewStore {
    fn new(tree: Tree<crate::app::WorkspaceTab>) -> Self {
        Self {
            tree,
            collapsed_actors: HashSet::new(),
            expanded_properties: HashSet::new(),
            diagnostics_panel_visible: false,
            settings_open: false,
            tool_mode: ToolMode::Select,
            debug_bounds: false,
            debug_layout: false,
            debug_spacing: false,
            shortcuts_open: false,
            welcome_open: false,
            workspace_switcher_open: false,
            command_palette_open: false,
            plugin_status_open: false,
            find_replace_open: false,
            active_scene: None,
            timeline_scroll_offset: 0.0,
            app_theme: eparts::AppThemeChoice::default(),
            theme_dir: None,
            theme_name: None,
            #[cfg(feature = "theme-json")]
            theme_names: Vec::new(),
            #[cfg(feature = "theme-json")]
            theme_error: None,
            timeline_focused: false,
            reduce_motion: false,
            density: eparts::Density::Default,
            layout_size: (1440.0, 960.0),
            layout_preset: crate::app::LayoutPreset::Animate,
            autosave: AutosaveState::new(),
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
    pub snap_fps: f32,
    pub pending_actions: ActionQueue,
    pub toasts: ToastQueue,
    /// Path buffer for the workspace switcher dialog.
    pub workspace_switcher_path: String,
    /// Selected index in the command palette list.
    pub command_palette_selected: usize,
    /// Query string for the command palette.
    pub command_palette_query: String,
    /// Find/replace query string.
    pub find_query: String,
    /// Find/replace replacement string.
    pub replace_query: String,
    /// Byte offset of the last Find Next match, for cursor-relative search.
    pub find_last_match: Option<usize>,
    /// Match case when searching.
    pub find_case_sensitive: bool,
    /// Match whole words only.
    pub find_whole_word: bool,
    /// Treat the find query as a regular expression.
    pub find_regex: bool,
    /// Unsaved changes confirmation dialog state.
    pub unsaved_changes: UnsavedChangesDialog,
    /// Startup crash-recovery prompt state.
    pub recovery_prompt: RecoveryPrompt,
    /// Persisted shortcut overrides keyed by stable binding name.
    pub shortcut_overrides:
        std::collections::BTreeMap<String, crate::app::interaction::keyboard::SavedShortcut>,
    /// Binding currently waiting for the next key press in the settings dialog.
    pub recording_shortcut: Option<String>,
    /// Path buffer for adding an explicit plugin manifest/library path.
    pub plugin_path_input: String,
    /// Recently opened files, newest first (populated from app-state persistence).
    pub recent_files: Vec<PathBuf>,
}

impl UiStore {
    pub fn new(tree: Tree<crate::app::WorkspaceTab>) -> Self {
        Self {
            selection: SelectionStore::new(),
            interaction: InteractionStore::new(),
            clipboard: ClipboardStore::new(),
            view: ViewStore::new(tree),
            editor_sync_enabled: true, // Auto-key is off by default: property edits change the base value
            // unless the user explicitly records or clicks a keyframe diamond.
            keyframe_mode: false,
            cursor_time_s: None,
            keyframe_merge_window_s: 0.05,
            pivot_offsets: HashMap::new(),
            sidebar_tab: SidebarTab::Project,
            property_view_mode: PropertyViewMode::Semantic,
            keyframe_view_mode: KeyframeViewMode::List,
            rebuild_debounce_ms: 150,
            scrub_step_s: 0.1,
            nudge_step_px: 1.0,
            nudge_step_shift_px: 10.0,
            rotation_snap_degrees: 15.0,
            snap_fps: 60.0,
            pending_actions: ActionQueue::default(),
            toasts: ToastQueue::default(),
            workspace_switcher_path: String::new(),
            command_palette_selected: 0,
            command_palette_query: String::new(),
            find_query: String::new(),
            replace_query: String::new(),
            find_last_match: None,
            find_case_sensitive: false,
            find_whole_word: false,
            find_regex: false,
            unsaved_changes: UnsavedChangesDialog::default(),
            recovery_prompt: RecoveryPrompt::default(),
            shortcut_overrides: std::collections::BTreeMap::new(),
            recording_shortcut: None,
            plugin_path_input: String::new(),
            recent_files: Vec::new(),
        }
    }

    /// Capture UI state plus playback/timeline state for undo/redo.
    pub fn snapshot_with_preview(
        &self,
        preview: &PreviewStore,
    ) -> crate::app::document::history::UiSnapshot {
        use crate::app::document::history::UiSnapshot;
        UiSnapshot {
            active_scene: self.view.active_scene.clone(),
            selected_actors: self.selection.selected_actors.clone(),
            selected_keyframes: self.selection.selected_keyframes.clone(),
            playhead_time_s: preview.preview.playback.current_time_s(),
            loop_start_s: preview.preview.playback.loop_start_s,
            loop_end_s: preview.preview.playback.loop_end_s,
            timeline_scroll_offset: preview.preview.timeline_scroll_offset,
            tool_mode: self.view.tool_mode,
        }
    }

    /// Restore UI state from a snapshot.
    pub fn restore_snapshot(&mut self, snapshot: crate::app::document::history::UiSnapshot) {
        self.view.active_scene = snapshot.active_scene;
        self.selection.selected_actors = snapshot.selected_actors;
        self.selection.selected_keyframes = snapshot.selected_keyframes;
        self.view.timeline_scroll_offset = snapshot.timeline_scroll_offset as f32;
        self.view.tool_mode = snapshot.tool_mode;
        // Clear drag state on restore
        self.interaction.drag_state = crate::app::preview::DragState::None;
    }
}

/// Confirmation dialog for unsaved changes.
#[derive(Debug, Clone, Default)]
pub struct UnsavedChangesDialog {
    pub is_open: bool,
    pub message: String,
    pub pending_close: bool,
    pub pending_action: Option<ShellAction>,
}

impl UnsavedChangesDialog {
    pub fn open(&mut self, message: impl Into<String>, action: ShellAction) {
        self.is_open = true;
        self.message = message.into();
        self.pending_action = Some(action);
        self.pending_close = false;
    }

    pub fn open_for_close(&mut self) {
        self.is_open = true;
        self.message = "Save changes before closing?".into();
        self.pending_action = None;
        self.pending_close = true;
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.message.clear();
        self.pending_action = None;
        self.pending_close = false;
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
        assert!(!store.keyframe_mode);
        assert_eq!(store.cursor_time_s, None);
        assert_eq!(store.sidebar_tab, SidebarTab::Project);
        assert_eq!(store.property_view_mode, PropertyViewMode::Semantic);
        assert_eq!(store.keyframe_view_mode, KeyframeViewMode::List);
        assert_eq!(store.scrub_step_s, 0.1);
        assert_eq!(store.nudge_step_px, 1.0);
        assert_eq!(store.rotation_snap_degrees, 15.0);
        assert_eq!(store.snap_fps, 60.0);
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
        assert!(!store.view.debug_layout);
        assert!(!store.view.debug_spacing);
        assert_eq!(store.view.tool_mode, ToolMode::Select);
    }

    #[test]
    fn autosave_defaults_to_enabled_twenty_seconds() {
        let store = UiStore::new(default_tree());

        assert!(store.view.autosave.enabled);
        assert_eq!(store.view.autosave.interval.as_secs_f64(), DEFAULT_AUTOSAVE_INTERVAL_S);
        assert!(store.view.autosave.is_due(Instant::now()), "first write is due immediately");
    }

    #[test]
    fn autosave_from_prefs_clamps_interval() {
        let tiny = AutosaveState::from_prefs(true, 0.0);
        assert_eq!(tiny.interval.as_secs_f64(), MIN_AUTOSAVE_INTERVAL_S);

        let huge = AutosaveState::from_prefs(false, 1.0e9);
        assert_eq!(huge.interval.as_secs_f64(), MAX_AUTOSAVE_INTERVAL_S);
        assert!(!huge.enabled);
    }

    #[test]
    fn autosave_due_respects_interval_and_write_time() {
        let mut state = AutosaveState::from_prefs(true, 5.0);
        let start = Instant::now();
        state.track_source(Path::new("/tmp/scene.amx"));
        assert!(state.is_due(start), "newly tracked source is due");

        state.note_write(start);
        assert!(!state.is_due(start));
        assert!(!state.is_due(start + Duration::from_secs(4)));
        assert_eq!(state.remaining(start + Duration::from_secs(4)), Duration::from_secs(1));
        assert!(state.is_due(start + Duration::from_secs(5)));
        assert_eq!(state.remaining(start + Duration::from_secs(9)), Duration::ZERO);
    }

    #[test]
    fn autosave_restarts_when_the_source_document_changes() {
        let mut state = AutosaveState::from_prefs(true, 30.0);
        let start = Instant::now();
        state.track_source(Path::new("/tmp/a.amx"));
        state.note_write(start);
        assert!(!state.is_due(start + Duration::from_secs(1)));

        // Re-tracking the same path keeps the timer...
        state.track_source(Path::new("/tmp/a.amx"));
        assert!(!state.is_due(start + Duration::from_secs(1)));

        // ...but a different document resets it so it can be saved promptly.
        state.track_source(Path::new("/tmp/b.amx"));
        assert!(state.is_due(start + Duration::from_secs(1)));
    }

    #[test]
    fn recovery_prompt_open_and_close_roundtrip() {
        let mut prompt = RecoveryPrompt::default();
        prompt.open(PathBuf::from("/tmp/scene.amx"), PathBuf::from("/tmp/scene.amx.autosave"));

        assert!(prompt.is_open);
        assert!(prompt.message.contains("scene.amx"));
        assert_eq!(prompt.source_path.as_deref(), Some(Path::new("/tmp/scene.amx")));
        assert_eq!(prompt.recovery_path.as_deref(), Some(Path::new("/tmp/scene.amx.autosave")));

        prompt.close();
        assert!(!prompt.is_open);
        assert!(prompt.message.is_empty());
        assert!(prompt.source_path.is_none());
        assert!(prompt.recovery_path.is_none());
    }
}
