#![allow(dead_code)]

mod actions;
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
use crate::app::panels::{UiActions, WorkspaceViewer};
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

struct PreviewPaneState {
    current_time_s: f64,
    duration_s: f64,
    is_playing: bool,
    status: String,
    error: Option<String>,
    dimensions: SceneDimensions,
    /// When set, the scene list panel should open the transition editor for this scene.
    open_transition_editor: Option<String>,
    /// Preview canvas zoom level (1.0 = 100%).
    preview_zoom: f32,
    /// Preview canvas pan offset in scene coordinates (scene point centered in preview).
    preview_pan: Vec2,
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
            open_transition_editor: None,
            preview_zoom: 1.0,
            preview_pan: Vec2::new(dimensions.width as f32 / 2.0, dimensions.height as f32 / 2.0),
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

        self.current_time_s += delta.as_secs_f64();
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
    preview_dirty: bool,
    pending_rebuild_at: Option<Instant>,
    last_frame_at: Instant,
    persistence_path: PathBuf,
    hot_reloader: Option<HotReloader>,
    last_reload_time: Option<Instant>,
    selected_actors: HashSet<String>,
    /// Per-actor hit regions from the last render (for click-to-select).
    hit_regions: Vec<(String, kurbo::Rect)>,
    /// Current drag interaction state on the preview canvas.
    drag_state: DragState,
    /// Selection system state (hover, cycling, context menu).
    selection: crate::app::preview::selection::SelectionState,
    /// Undo stack for property edits (source text snapshots).
    undo_stack: Vec<String>,
    /// Redo stack for property edits (source text snapshots).
    redo_stack: Vec<String>,
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
            preview_dirty: true,
            pending_rebuild_at: None,
            last_frame_at: Instant::now(),
            persistence_path,
            hot_reloader,
            last_reload_time: None,
            selected_actors: HashSet::new(),
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
        let mut actions = UiActions::default();

        // Global keyboard shortcuts for timeline toggles
        ui.input(|i| {
            if i.key_pressed(egui::Key::S) && !i.modifiers.command {
                actions.toggle_editor_sync = true;
            }
            if i.key_pressed(egui::Key::K) && !i.modifiers.command {
                actions.toggle_keyframe_mode = true;
            }
        });

        // Compact toolbar
        egui::Panel::top("toolbar")
            .resizable(false)
            .show_inside(ui, |ui| self.toolbar_ui(ui, &mut actions));

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
                    self.document.scene_dimensions,
                    &timeline_markers,
                    actor_count,
                    keyframe_count,
                    self.document.is_dirty,
                    has_error,
                    &diagnostics,
                    &mut actions,
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
                self.workspace_ui(ui, preview_texture_id, &mut actions);
            });

        // Update cursor time from editor position (bi-directional sync)
        self.cursor_time_s = self
            .editor
            .cursor_line
            .and_then(|line| self.document.timeline_index.time_s_for_line(line));

        self.handle_actions(actions);

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
        actions: &mut UiActions,
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
            diagnostics: &diagnostics,
            preview_texture_id,
            actions,
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

    fn handle_actions(&mut self, actions: UiActions) {
        if let Some(path) = actions.open_file {
            self.open_document(path);
        }
        if let Some(path) = actions.toggle_expand_dir {
            if self.expanded_dirs.contains(&path) {
                self.expanded_dirs.remove(&path);
            } else {
                self.expanded_dirs.insert(path.clone());
            }
            self.file_tree = build_file_tree(&self.workspace_root, &self.document.file_path, &self.expanded_dirs);
        }
        if actions.show_inspector {
            self.open_workspace_tab(WorkspaceTab::Inspector);
        }
        if actions.open_export_dialog {
            self.export_dialog_open = true;
            if self.export_state.output_path.is_empty() {
                self.update_default_export_filename();
            }
        }
        if actions.toggle_diagnostics_panel {
            self.diagnostics_panel_visible = !self.diagnostics_panel_visible;
        }
        if actions.save {
            let _ = self.save();
        }
        if actions.reload {
            let _ = self.reload();
        }
        if actions.rebuild {
            let _ = self.rebuild();
        }
        if let Some(next_time) = actions.scrub_to {
            self.preview.current_time_s = next_time;
            self.preview.clamp_time();
            self.preview.is_playing = false;
            self.preview_dirty = true;
            self.sync_active_scene_from_time();
            if self.editor_sync_enabled {
                if let Some(line) = self.document.find_keyframe_line_at(next_time) {
                    self.editor.scroll_to_line(line);
                    self.editor.set_highlighted_line(Some(line));
                }
            }
        }
        if actions.toggle_playback {
            self.preview.toggle_playback();
            self.preview_dirty = true;
        }
        if actions.toggle_editor_sync {
            self.editor_sync_enabled = !self.editor_sync_enabled;
            self.preview.status = if self.editor_sync_enabled {
                "Editor sync ON".to_string()
            } else {
                "Editor sync OFF".to_string()
            };
        }
        if actions.toggle_keyframe_mode {
            self.keyframe_mode = !self.keyframe_mode;
            self.preview.status = if self.keyframe_mode {
                "Keyframe mode ON — edits create timestamps".to_string()
            } else {
                "Keyframe mode OFF — edits overwrite defaults".to_string()
            };
        }
        if actions.editor_changed {
            self.document
                .set_source_text(self.editor.text().to_string());
            self.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
            self.preview.status = "Editing source • rebuild scheduled".to_string();
            // Clear any stale error from a previous failed rebuild so the user
            // doesn't see an outdated error banner while typing.
            self.preview.error = None;
            // Also clear stale document diagnostics so the preview banner doesn't
            // show an outdated parse error during the debounce window.
            self.document.diagnostics.clear();
        }
        if actions.request_repaint {
            self.preview_dirty = true;
        }
        if actions.prev_keyframe {
            let keyframes = timeline_keyframe_times_s(
                if self.document.composition.is_some() {
                    None
                } else {
                    self.document.active_timeline()
                },
                self.document.composition.as_ref(),
                self.document.active_scene.as_deref(),
            );
            self.preview.go_to_previous_keyframe(&keyframes);
            self.preview.status = format!(
                "Previous keyframe • t = {:.2}s / {:.2}s",
                self.preview.current_time_s, self.preview.duration_s
            );
            self.preview_dirty = true;
            if self.editor_sync_enabled {
                if let Some(line) = self.document.find_keyframe_line_at(self.preview.current_time_s)
                {
                    self.editor.scroll_to_line(line);
                    self.editor.set_highlighted_line(Some(line));
                }
            }
        }
        if actions.next_keyframe {
            let keyframes = timeline_keyframe_times_s(
                if self.document.composition.is_some() {
                    None
                } else {
                    self.document.active_timeline()
                },
                self.document.composition.as_ref(),
                self.document.active_scene.as_deref(),
            );
            self.preview.go_to_next_keyframe(&keyframes);
            self.preview.status = format!(
                "Next keyframe • t = {:.2}s / {:.2}s",
                self.preview.current_time_s, self.preview.duration_s
            );
            self.preview_dirty = true;
            if self.editor_sync_enabled {
                if let Some(line) = self.document.find_keyframe_line_at(self.preview.current_time_s)
                {
                    self.editor.scroll_to_line(line);
                    self.editor.set_highlighted_line(Some(line));
                }
            }
        }
        if actions.prev_scene || actions.next_scene {
            if let Some(composition) = self.document.composition.as_ref() {
                let current_idx = self
                    .document
                    .active_scene
                    .as_deref()
                    .and_then(|name| composition.declaration_order.iter().position(|n| n == name))
                    .unwrap_or(0);
                let target_idx = if actions.prev_scene {
                    current_idx.saturating_sub(1)
                } else {
                    (current_idx + 1).min(composition.declaration_order.len().saturating_sub(1))
                };
                if let Some(target_name) = composition.declaration_order.get(target_idx) {
                    self.document.active_scene = Some(target_name.clone());
                    if let Some(start) = composition.scene_start_times.get(target_name) {
                        self.preview.current_time_s = *start;
                        self.preview.clamp_time();
                        self.preview.is_playing = false;
                        self.preview_dirty = true;
                        self.preview.status = format!(
                            "Scene {} • t = {:.2}s / {:.2}s",
                            target_name, self.preview.current_time_s, self.preview.duration_s
                        );
                    }
                }
            }
        }
        if let Some(scene) = actions.select_scene {
            if let Some(composition) = self.document.composition.as_ref() {
                if composition.scenes.contains_key(&scene) {
                    self.document.active_scene = Some(scene.clone());
                    if let Some(start) = composition.scene_start_times.get(&scene) {
                        let mut target_time = *start;
                        // Jump past any incoming transition to land in the stable part of the scene
                        for edge in composition.edges.values() {
                            if edge.to_scene == scene {
                                target_time += edge.transition.duration_ms as f64 / 1000.0;
                                break;
                            }
                        }
                        self.preview.current_time_s = target_time;
                        self.preview.clamp_time();
                        self.preview.is_playing = false;
                        self.preview_dirty = true;
                        self.preview.status = format!(
                            "Scene {} • t = {:.2}s / {:.2}s",
                            scene, self.preview.current_time_s, self.preview.duration_s
                        );
                    }
                }
            }
        }
        if let Some(scene) = actions.delete_scene {
            if let Some(ref mut stmts) = self.document.raw_statements {
                let edit = crate::source_edit::SourceEdit::DeleteScene {
                    name: scene.clone(),
                };
                if crate::source_edit::apply_edit(stmts, edit) {
                    let new_source = animatix::to_source::stmts_to_source(stmts);
                    self.document.source_text = new_source.clone();
                    self.editor.replace_text(new_source);
                    self.document.is_dirty = true;
                    self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
                    self.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
                    self.preview.status = format!("Deleted scene {}", scene);
                    // Clear active scene if it was the deleted one
                    if self.document.active_scene.as_ref() == Some(&scene) {
                        self.document.active_scene = None;
                    }
                }
            }
        }
        if actions.add_scene {
            let existing: std::collections::HashSet<String> =
                self.document.scene_names().into_iter().collect();
            if let Some(ref mut stmts) = self.document.raw_statements {
                let mut i = 1;
                let new_name = loop {
                    let candidate = format!("Scene{}", i);
                    if !existing.contains(&candidate) {
                        break candidate;
                    }
                    i += 1;
                };

                let edit = crate::source_edit::SourceEdit::AddScene {
                    name: new_name.clone(),
                };
                if crate::source_edit::apply_edit(stmts, edit) {
                    let new_source = animatix::to_source::stmts_to_source(stmts);
                    self.document.source_text = new_source.clone();
                    self.editor.replace_text(new_source);
                    self.document.is_dirty = true;
                    self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
                    self.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
                    self.preview.status = format!("Added scene {}", new_name);
                }
            }
        }
        if let Some((old_name, new_name)) = actions.rename_scene {
            if old_name != new_name && !new_name.is_empty() {
                if let Some(ref mut stmts) = self.document.raw_statements {
                    let edit = crate::source_edit::SourceEdit::RenameScene {
                        old_name,
                        new_name: new_name.clone(),
                    };
                    if crate::source_edit::apply_edit(stmts, edit) {
                        let new_source = animatix::to_source::stmts_to_source(stmts);
                        self.document.source_text = new_source.clone();
                        self.editor.replace_text(new_source);
                        self.document.is_dirty = true;
                        self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
                        self.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
                        self.preview.status = format!("Renamed scene to {}", new_name);
                    }
                }
            }
        }
        if let Some(new_order) = actions.reorder_scenes {
            if let Some(ref mut stmts) = self.document.raw_statements {
                let edit = crate::source_edit::SourceEdit::ReorderScenes {
                    new_order: new_order.clone(),
                };
                if crate::source_edit::apply_edit(stmts, edit) {
                    let new_source = animatix::to_source::stmts_to_source(stmts);
                    self.document.source_text = new_source.clone();
                    self.editor.replace_text(new_source);
                    self.document.is_dirty = true;
                    self.document.source_index = Some(animatix::source_index::SourceIndex::build(stmts));
                    self.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
                    self.preview.status = "Reordered scenes".to_string();
                }
            }
        }
        if let Some((ty, label, position)) = actions.create_actor {
            self.handle_create_actor(&ty, &label, position);
        }
        if let Some(original_label) = actions.duplicate_actor {
            self.handle_duplicate_actor(&original_label);
        }
        if let Some((from_scene, transition)) = actions.set_transition {
            self.handle_set_transition(&from_scene, transition);
        }
        if let Some((from_scene, target)) = actions.set_play_target {
            self.handle_set_play_target(&from_scene, target);
        }
        if let Some((old_label, new_label)) = actions.rename_actor {
            self.handle_rename_actor(&old_label, &new_label);
        }
        if let Some((actor, property, time_s, easing)) = actions.set_keyframe_easing {
            self.handle_set_keyframe_easing(&actor, &property, time_s, easing);
        }
        if actions.inspector_input_drag_started {
            self.inspector_input_drag_active = true;
        }
        if let Some(scene) = actions.open_transition_editor {
            self.preview.open_transition_editor = Some(scene);
        }
        for edit in actions.property_edits {
            self.handle_property_edit(edit);
        }
        // End drag AFTER processing edits — the workspace signals drag_ended
        // instead of resetting drag_state directly, so handle_property_edit
        // sees the drag as still active and coalesces the final frame's edits
        // into the same undo entry.
        if actions.drag_ended {
            self.drag_state = DragState::None;
            self.drag_snapshot_taken = false;
        }
        if actions.inspector_input_drag_ended {
            self.inspector_input_drag_active = false;
            self.drag_snapshot_taken = false;
        }
        if actions.undo {
            self.undo();
        }
        if actions.redo {
            self.redo();
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
        self.document.save_to_disk()?;
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
    fn snapshot(&mut self) {
        self.undo_stack.push(self.document.source_text.clone());
        self.redo_stack.clear();
        // Limit undo history
        if self.undo_stack.len() > self.undo_limit {
            self.undo_stack.remove(0);
        }
    }

    /// Undo the last property edit.
    fn undo(&mut self) {
        if let Some(previous) = self.undo_stack.pop() {
            self.redo_stack.push(self.document.source_text.clone());
            self.document.source_text = previous.clone();
            self.editor.replace_text(previous);
            self.document.is_dirty = true;
            self.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
            self.preview.status = "Undo".to_string();
        }
    }

    /// Redo the last undone property edit.
    fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.document.source_text.clone());
            self.document.source_text = next.clone();
            self.editor.replace_text(next);
            self.document.is_dirty = true;
            self.pending_rebuild_at = Some(Instant::now() + Duration::from_millis(self.rebuild_debounce_ms));
            self.preview.status = "Redo".to_string();
        }
    }

    fn open_workspace_tab(&mut self, target: WorkspaceTab) {
        let target = match target {
            WorkspaceTab::Sidebar => WorkspaceTab::Sidebar,
            WorkspaceTab::Editor => WorkspaceTab::Editor,
            WorkspaceTab::Preview => WorkspaceTab::Preview,
            WorkspaceTab::Inspector => WorkspaceTab::Inspector,
        };
        let _ = self.tree.make_active(|_, tile| matches!(tile, Tile::Pane(tab) if *tab == target));
    }

    /// Duplicate an actor, preserving its type and properties.
    fn handle_duplicate_actor(&mut self, original_label: &str) {
        self.snapshot();

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
            animatix::ast::Stmt::Text { label, .. } => *label = Some(new_label.clone()),
            animatix::ast::Stmt::Math { label, .. } => *label = Some(new_label.clone()),
            animatix::ast::Stmt::Code { label, .. } => *label = Some(new_label.clone()),
            animatix::ast::Stmt::Svg { label, .. } => *label = Some(new_label.clone()),
            animatix::ast::Stmt::Image { label, .. } => *label = Some(new_label.clone()),
            _ => {
                self.preview.status = "Failed to duplicate — unsupported actor type".to_string();
                return;
            }
        }

        // Find position to insert (after the original actor)
        if let Some(pos) = stmts.iter().position(|s| {
            match s {
                animatix::ast::Stmt::ActorDecl { label, .. } if label == original_label => true,
                animatix::ast::Stmt::Text { label: Some(l), .. } if l == original_label => true,
                animatix::ast::Stmt::Math { label: Some(l), .. } if l == original_label => true,
                animatix::ast::Stmt::Code { label: Some(l), .. } if l == original_label => true,
                animatix::ast::Stmt::Svg { label: Some(l), .. } if l == original_label => true,
                animatix::ast::Stmt::Image { label: Some(l), .. } if l == original_label => true,
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
        self.snapshot();

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
                    animatix::ast::Stmt::Text { label: Some(l), .. } if l == label => true,
                    animatix::ast::Stmt::Math { label: Some(l), .. } if l == label => true,
                    animatix::ast::Stmt::Code { label: Some(l), .. } if l == label => true,
                    animatix::ast::Stmt::Svg { label: Some(l), .. } if l == label => true,
                    animatix::ast::Stmt::Image { label: Some(l), .. } if l == label => true,
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
        self.snapshot();

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
        self.snapshot();

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
        self.snapshot();

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

#[cfg(test)]
mod tests;
