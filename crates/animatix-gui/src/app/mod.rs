#![allow(dead_code)]

mod actions;
pub(crate) mod command_handlers;
pub(crate) mod commands;
pub(crate) mod components;
mod file_tree;
pub(crate) mod icons;
pub(crate) mod panels;
mod persistence;
mod preview;
mod runtime;
mod shell;
pub mod theme;
mod utils;

use crate::document::{DocumentSession, default_file_path, timeline_keyframe_times_s};
use crate::hot_reload::{HotReloader, ReloadStatus};
use crate::editor::EditorBuffer;
use crate::text_diff::diff_text;
use crate::preview_surface::PreviewSurface;
use animatix::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase, diagnostics_phase_summary};
use animatix::renderer::video::ExportError;
use animatix::timeline::SceneDimensions;
use directories::ProjectDirs;
use egui::{Color32, Stroke, Vec2};
use egui_tiles::{Tile, Tree};
use file_tree::{build_file_tree, workspace_root_for};
use persistence::{default_tree, load_workspace_persistence, persistence_path};
#[cfg(test)]
use preview::fit_preview;
use preview::DragState;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::time::{Duration, Instant};
use crate::app::commands::{Command, CommandQueue, UndoEntry};
use crate::app::panels::WorkspaceViewer;
use crate::app::utils::*;

const INITIAL_WINDOW_SIZE: (f64, f64) = (1440.0, 960.0);
const DEFAULT_PREVIEW_SIZE: SceneDimensions = SceneDimensions {
    width: 1920,
    height: 1080,
};
const MAX_TREE_DEPTH: usize = 4;
const MAX_TREE_ENTRIES: usize = 200;
const EXPLORER_INDENT_PX: f32 = 10.0;

pub use runtime::run_gui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum WorkspaceTab {
    Sidebar,
    Editor,
    Preview,
    Inspector,
    Timeline,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkspacePersistence {
    tree: Tree<WorkspaceTab>,
}

#[derive(Debug, Clone)]
struct FileTreeEntry {
    path: PathBuf,
    name: String,
    depth: usize,
    is_dir: bool,
}

pub(crate) struct PreviewPaneState {
    current_time_s: f64,
    duration_s: f64,
    is_playing: bool,
    status: String,
    error: Option<String>,
    dimensions: SceneDimensions,
    /// Preview canvas zoom level (1.0 = 100%).
    preview_zoom: f32,
    /// Preview canvas pan offset in scene coordinates (scene point centered in preview).
    preview_pan: Vec2,
    /// Playback speed multiplier (0.25, 0.5, 1.0, 2.0).
    playback_speed: f32,
    /// Loop region start time (A marker). None = not set.
    loop_start_s: Option<f64>,
    /// Loop region end time (B marker). None = not set.
    loop_end_s: Option<f64>,
    /// Horizontal guide positions in scene y-coordinates (pixels).
    horizontal_guides: Vec<f32>,
    /// Vertical guide positions in scene x-coordinates (pixels).
    vertical_guides: Vec<f32>,
    /// Snap lines to draw this frame (cleared at start of each preview_ui).
    snap_lines_h: Vec<f32>,
    /// Snap lines to draw this frame (cleared at start of each preview_ui).
    snap_lines_v: Vec<f32>,
    /// Color of the current snap lines.
    snap_line_color: Option<Color32>,
    /// Whether smart snap is enabled during drag.
    snap_enabled: bool,
    /// Snap threshold in scene pixels. Default 10.0.
    snap_threshold: f32,
    /// HUD label text when snapped (e.g. "Circle_2 center", "Container left").
    snap_hud_label: Option<String>,
    /// Time lens HUD state (Space-drag time scrubbing).
    pub time_lens: crate::app::preview::time_lens::TimeLens,
}

/// Transient UI state for panels (not preview/playback state).
pub(crate) struct PanelState {
    /// When set, the scene list panel should open the transition editor for this scene.
    pub open_transition_editor: Option<String>,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            open_transition_editor: None,
        }
    }
}

impl PreviewPaneState {
    fn new(duration_s: f64, dimensions: SceneDimensions) -> Self {
        Self {
            current_time_s: 0.0,
            duration_s,
            is_playing: false,
            status: "Loaded file".to_string(),
            error: None,
            dimensions,
            preview_zoom: 1.0,
            preview_pan: Vec2::new(dimensions.width as f32 / 2.0, dimensions.height as f32 / 2.0),
            playback_speed: 1.0,
            loop_start_s: None,
            loop_end_s: None,
            horizontal_guides: vec![],
            vertical_guides: vec![],
            snap_lines_h: vec![],
            snap_lines_v: vec![],
            snap_line_color: None,
            snap_enabled: true,
            snap_threshold: 10.0,
            snap_hud_label: None,
            time_lens: crate::app::preview::time_lens::TimeLens::default(),
        }
    }

    fn clamp_time(&mut self) {
        let max_duration = self.duration_s.max(0.1);
        self.current_time_s = self.current_time_s.clamp(0.0, max_duration);
    }

    fn go_to_next_keyframe(&mut self, keyframes: &[f64]) {
        if keyframes.is_empty() {
            return;
        }
        let next = keyframes
            .iter()
            .find(|&&t| t > self.current_time_s)
            .copied()
            .unwrap_or(self.duration_s);
        self.current_time_s = next;
        self.clamp_time();
        self.is_playing = false;
    }

    fn go_to_previous_keyframe(&mut self, keyframes: &[f64]) {
        if keyframes.is_empty() {
            return;
        }
        let prev = keyframes
            .iter()
            .rev()
            .find(|&&t| t < self.current_time_s)
            .copied()
            .unwrap_or(0.0);
        self.current_time_s = prev;
        self.clamp_time();
        self.is_playing = false;
    }

    fn toggle_playback(&mut self) {
        if self.current_time_s >= self.duration_s {
            self.current_time_s = 0.0;
        }
        self.is_playing = !self.is_playing;
    }

    fn tick(&mut self, delta: Duration) {
        if !self.is_playing {
            return;
        }

        self.current_time_s += delta.as_secs_f64() * self.playback_speed as f64;

        // Loop region: if A and B are set and we've reached B, jump back to A.
        if let (Some(start), Some(end)) = (self.loop_start_s, self.loop_end_s) {
            if end > start && self.current_time_s >= end {
                self.current_time_s = start;
            }
        }

        if self.current_time_s >= self.duration_s {
            self.current_time_s = self.duration_s;
            self.is_playing = false;
        }
    }
}

struct GuiShell {
    document: DocumentSession,
    render_diagnostics: Vec<Diagnostic>,
    editor: EditorBuffer,
    workspace_root: PathBuf,
    expanded_dirs: HashSet<PathBuf>,
    file_tree: Vec<FileTreeEntry>,
    tree: Tree<WorkspaceTab>,
    preview: PreviewPaneState,
    panel_state: PanelState,
    preview_dirty: bool,
    pending_rebuild_at: Option<Instant>,
    last_frame_at: Instant,
    persistence_path: PathBuf,
    hot_reloader: Option<HotReloader>,
    last_reload_time: Option<Instant>,
    selected_actors: HashSet<String>,
    /// Actor labels stored in the clipboard (for Ctrl+C/V paste).
    clipboard_actors: Vec<String>,
    /// Per-actor hit regions from the last render (for click-to-select).
    hit_regions: Vec<(String, kurbo::Rect)>,
    /// Current drag interaction state on the preview canvas.
    drag_state: DragState,
    /// Selection system state (hover, cycling, context menu).
    selection: crate::app::preview::selection::SelectionState,
    /// Undo stack — stores the command and source text before it was applied.
    undo_stack: Vec<UndoEntry>,
    /// Redo stack — stores the command and source text before it was applied.
    redo_stack: Vec<UndoEntry>,
    /// Whether we've already taken an undo snapshot for the current drag.
    /// One drag-start → drag-end counts as a single undo entry.
    drag_snapshot_taken: bool,
    /// Whether an inspector input (DragValue, Slider, etc.) is currently
    /// being dragged. Used to coalesce undo snapshots during inspector drags.
    inspector_input_drag_active: bool,
    /// When true, scrubbing the timeline scrolls the editor to the corresponding keyframe.
    editor_sync_enabled: bool,
    /// When true, property edits create keyframes at current time instead of overwriting defaults.
    keyframe_mode: bool,
    /// Time on the timeline corresponding to the editor cursor position (for bi-directional sync).
    cursor_time_s: Option<f64>,
    /// Actor labels that the user has explicitly collapsed in the layer tree.
    /// All actors are expanded by default.
    collapsed_actors: HashSet<String>,
    /// Whether the bottom diagnostics panel is visible.
    diagnostics_panel_visible: bool,
    /// Whether the settings dialog is currently open.
    settings_open: bool,
    /// Whether the export dialog is currently open.
    export_dialog_open: bool,
    /// State for the export dialog (format, resolution, etc.).
    export_state: crate::app::shell::export_dialog::ExportDialogState,
    /// Current export operation status.
    export_status: crate::app::shell::export_dialog::ExportStatus,
    /// Handle to the background export thread.
    export_thread: Option<std::thread::JoinHandle<(Result<(), ExportError>, PathBuf)>>,
    /// Shared progress counter for the active export (frames completed).
    export_progress: Arc<AtomicU32>,
    /// Shared cancellation flag for the active export.
    export_cancelled: Arc<AtomicBool>,
    /// When the current export started (for elapsed-time display).
    export_start_time: Option<Instant>,
    /// Total frames expected for the current export.
    export_total_frames: u32,
    /// Draw debug bounding boxes on preview and exports.
    debug_bounds: bool,
    /// Keyframe merge window in seconds. Edits within this window of the
    /// previous keyframe are merged instead of creating a new timestamp.
    keyframe_merge_window_s: f64,
    /// Whether grid snapping is enabled in the preview canvas.
    grid_enabled: bool,
    /// Grid size in pixels.
    grid_size: f32,
    /// Per-actor pivot offsets in object-local space (relative to actor centre).
    pivot_offsets: HashMap<String, [f32; 2]>,
    /// Active tool mode for preview canvas interactions.
    tool_mode: preview::ToolMode,
    /// Arrow-key scrub step in seconds.
    scrub_step_s: f64,
    /// Arrow-key nudge step in pixels (no modifier).
    nudge_step_px: f32,
    /// Arrow-key nudge step in pixels (Shift held).
    nudge_step_shift_px: f32,
    /// Rotation snap increment in degrees (Shift+rotate).
    rotation_snap_degrees: f32,
    /// Maximum undo history entries.
    undo_limit: usize,
    /// Rebuild debounce delay in milliseconds.
    rebuild_debounce_ms: u64,
    /// Commands pushed from outside the ui() loop (e.g. keyboard shortcuts in runtime.rs).
    pending_commands: CommandQueue,
}

impl GuiShell {
    fn apply_source_edit(&mut self, new_source: String) {
        let old_source = self.document.source_text.clone();
        let edits = diff_text(&old_source, &new_source);
        self.document.source_text = new_source;
        self.editor.apply_edits(&edits);
        self.document.is_dirty = true;
    }

    fn check_hot_reload(&mut self, app_time: Instant) {
        if let Some(ref mut reloader) = self.hot_reloader {
            match reloader.update(app_time) {
                ReloadStatus::ShouldReload { path: _ } => {
                    // LiveDocument: editor is the source of truth. If the editor
                    // has unsaved changes, do NOT silently overwrite them.
                    if self.document.is_dirty {
                        self.preview.status = "External file changed • reload blocked (unsaved edits)".to_string();
                        return;
                    }
                    if let Err(err) = self.document.reload_from_disk() {
                        self.preview.error = Some(err);
                        self.preview.status = "Hot reload failed".to_string();
                    } else {
                        self.editor
                            .set_document(&self.document.file_path, self.document.source_text.clone());
                        self.last_reload_time = Some(app_time);
                        self.preview.status = "File reloaded".to_string();
                        self.preview.error = None;
                    }
                }
                ReloadStatus::NoChange => {}
            }
        }
    }

    fn load(initial_path: PathBuf) -> Self {
        let (document, status, error) = match DocumentSession::load(initial_path.clone()) {
            Ok(document) => {
                let error = document.last_rebuild_error.clone();
                (document, None, error)
            }
            Err(error) => (
                DocumentSession::from_error(initial_path.clone()),
                Some("Failed to initialize session".to_string()),
                Some(error),
            ),
        };

        let workspace_root = workspace_root_for(&document.file_path);
        let expanded_dirs = HashSet::from([workspace_root.clone()]);
        let file_tree = build_file_tree(&workspace_root, &document.file_path, &expanded_dirs);
        let persistence_path = persistence_path();
        let tree = load_workspace_persistence(&persistence_path).unwrap_or_else(default_tree);
        let hot_reloader = HotReloader::new(&document.file_path).ok();
        let duration_s = document.duration_s.max(0.1);
        let mut preview = PreviewPaneState::new(duration_s, document.scene_dimensions);
        if let Some(status) = status {
            preview.status = status;
        } else if has_source_load_failure(&document.diagnostics) {
            preview.status = format!(
                "Opened {} • parse/load error • {}",
                document.file_path.display(),
                diagnostics_phase_summary(&document.diagnostics)
            );
        }
        preview.error = error;

        Self {
            editor: EditorBuffer::new(&document.file_path, document.source_text.clone()),
            document,
            render_diagnostics: Vec::new(),
            workspace_root,
            expanded_dirs,
            file_tree,
            tree,
            preview,
            panel_state: PanelState::default(),
            preview_dirty: true,
            pending_rebuild_at: None,
            last_frame_at: Instant::now(),
            persistence_path,
            hot_reloader,
            last_reload_time: None,
            selected_actors: HashSet::new(),
            clipboard_actors: Vec::new(),
            hit_regions: Vec::new(),
            drag_state: DragState::None,
            selection: crate::app::preview::selection::SelectionState::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            drag_snapshot_taken: false,
            inspector_input_drag_active: false,
            editor_sync_enabled: true,
            keyframe_mode: false,
            cursor_time_s: None,
            collapsed_actors: HashSet::new(),
            diagnostics_panel_visible: false,
            settings_open: false,
            export_dialog_open: false,
            export_state: crate::app::shell::export_dialog::ExportDialogState::default(),
            export_status: crate::app::shell::export_dialog::ExportStatus::Idle,
            export_thread: None,
            export_progress: Arc::new(AtomicU32::new(0)),
            export_cancelled: Arc::new(AtomicBool::new(false)),
            export_start_time: None,
            export_total_frames: 0,
            debug_bounds: false,
            keyframe_merge_window_s: 0.05,
            grid_enabled: true,
            grid_size: 20.0,
            pivot_offsets: HashMap::new(),
            tool_mode: preview::ToolMode::Select,
            scrub_step_s: 0.1,
            nudge_step_px: 1.0,
            nudge_step_shift_px: 10.0,
            rotation_snap_degrees: 15.0,
            undo_limit: 100,
            rebuild_debounce_ms: 150,
            pending_commands: CommandQueue::default(),
        }
    }

    fn is_playing(&self) -> bool {
        self.preview.is_playing
    }

    fn has_pending_rebuild(&self) -> bool {
        self.pending_rebuild_at.is_some()
    }

    fn prepare_frame(&mut self) {
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.last_frame_at);
        self.last_frame_at = now;

        // Check for hot reload
        self.check_hot_reload(now);

        // Poll background export status
        self.poll_export_status();

        if self.preview.is_playing {
            self.preview.tick(delta);
            self.preview_dirty = true;

            if self.editor_sync_enabled {
                if let Some(line) = self.document.find_keyframe_line_at(self.preview.current_time_s) {
                    if self.editor.highlighted_line != Some(line) {
                        self.editor.scroll_to_line(line);
                        self.editor.set_highlighted_line(Some(line));
                    }
                }
            }
        }

        self.sync_active_scene_from_time();

        if let Some(deadline) = self.pending_rebuild_at
            && now >= deadline
        {
            self.pending_rebuild_at = None;
            // Clear any stale error before rebuild so a successful rebuild
            // doesn't leave an outdated error banner visible.
            self.preview.error = None;
            let _ = self.rebuild();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, preview_texture_id: Option<egui::TextureId>) {
        let mut commands: CommandQueue = CommandQueue::default();
        commands.append(&mut self.pending_commands);

        // Global keyboard shortcuts for timeline toggles
        ui.input(|i| {
            if i.key_pressed(egui::Key::S) && !i.modifiers.command {
                commands.push_back(Command::ToggleEditorSync);
            }
            if i.key_pressed(egui::Key::K) && !i.modifiers.command {
                commands.push_back(Command::ToggleKeyframeMode);
            }
            // Copy selected actors (Ctrl+C)
            if i.modifiers.command && i.key_pressed(egui::Key::C) {
                if !self.selected_actors.is_empty() {
                    self.copy_selected_actors();
                }
            }
            // Paste actors (Ctrl+V)
            if i.modifiers.command && i.key_pressed(egui::Key::V) {
                if !self.clipboard_actors.is_empty() {
                    commands.push_back(Command::PasteActors);
                }
            }
        });

        // Compact toolbar
        egui::Panel::top("toolbar")
            .resizable(false)
            .show_inside(ui, |ui| self.toolbar_ui(ui, &mut commands));

        // NL Command Bar
        egui::Panel::top("nl_command_bar")
            .resizable(false)
            .show_inside(ui, |ui| {
                shell::nl_command_bar::nl_command_bar_ui(ui, &mut commands);
            });

        // Transport bar at the very bottom
        let keyframe_count = self
            .document
            .active_timeline()
            .map(|t| t.keyframe_times_s().len())
            .unwrap_or(0);
        let actor_count = self
            .document
            .active_timeline()
            .map(|t| t.tracks.len())
            .unwrap_or(0);
        let timeline_markers = timeline_keyframe_times_s(
            if self.document.composition.is_some() {
                None
            } else {
                self.document.active_timeline()
            },
            self.document.composition.as_ref(),
            self.document.active_scene.as_deref(),
        );
        let has_error = self.preview.error.is_some();
        let diagnostics = self.combined_diagnostics();

        egui::Panel::bottom("transport_bar")
            .resizable(false)
            .show_inside(ui, |ui| {
                shell::transport_bar::transport_bar_ui(
                    ui,
                    &mut self.preview,
                    &mut self.panel_state,
                    self.document.scene_dimensions,
                    &timeline_markers,
                    actor_count,
                    keyframe_count,
                    self.document.is_dirty,
                    has_error,
                    &diagnostics,
                    &mut commands,
                    self.editor_sync_enabled,
                    self.keyframe_mode,
                    self.cursor_time_s,
                    self.document.composition.as_ref(),
                    self.document.active_scene.as_deref(),
                );
            });

        // Diagnostics panel (above transport bar, collapsible)
        if self.diagnostics_panel_visible && !diagnostics.is_empty() {
            egui::Panel::bottom("diagnostics_panel")
                .resizable(true)
                .default_size(180.0)
                .min_size(80.0)
                .max_size(400.0)
                .show_inside(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if let Some(target) =
                        components::diagnostics_list(ui, &diagnostics)
                    {
                        self.editor.focus_diagnostic(target.line, target.column);
                    }
                });
        }

        // Central workspace — edge-to-edge tiles, no outer margin
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin::ZERO))
            .show_inside(ui, |ui| {
                self.workspace_ui(ui, preview_texture_id, &mut commands);
            });

        // Update cursor time from editor position (bi-directional sync)
        self.cursor_time_s = self
            .editor
            .cursor_line
            .and_then(|line| self.document.timeline_index.time_s_for_line(line));

        self.handle_commands(commands);

        // Settings modal overlay (rendered on top of everything)
        if self.settings_open {
            self.settings_dialog_ui(ui);
        }

        // Export dialog overlay
        if self.export_dialog_open {
            self.export_dialog_ui(ui);
        }
    }

    fn workspace_ui(
        &mut self,
        ui: &mut egui::Ui,
        preview_texture_id: Option<egui::TextureId>,
        commands: &mut CommandQueue,
    ) {
        let diagnostics = self.combined_diagnostics();

        let scene_dimensions = self.document.scene_dimensions;

        let viewer = WorkspaceViewer {
            scene_names: self.document.scene_names(),
            import_aliases: self.document.import_aliases(),
            active_scene: self.document.active_scene.clone(),
            is_composition: self.document.is_composition(),
            composition: self.document.composition.as_ref(),
            current_file: &self.document.file_path,
            workspace_root: &self.workspace_root,
            expanded_dirs: &mut self.expanded_dirs,
            file_tree: &self.file_tree,
            editor: &mut self.editor,
            preview: &mut self.preview,
            panel_state: &mut self.panel_state,
            diagnostics: &diagnostics,
            preview_texture_id,
            commands,
            source_dirty: &mut self.document.source_text,
            scene_dimensions,
            timeline: self.document.timeline.as_ref(),
            selected_actors: &mut self.selected_actors,
            hit_regions: &self.hit_regions,
            drag_state: &mut self.drag_state,
            selection: &mut self.selection,
            keyframe_mode: self.keyframe_mode,
            collapsed_actors: &mut self.collapsed_actors,
            grid_enabled: &mut self.grid_enabled,
            grid_size: &mut self.grid_size,
            pivot_offsets: &mut self.pivot_offsets,
            tool_mode: &mut self.tool_mode,
            rotation_snap_degrees: self.rotation_snap_degrees,
        };

        let mut behavior = panels::behavior::WorkspaceBehavior { viewer };
        self.tree.ui(&mut behavior, ui);
    }

    fn handle_commands(&mut self, commands: CommandQueue) {
        for command in commands {
            self.handle_command(command);
        }
    }

    fn open_document(&mut self, path: PathBuf) {
        match DocumentSession::load(path.clone()) {
            Ok(document) => {
                let new_workspace_root = workspace_root_for(&path);
                if new_workspace_root != self.workspace_root {
                    self.workspace_root = new_workspace_root;
                    self.expanded_dirs = HashSet::from([self.workspace_root.clone()]);
                }
                self.file_tree = build_file_tree(&self.workspace_root, &path, &self.expanded_dirs);
                self.document = document;
                self.editor
                    .set_document(&self.document.file_path, self.document.source_text.clone());
                // Clear undo/redo history when switching files
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.drag_snapshot_taken = false;
                self.inspector_input_drag_active = false;
                if let Some(ref mut reloader) = self.hot_reloader {
                    let _ = reloader.update_watched_file(&self.document.file_path);
                }
                let status = if has_source_load_failure(&self.document.diagnostics) {
                    format!(
                        "Opened {} • parse/load error • {}",
                        self.document.file_path.display(),
                        diagnostics_phase_summary(&self.document.diagnostics)
                    )
                } else {
                    self.document_status(format!("Opened {}", self.document.file_path.display()))
                };
                let error = self.document.last_rebuild_error.clone();
                self.sync_preview_from_document(status, true, true);
                self.preview.error = error;
            }
            Err(error) => {
                self.preview.error = Some(error.clone());
                self.preview.status = format!("Open failed • {}", path.display());
            }
        }
    }

    fn save(&mut self) -> Result<(), String> {
        let text = self.editor.text().to_string();
        std::fs::write(&self.document.file_path, &text)
            .map_err(|err| format!("Failed to save {}: {err}", self.document.file_path.display()))?;
        self.document.source_text = text;
        self.document.is_dirty = false;
        self.preview.status = format!("Saved {}", self.document.file_path.display());
        Ok(())
    }

    fn reload(&mut self) -> Result<(), String> {
        self.document.reload_from_disk()?;
        self.editor
            .set_document(&self.document.file_path, self.document.source_text.clone());
        let status = if has_source_load_failure(&self.document.diagnostics) {
            format!(
                "Reloaded {} • parse/load error • {}",
                self.document.file_path.display(),
                diagnostics_phase_summary(&self.document.diagnostics)
            )
        } else {
            self.document_status(format!("Reloaded {}", self.document.file_path.display()))
        };
        let error = self.document.last_rebuild_error.clone();
        self.sync_preview_from_document(status, false, false);
        self.preview.error = error;
        self.file_tree = build_file_tree(&self.workspace_root, &self.document.file_path, &self.expanded_dirs);
        Ok(())
    }

    fn rebuild(&mut self) -> Result<(), String> {
        match self.document.rebuild() {
            Ok(()) => {
                let status = if self.document.diagnostics.is_empty() {
                    format!(
                        "Built timeline • {:.2}s total duration",
                        self.document.duration_s.max(0.1)
                    )
                } else {
                    format!(
                        "Built timeline • {:.2}s total duration • {}",
                        self.document.duration_s.max(0.1),
                        diagnostics_phase_summary(&self.document.diagnostics)
                    )
                };
                self.sync_preview_from_document(status, false, false);
                Ok(())
            }
            Err(error) => {
                let status = if has_source_load_failure(&self.document.diagnostics) {
                    format!(
                        "Rebuild blocked • parse/load error • {}",
                        diagnostics_phase_summary(&self.document.diagnostics)
                    )
                } else {
                    "Rebuild blocked".to_string()
                };
                self.preview.duration_s = self.document.duration_s.max(0.1);
                self.preview.dimensions = self.document.scene_dimensions;
                self.preview.clamp_time();
                self.preview.status = status;
                self.preview.error = Some(error.clone());
                self.preview_dirty = true;
                Err(error)
            }
        }
    }

    fn document_status(&self, base_status: String) -> String {
        if self.document.diagnostics.is_empty() {
            base_status
        } else {
            format!(
                "{base_status} • {}",
                diagnostics_phase_summary(&self.document.diagnostics)
            )
        }
    }

    fn combined_diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = self.document.diagnostics.clone();
        diagnostics.extend(self.render_diagnostics.iter().cloned());
        diagnostics
    }

    fn sync_preview_from_document(
        &mut self,
        status: String,
        reset_time: bool,
        stop_playback: bool,
    ) {
        self.preview.duration_s = self.document.duration_s.max(0.1);
        self.preview.dimensions = self.document.scene_dimensions;
        if reset_time {
            self.preview.current_time_s = 0.0;
            self.preview.preview_zoom = 1.0;
            self.preview.preview_pan = Vec2::new(
                self.document.scene_dimensions.width as f32 / 2.0,
                self.document.scene_dimensions.height as f32 / 2.0,
            );
        } else {
            self.preview.clamp_time();
        }
        if stop_playback {
            self.preview.is_playing = false;
        }
        self.clear_any_error(status);
        self.preview_dirty = true;
    }

    fn sync_active_scene_from_time(&mut self) {
        if let Some(composition) = self.document.composition.as_ref() {
            let (scene, _, _) = composition.evaluate(self.preview.current_time_s);
            self.document.active_scene = (!scene.is_empty()).then_some(scene);
        }
    }

    fn set_status(&mut self, status: String, error: Option<String>) {
        self.preview.status = status;
        self.preview.error = error;
    }

    fn set_render_error(&mut self, error: String) {
        self.render_diagnostics = vec![Diagnostic::error(
            DiagnosticCode::RenderFailure,
            DiagnosticPhase::Render,
            error.clone(),
        )];
        self.preview_dirty = false;
        self.set_status(format!("Render failed • {error}"), Some(error));
    }

    fn clear_render_error(&mut self, status: String) {
        let active_render_error = self
            .render_diagnostics
            .first()
            .map(|diagnostic| diagnostic.message.clone());
        self.render_diagnostics.clear();

        if let Some(render_error) = active_render_error
            && self.preview.error.as_deref() == Some(render_error.as_str())
            && self.preview.status == format!("Render failed • {render_error}")
        {
            self.preview.error = None;
            self.preview.status = status;
        }
    }

    /// Force-clear any active error state (parse or render).
    fn clear_any_error(&mut self, status: String) {
        self.render_diagnostics.clear();
        self.preview.error = None;
        self.preview.status = status;
    }

    fn save_persistence(&self) {
        if let Some(parent) = self.persistence_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let persistence = WorkspacePersistence {
            tree: self.tree.clone(),
        };
        if let Ok(serialized) =
            ron::ser::to_string_pretty(&persistence, ron::ser::PrettyConfig::default())
        {
            let _ = fs::write(&self.persistence_path, serialized);
        }
    }

    /// Take a snapshot of the current source text for undo/redo.
    /// Call this BEFORE making a change to the source.
    fn snapshot(&mut self, command: Command) {
        self.undo_stack.push(UndoEntry {
            command,
            source_before: self.document.source_text.clone(),
        });
        self.redo_stack.clear();
        // Limit undo history
        if self.undo_stack.len() > self.undo_limit {
            self.undo_stack.remove(0);
        }
    }

    /// Undo the last command by restoring the source text captured before it ran.
    fn undo(&mut self) {
        if let Some(entry) = self.undo_stack.pop() {
            self.redo_stack.push(UndoEntry {
                command: entry.command,
                source_before: self.document.source_text.clone(),
            });
            self.document.source_text = entry.source_before.clone();
            self.editor.replace_text(entry.source_before);
            self.document.is_dirty = true;
            self.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
            self.preview.status = "Undo".to_string();
        }
    }

    /// Redo the last undone command.
    fn redo(&mut self) {
        if let Some(entry) = self.redo_stack.pop() {
            self.undo_stack.push(UndoEntry {
                command: entry.command,
                source_before: self.document.source_text.clone(),
            });
            self.document.source_text = entry.source_before.clone();
            self.editor.replace_text(entry.source_before);
            self.document.is_dirty = true;
            self.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
            self.preview.status = "Redo".to_string();
        }
    }

    fn open_workspace_tab(&mut self, target: WorkspaceTab) {
        let _ = self.tree.make_active(|_, tile| matches!(tile, Tile::Pane(tab) if *tab == target));
    }

    /// Duplicate an actor, preserving its type and properties.
    fn handle_duplicate_actor(&mut self, original_label: &str) {
        self.snapshot(Command::DuplicateActor(original_label.to_string()));

        // Generate new label before borrowing self mutably
        let new_label = self.unique_label(original_label);

        let Some(ref mut stmts) = self.document.raw_statements else {
            self.preview.status = "Failed to duplicate — no AST available".to_string();
            return;
        };

        // Find the original actor declaration
        let original_stmt = crate::source_edit::find_actor_decl(stmts, original_label).cloned();

        let Some(mut new_stmt) = original_stmt else {
            self.preview.status = format!("Failed to duplicate — actor '{}' not found", original_label);
            return;
        };

        // Update label in the new statement
        match &mut new_stmt {
            animatix::ast::Stmt::ActorDecl { label, .. } => *label = new_label.clone(),
            _ => {
                self.preview.status = "Failed to duplicate — unsupported actor type".to_string();
                return;
            }
        }

        // Find position to insert (after the original actor)
        if let Some(pos) = stmts.iter().position(|s| {
            match s {
                animatix::ast::Stmt::ActorDecl { label, .. } if label == original_label => true,
                _ => false,
            }
        }) {
            stmts.insert(pos + 1, new_stmt);
        } else {
            stmts.push(new_stmt);
        }

        // Update source
        let new_source = animatix::to_source::stmts_to_source(stmts);
        self.document.source_text = new_source.clone();
        self.editor.replace_text(new_source);
        self.document.is_dirty = true;
        self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
        self.pending_rebuild_at = Some(std::time::Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));

        // Select new actor and start move drag
        self.selected_actors.clear();
        self.selected_actors.insert(new_label.clone());
        self.preview_dirty = true;
        self.preview.status = format!("Duplicated '{}' → '{}'", original_label, new_label);

        // Start move drag for the new actor at the original position
        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
        if let Some(timeline) = self.document.timeline.as_ref() {
            if let Some(track) = timeline.get_track(original_label) {
                let position = track.position.as_ref().map(|p| p.evaluate(time_ms)).unwrap_or([0.0, 0.0]);
                self.drag_state = DragState::Move {
                    primary: new_label.clone(),
                    actors: vec![(new_label, position)],
                    start_scene: kurbo::Point::new(position[0] as f64, position[1] as f64),
                };
            }
        }
    }

    /// Delete all selected actors from the source AST.
    fn handle_delete_selected_actors(&mut self) {
        self.snapshot(Command::DeleteSelectedActors);

        let Some(ref mut stmts) = self.document.raw_statements else {
            self.preview.status = "Failed to delete — no AST available".to_string();
            return;
        };

        let to_delete: Vec<String> = self.selected_actors.iter().cloned().collect();
        if to_delete.is_empty() {
            return;
        }

        let mut deleted = Vec::new();
        for label in &to_delete {
            // Find and remove the actor declaration
            let pos = stmts.iter().position(|s| {
                match s {
                    animatix::ast::Stmt::ActorDecl { label: l, .. } if l == label => true,
                    _ => false,
                }
            });
            if let Some(pos) = pos {
                stmts.remove(pos);
                deleted.push(label.clone());
            }
        }

        if deleted.is_empty() {
            self.preview.status = "No actors deleted".to_string();
            return;
        }

        // Update source
        let new_source = animatix::to_source::stmts_to_source(stmts);
        self.document.source_text = new_source.clone();
        self.editor.replace_text(new_source);
        self.document.is_dirty = true;
        self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
        self.pending_rebuild_at = Some(std::time::Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));

        // Clear selection
        self.selected_actors.clear();
        self.preview_dirty = true;
        self.preview.status = format!("Deleted {} actor(s)", deleted.len());
    }

    /// Update the transition on a scene's play statement.
    fn handle_set_transition(&mut self, from_scene: &str, transition: animatix::ast::Transition) {
        self.snapshot(Command::SetTransition { from_scene: from_scene.to_string(), transition: transition.clone() });

        let Some(ref mut stmts) = self.document.raw_statements else {
            self.preview.status = "Failed to set transition — no AST available".to_string();
            return;
        };

        let edit = crate::source_edit::SourceEdit::SetTransition {
            from_scene: from_scene.into(),
            transition: Some(transition.clone()),
        };

        if crate::source_edit::apply_edit(stmts, edit) {
            let new_source = animatix::to_source::stmts_to_source(stmts);
            self.document.source_text = new_source.clone();
            self.editor.replace_text(new_source);
            self.document.is_dirty = true;
            self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
            self.pending_rebuild_at = Some(std::time::Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
            self.preview.status = format!(
                "Set transition on '{}' → {}ms",
                from_scene,
                transition.duration_ms
            );
        } else {
            self.preview.status = format!("Failed to set transition on '{}'", from_scene);
        }
    }

    /// Update the play target for a scene.
    fn handle_set_play_target(&mut self, from_scene: &str, target: Option<String>) {
        self.snapshot(Command::SetPlayTarget { from_scene: from_scene.to_string(), target: target.clone() });

        let Some(ref mut stmts) = self.document.raw_statements else {
            self.preview.status = "Failed to set play target — no AST available".to_string();
            return;
        };

        let edit = crate::source_edit::SourceEdit::SetPlayTarget {
            scene: from_scene.into(),
            target: target.clone(),
        };

        if crate::source_edit::apply_edit(stmts, edit) {
            let new_source = animatix::to_source::stmts_to_source(stmts);
            self.document.source_text = new_source.clone();
            self.editor.replace_text(new_source);
            self.document.is_dirty = true;
            self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
            self.pending_rebuild_at = Some(std::time::Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
            if let Some(ref t) = target {
                self.preview.status = format!("Set play target: '{}' → '{}'", from_scene, t);
            } else {
                self.preview.status = format!("Removed play target from '{}'", from_scene);
            }
        } else {
            self.preview.status = format!("Failed to set play target on '{}'", from_scene);
        }
    }

    /// Handle a keyframe easing change request.
    fn handle_set_keyframe_easing(&mut self, actor: &str, property: &str, time_s: f64, easing: animatix::easing::Easing) {
        self.snapshot(Command::SetKeyframeEasing { actor: actor.to_string(), property: property.to_string(), time_s, easing });

        let Some(ref mut stmts) = self.document.raw_statements else {
            self.preview.status = "Failed to set keyframe easing — no AST available".to_string();
            return;
        };

        let edit = crate::source_edit::SourceEdit::SetKeyframeEasing {
            actor: actor.into(),
            property: property.into(),
            time_s,
            easing,
        };

        if crate::source_edit::apply_edit(stmts, edit) {
            let new_source = animatix::to_source::stmts_to_source(stmts);
            self.document.source_text = new_source.clone();
            self.editor.replace_text(new_source);
            self.document.is_dirty = true;
            self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
            self.pending_rebuild_at = Some(std::time::Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
            self.preview.status = format!("Set easing on '{}.{}' @ {:.2}s", actor, property, time_s);
        } else {
            self.preview.status = format!(
                "Failed to set easing on '{}.{}' @ {:.2}s — keyframe not found",
                actor, property, time_s
            );
        }
    }

    /// Handle a keyframe deletion request.
    fn handle_delete_keyframe(&mut self, actor: &str, property: &str, time_s: f64) {
        self.snapshot(Command::DeleteKeyframe { actor: actor.to_string(), property: property.to_string(), time_s });

        let Some(ref mut stmts) = self.document.raw_statements else {
            self.preview.status = "Failed to delete keyframe — no AST available".to_string();
            return;
        };

        let edit = crate::source_edit::SourceEdit::DeleteKeyframe {
            actor: actor.into(),
            property: property.into(),
            time_s,
        };

        if crate::source_edit::apply_edit(stmts, edit) {
            let new_source = animatix::to_source::stmts_to_source(stmts);
            self.document.source_text = new_source.clone();
            self.editor.replace_text(new_source);
            self.document.is_dirty = true;
            self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
            self.pending_rebuild_at = Some(std::time::Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
            self.preview.status = format!("Deleted keyframe '{}.{}' @ {:.2}s", actor, property, time_s);
        } else {
            self.preview.status = format!(
                "Failed to delete keyframe '{}.{}' @ {:.2}s — keyframe not found",
                actor, property, time_s
            );
        }
    }

    /// Reparent an actor under a new parent (or to top-level).
    fn handle_reparent_actor(&mut self, actor: &str, new_parent: Option<String>) {
        self.snapshot(Command::ReparentActor { actor: actor.to_string(), new_parent: new_parent.clone() });

        let Some(ref mut stmts) = self.document.raw_statements else {
            self.preview.status = "Failed to reparent — no AST available".to_string();
            return;
        };

        let edit = crate::source_edit::SourceEdit::Reparent {
            actor: actor.into(),
            new_parent: new_parent.clone(),
        };

        if crate::source_edit::apply_edit(stmts, edit) {
            let new_source = animatix::to_source::stmts_to_source(stmts);
            self.document.source_text = new_source.clone();
            self.editor.replace_text(new_source);
            self.document.is_dirty = true;
            self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
            self.pending_rebuild_at = Some(std::time::Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
            if let Some(ref parent) = new_parent {
                self.preview.status = format!("Reparented '{}' under '{}'", actor, parent);
            } else {
                self.preview.status = format!("Reparented '{}' to top level", actor);
            }
        } else {
            self.preview.status = format!("Failed to reparent '{}'", actor);
        }
    }

    /// Copy currently selected actor labels into the clipboard buffer.
    fn copy_selected_actors(&mut self) {
        let count = self.selected_actors.len();
        self.clipboard_actors = self.selected_actors.iter().cloned().collect();
        self.preview.status = format!("Copied {} actor(s)", count);
    }

    /// Paste actors from the clipboard into the current scene.
    ///
    /// For each clipboard actor:
    ///   - Clones the declaration with a unique label (`_copy` suffix + dedup)
    ///   - Clones all keyframe assignment statements referencing the original actor
    ///   - Renames references to the new label
    ///   - Shifts absolute keyframe times by `current_time_s`
    ///   - Inserts everything into the AST at the end
    fn paste_actors(&mut self) {
        self.snapshot(Command::PasteActors);

        let current_time_s = self.preview.current_time_s;
        let clipboard = self.clipboard_actors.clone();

        // Pre-generate all unique labels before mutating the AST.
        let label_map: Vec<(String, String)> = clipboard
            .iter()
            .map(|orig| (orig.clone(), self.paste_unique_label(orig)))
            .collect();

        let Some(ref mut stmts) = self.document.raw_statements else {
            self.preview.status = "Failed to paste — no AST available".to_string();
            return;
        };

        let mut pasted_labels = Vec::new();

        for (original_label, new_label) in &label_map {
            // Find the original actor declaration
            let original_stmt = crate::source_edit::find_actor_decl(stmts, original_label).cloned();
            let Some(mut new_stmt) = original_stmt else {
                continue;
            };

            // Update label in the new statement
            match &mut new_stmt {
                animatix::ast::Stmt::ActorDecl { label, .. } => *label = new_label.clone(),
                _ => continue,
            }

            // Insert the declaration at the end (or after the original)
            if let Some(pos) = stmts.iter().position(|s| {
                match s {
                    animatix::ast::Stmt::ActorDecl { label, .. } if label == original_label => true,
                    _ => false,
                }
            }) {
                stmts.insert(pos + 1, new_stmt);
            } else {
                stmts.push(new_stmt);
            }

            // Find and clone all keyframe assignments referencing the original actor
            let keyframe_stmts = find_keyframes_for_actor(stmts, original_label);
            for mut kf in keyframe_stmts {
                // Rename references within the keyframe
                crate::source_edit::rename_all_references(
                    std::slice::from_mut(&mut kf),
                    original_label,
                    new_label,
                );
                // Shift absolute keyframe times by current_time_s
                shift_keyframe_times(std::slice::from_mut(&mut kf), current_time_s);
                stmts.push(kf);
            }

            pasted_labels.push(new_label.clone());
        }

        if pasted_labels.is_empty() {
            self.preview.status = "Failed to paste — actor(s) not found in AST".to_string();
            return;
        }

        // Update source
        let new_source = animatix::to_source::stmts_to_source(stmts);
        self.document.source_text = new_source.clone();
        self.editor.replace_text(new_source);
        self.document.is_dirty = true;
        self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
        self.pending_rebuild_at = Some(std::time::Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));

        // Select the pasted actors
        self.selected_actors.clear();
        for label in &pasted_labels {
            self.selected_actors.insert(label.clone());
        }
        self.preview_dirty = true;
        self.preview.status = format!("Pasted {} actor(s)", pasted_labels.len());
    }

    /// Generate a unique label for pasted actors using `_copy` suffix.
    fn paste_unique_label(&self, base: &str) -> String {
        let candidate = format!("{}_copy", base);
        if !self.has_actor_label(&candidate) {
            return candidate;
        }
        for i in 1.. {
            let candidate = format!("{}_{}", base, i);
            if !self.has_actor_label(&candidate) {
                return candidate;
            }
        }
        format!("{}_{}", base, 999)
    }

    /// Check if an actor label already exists in the timeline (or in clipboard).
    fn has_actor_label(&self, label: &str) -> bool {
        if self
            .document
            .timeline
            .as_ref()
            .map_or(false, |t| t.has_actor(label))
        {
            return true;
        }
        if self.clipboard_actors.contains(&label.to_string()) {
            return true;
        }
        if let Some(ref stmts) = self.document.raw_statements {
            if crate::source_edit::find_actor_decl(stmts, label).is_some() {
                return true;
            }
        }
        false
    }

    /// Generate a unique label for a new actor of the given type.
    fn unique_label(&self, ty: &str) -> String {
        let base = ty.to_lowercase();
        let existing: std::collections::HashSet<String> = self
            .document
            .timeline
            .as_ref()
            .map(|t| t.tracks.keys().cloned().collect())
            .unwrap_or_default();
        for i in 1.. {
            let candidate = format!("{}{}", base, i);
            if !existing.contains(&candidate) {
                return candidate;
            }
        }
        format!("{}{}", base, existing.len() + 1)
    }
}

// ---------------------------------------------------------------------------
// Helpers for copy/paste
// ---------------------------------------------------------------------------

/// Find all top-level keyframe or relative-keyframe statements whose body
/// contains an assignment targeting the given actor label.
fn find_keyframes_for_actor(stmts: &[animatix::ast::Stmt], actor: &str) -> Vec<animatix::ast::Stmt> {
    let mut result = Vec::new();
    for stmt in stmts {
        match stmt {
            animatix::ast::Stmt::Keyframe { .. } | animatix::ast::Stmt::RelativeKeyframe { .. } => {
                if keyframe_references_actor(stmt, actor) {
                    result.push(stmt.clone());
                }
            }
            _ => {}
        }
    }
    result
}

/// Check if a keyframe statement (or any nested child) references the given actor.
fn keyframe_references_actor(stmt: &animatix::ast::Stmt, actor: &str) -> bool {
    match stmt {
        animatix::ast::Stmt::Assignment { target, .. } => {
            target.iter().any(|t| t == actor)
        }
        animatix::ast::Stmt::Keyframe { body, .. }
        | animatix::ast::Stmt::RelativeKeyframe { body, .. }
        | animatix::ast::Stmt::Sequence { body, .. }
        | animatix::ast::Stmt::Stagger { body, .. }
        | animatix::ast::Stmt::Always { body, .. }
        | animatix::ast::Stmt::ComponentDef(animatix::ast::ComponentDef { body, .. }, _)
        | animatix::ast::Stmt::ComponentAction { body, .. } => {
            body.iter().any(|child| keyframe_references_actor(child, actor))
        }
        animatix::ast::Stmt::Conditional { then_branch, else_branch, .. } => {
            then_branch.iter().any(|child| keyframe_references_actor(child, actor))
                || else_branch.as_ref().map_or(false, |eb| eb.iter().any(|child| keyframe_references_actor(child, actor)))
        }
        animatix::ast::Stmt::ForLoop { body, .. } => {
            body.iter().any(|child| keyframe_references_actor(child, actor))
        }
        _ => false,
    }
}

/// Shift absolute keyframe times by `offset_s` seconds. Relative keyframes
/// and non-keyframe statements are left untouched.
fn shift_keyframe_times(stmts: &mut [animatix::ast::Stmt], offset_s: f64) {
    if offset_s.abs() < 0.001 {
        return;
    }
    for stmt in stmts.iter_mut() {
        match stmt {
            animatix::ast::Stmt::Keyframe { time, .. } => {
                let t = match time {
                    animatix::ast::Time::Seconds(s) => *s,
                    animatix::ast::Time::Milliseconds(ms) => *ms as f64 / 1000.0,
                };
                let new_t = t + offset_s;
                *time = if new_t.fract() == 0.0 && new_t == new_t.trunc() {
                    animatix::ast::Time::Seconds(new_t)
                } else {
                    animatix::ast::Time::Seconds(new_t)
                };
            }
            animatix::ast::Stmt::RelativeKeyframe { .. } => {
                // Relative keyframes keep their relative offset
            }
            animatix::ast::Stmt::Sequence { body, .. }
            | animatix::ast::Stmt::Stagger { body, .. }
            | animatix::ast::Stmt::Always { body, .. }
            | animatix::ast::Stmt::ComponentDef(animatix::ast::ComponentDef { body, .. }, _)
            | animatix::ast::Stmt::ComponentAction { body, .. } => {
                shift_keyframe_times(body, offset_s);
            }
            animatix::ast::Stmt::Conditional { then_branch, else_branch, .. } => {
                shift_keyframe_times(then_branch, offset_s);
                if let Some(eb) = else_branch {
                    shift_keyframe_times(eb, offset_s);
                }
            }
            animatix::ast::Stmt::ForLoop { body, .. } => {
                shift_keyframe_times(body, offset_s);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
