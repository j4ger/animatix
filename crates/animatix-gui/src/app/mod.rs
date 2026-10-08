mod actions;
pub(crate) mod audio;
pub(crate) mod command_handlers;
pub(crate) mod commands;
pub(crate) mod components;
pub mod design_tokens;
pub(crate) mod document;
mod document_controller;
mod file_tree;
pub(crate) mod handlers;
pub(crate) mod icons;
pub(crate) mod insertion;
pub(crate) mod interaction;
pub(crate) mod panels;
mod perf_log;
mod persistence;
pub(crate) mod preview;
pub(crate) mod review;
mod runtime;
pub(crate) mod shell;
pub(crate) mod stores;
mod utils;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use animatix::timeline::SceneDimensions;
use animatix_syntax::diagnostics::{
    Diagnostic, DiagnosticCode, DiagnosticPhase, diagnostics_phase_summary,
};
use directories::ProjectDirs;
use egui::{Color32, Stroke, Vec2};
use file_tree::{build_file_tree, workspace_root_for};
use persistence::{
    SettingsPersistence, WorkspacePersistence, default_tree, load_workspace_persistence,
    persistence_path,
};
#[cfg(test)]
use preview::fit_preview;
use serde::{Deserialize, Serialize};

use crate::app::commands::{
    ActionQueue, CommandQueue, CommandSender, DocumentCommand, Effect, UndoLabel, ViewCommand,
};
use crate::app::components::button::Button;
use crate::app::components::toast::Toast;
use crate::app::components::{Alert, AlertLevel, dialog, text_tooltip};
use crate::app::design_tokens::spatial::welcome::TOP_OFFSET_FRAC as WELCOME_TOP_OFFSET_FRAC;
use crate::app::design_tokens::spatial::{
    RADIUS_L, RADIUS_S, ROW_L, SPACE_2, SPACE_3, SPACE_4, SPACE_5, STROKE_WIDTH, spatial,
};
use crate::app::design_tokens::typography::TextRole;
use crate::app::document::plugins::DocumentPluginManager;
use crate::app::document::rebuild::RebuildWorker;
use crate::app::handlers::file;
use crate::app::interaction::keyboard::ShortcutRegistry;
use crate::app::shell::insertion_palette::InsertionPalette;
use crate::app::stores::*;
use crate::app::utils::*;
use crate::document::{DocumentSession, default_file_path};
use crate::editor::EditorBuffer;
use crate::hot_reload::{HotReloader, ReloadStatus};
use crate::preview_surface::PreviewSurface;

const INITIAL_WINDOW_SIZE: (f64, f64) = (1440.0, 960.0);
const DEFAULT_PREVIEW_SIZE: SceneDimensions = SceneDimensions {
    width: 1920,
    height: 1080,
};
const MAX_TREE_DEPTH: usize = 4;
const MAX_TREE_ENTRIES: usize = 200;

pub use runtime::run_gui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceTab {
    Sidebar,
    /// Source code editor. Shares the right "detail" tab group with the
    /// Inspector; named `Editor` in layouts persisted before the rename.
    #[serde(alias = "Editor")]
    Code,
    Preview,
    Inspector,
    Timeline,
    /// Interactive F-curve editor. Shares the bottom tab group with the
    /// Timeline; absent from layouts persisted before it was introduced.
    Curves,
}

/// Named workspace layouts (design doc §9.2).
///
/// A preset adjusts region proportions and which detail tab is active on the
/// existing tree, so it never discards a user's custom arrangement; only
/// "Reset layout" rebuilds from scratch. `Focus` additionally hides the
/// surrounding regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutPreset {
    Animate,
    Code,
    Inspect,
    Focus,
}

impl LayoutPreset {
    pub const ALL: [Self; 4] = [Self::Animate, Self::Code, Self::Inspect, Self::Focus];

    pub fn label(self) -> &'static str {
        match self {
            Self::Animate => "Animate",
            Self::Code => "Code",
            Self::Inspect => "Inspect",
            Self::Focus => "Focus",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FileTreeEntry {
    path: PathBuf,
    name: String,
    depth: usize,
    is_dir: bool,
}

/// Playback controller: time, duration, play/pause, speed, loop region, ping-pong.
#[derive(Debug, Clone)]
pub(crate) struct PlaybackController {
    current_time_s: f64,
    pub duration_s: f64,
    pub is_playing: bool,
    pub playback_speed: f32,
    pub loop_start_s: Option<f64>,
    pub loop_end_s: Option<f64>,
    pub ping_pong: bool,
    pub ping_pong_direction: i32,
    pub fps: f32,
}

impl PlaybackController {
    /// Read the current playback time in seconds.
    pub(crate) fn current_time_s(&self) -> f64 {
        self.current_time_s
    }

    /// Jump to an absolute time (clamped to [0, duration]).
    pub(crate) fn scrub_to(&mut self, time_s: f64) {
        self.current_time_s = time_s.clamp(0.0, self.duration_s.max(0.1));
        self.is_playing = false;
    }

    /// Restore playback state captured by an undo/redo snapshot.
    pub(crate) fn restore_snapshot_state(
        &mut self,
        time_s: f64,
        loop_start_s: Option<f64>,
        loop_end_s: Option<f64>,
    ) {
        self.current_time_s = time_s.clamp(0.0, self.duration_s.max(0.1));
        self.loop_start_s = loop_start_s;
        self.loop_end_s = loop_end_s;
        self.is_playing = false;
    }

    fn clamp_time(&mut self) {
        let max_duration = self.duration_s.max(0.1);
        self.current_time_s = self.current_time_s.clamp(0.0, max_duration);
    }

    /// Advance by one frame at the given fps. Stops playback.
    pub(crate) fn step_frame(&mut self, delta_s: f64) {
        self.current_time_s = (self.current_time_s + delta_s).clamp(0.0, self.duration_s.max(0.0));
        self.is_playing = false;
    }

    /// Advance by one frame at the given fps. Stops playback.
    pub(crate) fn frame_step_forward(&mut self, fps: f32) {
        let step = 1.0 / fps.max(1.0) as f64;
        self.current_time_s = (self.current_time_s + step).min(self.duration_s);
        self.is_playing = false;
    }

    /// Rewind by one frame at the given fps. Stops playback.
    pub(crate) fn frame_step_backward(&mut self, fps: f32) {
        let step = 1.0 / fps.max(1.0) as f64;
        self.current_time_s = (self.current_time_s - step).max(0.0);
        self.is_playing = false;
    }

    /// Return the current time formatted as HH:MM:SS:FF at the stored fps.
    pub(crate) fn timecode_string(&self) -> String {
        let total_seconds = self.current_time_s.max(0.0);
        let hours = (total_seconds / 3600.0).floor() as u32;
        let minutes = ((total_seconds % 3600.0) / 60.0).floor() as u32;
        let seconds = (total_seconds % 60.0).floor() as u32;
        let frame = ((total_seconds % 1.0) * self.fps as f64).floor() as u32;
        format!("{:02}:{:02}:{:02}:{:02}", hours, minutes, seconds, frame)
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

    fn tick(&mut self, delta: std::time::Duration) {
        if !self.is_playing {
            return;
        }

        self.current_time_s +=
            delta.as_secs_f64() * self.playback_speed as f64 * self.ping_pong_direction as f64;

        // Loop region: if A and B are set, handle boundaries.
        if let (Some(start), Some(end)) = (self.loop_start_s, self.loop_end_s) {
            if end > start {
                if self.ping_pong {
                    if self.current_time_s >= end && self.ping_pong_direction > 0 {
                        self.ping_pong_direction = -1;
                        self.current_time_s = end;
                        return;
                    }
                    if self.current_time_s <= start && self.ping_pong_direction < 0 {
                        self.ping_pong_direction = 1;
                        self.current_time_s = start;
                        return;
                    }
                } else if self.current_time_s >= end {
                    self.current_time_s = start;
                    // Looping takes priority over end-of-timeline stop.
                    return;
                }
            }
        }

        // Natural boundaries (0 and duration_s)
        if self.current_time_s >= self.duration_s {
            if self.ping_pong {
                self.ping_pong_direction = -1;
                self.current_time_s = self.duration_s;
            } else {
                self.current_time_s = self.duration_s;
                self.is_playing = false;
            }
        } else if self.current_time_s <= 0.0 {
            if self.ping_pong {
                self.ping_pong_direction = 1;
                self.current_time_s = 0.0;
            } else {
                self.current_time_s = 0.0;
                self.is_playing = false;
            }
        }
    }
}

/// Viewport state: zoom and pan for the preview canvas.
#[derive(Debug, Clone)]
pub(crate) struct ViewportState {
    pub preview_zoom: f32,
    pub preview_pan: Vec2,
}

/// Guide lines drawn on the preview canvas.
#[derive(Debug, Clone)]
pub(crate) struct GuideState {
    pub horizontal_guides: Vec<f32>,
    pub vertical_guides: Vec<f32>,
}

/// Smart snap state for drag interactions.
#[derive(Debug, Clone)]
pub(crate) struct SnapState {
    pub snap_lines_h: Vec<f32>,
    pub snap_lines_v: Vec<f32>,
    pub snap_line_color: Option<Color32>,
    pub snap_enabled: bool,
    pub snap_threshold: f32,
    pub snap_hud_label: Option<String>,
}

/// State for in-place text editing on the preview canvas.
pub(crate) struct InlineTextEditState {
    pub actor: String,
    pub property: String,
    pub current_value: String,
    pub screen_pos: egui::Pos2,
    pub screen_size: egui::Vec2,
}

/// Severity level for preview status messages.
#[derive(Default, Clone, Copy, PartialEq)]
pub enum StatusSeverity {
    #[default]
    Info,
    Error,
}

pub(crate) struct PreviewPaneState {
    pub playback: PlaybackController,
    pub viewport: ViewportState,
    pub guides: GuideState,
    pub snap: SnapState,
    pub status: String,
    pub status_severity: StatusSeverity,
    pub error: Option<String>,
    pub dimensions: SceneDimensions,
    /// Time lens HUD state (Space-drag time scrubbing).
    pub time_lens: crate::app::preview::time_lens::TimeLens,
    /// Overlay toggle state.
    pub overlay: crate::app::preview::overlay::PreviewOverlay,
    /// Timeline horizontal zoom (1.0 = fit to width).
    pub timeline_zoom: f32,
    /// Timeline horizontal scroll offset in seconds (when zoomed).
    pub timeline_scroll_offset: f64,
    /// When true, the preview panel will recompute zoom to fit on next frame.
    pub fit_zoom_requested: bool,
    /// Keyframe times that were recently rewritten by `adjust_following_relative_keyframe`.
    /// Rendered with an amber flash in the timeline panel for ~300 ms.
    pub flashed_keyframe_times: Vec<(f64, std::time::Instant)>,
    /// In-place text editing state (activated by double-clicking text actors).
    pub inline_edit: Option<InlineTextEditState>,
}

impl PreviewPaneState {
    fn new(duration_s: f64, dimensions: SceneDimensions) -> Self {
        Self {
            playback: PlaybackController {
                current_time_s: 0.0,
                duration_s,
                is_playing: false,
                playback_speed: 1.0,
                loop_start_s: None,
                loop_end_s: None,
                ping_pong: false,
                ping_pong_direction: 1,
                fps: 60.0,
            },
            viewport: ViewportState {
                preview_zoom: 1.0,
                preview_pan: Vec2::new(
                    dimensions.width as f32 / 2.0,
                    dimensions.height as f32 / 2.0,
                ),
            },
            guides: GuideState {
                horizontal_guides: vec![],
                vertical_guides: vec![],
            },
            snap: SnapState {
                snap_lines_h: vec![],
                snap_lines_v: vec![],
                snap_line_color: None,
                snap_enabled: true,
                snap_threshold: 10.0,
                snap_hud_label: None,
            },
            status: "Loaded file".to_string(),
            status_severity: StatusSeverity::Info,
            error: None,
            dimensions,
            time_lens: crate::app::preview::time_lens::TimeLens::default(),
            overlay: crate::app::preview::overlay::PreviewOverlay::default(),
            timeline_zoom: 1.0,
            timeline_scroll_offset: 0.0,
            fit_zoom_requested: false,
            flashed_keyframe_times: Vec::new(),
            inline_edit: None,
        }
    }

    pub fn set_status_info(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_severity = StatusSeverity::Info;
    }

    pub fn set_status_error(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_severity = StatusSeverity::Error;
    }
}

struct GuiShell {
    document_store: DocumentStore,
    workspace_store: WorkspaceStore,
    preview_store: PreviewStore,
    ui_store: UiStore,
    export_store: ExportStore,
    rebuild_worker: RebuildWorker,
    plugin_manager: DocumentPluginManager,
    external_commands: CommandQueue,
    insertion_palette: InsertionPalette,
    shortcut_registry: ShortcutRegistry,
    pub(crate) window_size: [f32; 2],
    pub(crate) window_maximized: bool,
}

impl GuiShell {
    fn check_hot_reload(&mut self, app_time: Instant) {
        if let Some(ref mut reloader) = self.workspace_store.hot_reloader {
            match reloader.update(app_time) {
                ReloadStatus::ShouldReload { path: _ } => {
                    // LiveDocument: editor is the source of truth. If the editor
                    // has unsaved changes, route through the unsaved-changes dialog so
                    // the user can Save (which overwrites the on-disk edit) or Discard
                    // (which loads the external change). Guard with is_open to avoid
                    // re-prompting every frame while the watcher keeps firing.
                    if self.document_store.source.document.is_dirty {
                        if !self.ui_store.unsaved_changes.is_open {
                            self.ui_store.unsaved_changes.open(
                                "File changed on disk. Reload and discard your unsaved edits?",
                                DocumentCommand::Reload.into(),
                            );
                        }
                        return;
                    }
                    if let Err(err) = self.document_store.source.document.reload_from_disk() {
                        self.preview_store.preview.error = Some(err.to_string());
                        self.preview_store.preview.set_status_error("Hot reload failed");
                    } else {
                        self.document_store.source.invalidate_cache();
                        self.document_store.source.editor.set_document(
                            &self.document_store.source.document.file_path,
                            self.document_store.source.document.source_text.clone(),
                        );
                        self.workspace_store.last_reload_time = Some(app_time);
                        self.preview_store.preview.status = "File reloaded; rebuilding".to_string();
                        self.preview_store.preview.error = None;
                        self.preview_store.pending_rebuild_at = Some(
                            std::time::Instant::now()
                                + std::time::Duration::from_millis(
                                    self.ui_store.rebuild_debounce_ms,
                                ),
                        );
                    }
                },
                ReloadStatus::NoChange => {},
            }
        }
    }

    fn load(initial_path: PathBuf, show_welcome: bool) -> Self {
        let workspace_root = workspace_root_for(&initial_path);
        let persistence_path = persistence_path();
        let persistence = load_workspace_persistence(&persistence_path);
        let plugin_paths = persistence
            .as_ref()
            .and_then(|p| p.settings.as_ref())
            .map(|settings| settings.plugin_paths.clone())
            .unwrap_or_default();
        let plugin_manager = DocumentPluginManager::new_with_explicit_paths(
            initial_path.clone(),
            workspace_root.clone(),
            plugin_paths,
        );
        let (mut document, status, error, is_welcome) = if show_welcome {
            // No recent file persisted — show welcome screen
            let doc = DocumentSession::from_error(initial_path.clone());
            (doc, None, None, true)
        } else {
            let context = plugin_manager.context();
            let manifest = plugin_manager.manifest();
            match DocumentSession::load_with_extension(
                initial_path.clone(),
                context,
                manifest,
                Some(workspace_root.clone()),
            ) {
                Ok(document) => {
                    let error = document.last_rebuild_error.clone();
                    (document, None, error, false)
                },
                Err(error) => {
                    // Persisted file missing/deleted — fall back to welcome
                    let doc = DocumentSession::from_error(initial_path.clone());
                    (doc, None, Some(error.to_string()), true)
                },
            }
        };

        document.set_plugin_epoch(plugin_manager.epoch());

        let expanded_dirs = HashSet::from([workspace_root.clone()]);
        let file_tree = build_file_tree(&workspace_root, &document.file_path, &expanded_dirs);
        let tree = persistence.as_ref().map(|p| p.tree.clone()).unwrap_or_else(default_tree);
        let window_size =
            persistence.as_ref().and_then(|p| p.window_size).unwrap_or([1440.0, 960.0]);
        let window_maximized =
            persistence.as_ref().and_then(|p| p.window_maximized).unwrap_or(false);
        let (hot_reloader, hot_reload_error) = match HotReloader::new(&document.file_path) {
            Ok(reloader) => (Some(reloader), None),
            Err(err) => (None, Some(err)),
        };
        let duration_s = document.duration_s.max(0.1);
        let mut preview = PreviewPaneState::new(duration_s, document.scene_dimensions);
        if let Some(status) = status {
            preview.status = status;
        } else if has_source_load_failure(&document.diagnostics) {
            preview.set_status_error(format!(
                "Opened {} • parse/load error • {}",
                document.file_path.display(),
                diagnostics_phase_summary(&document.diagnostics)
            ));
        }
        preview.error = error.clone();

        let editor = EditorBuffer::new(&document.file_path, document.source_text.clone());

        let mut ui_store = UiStore::new(tree);
        ui_store.recent_files = crate::app::persistence::load_recent_files();
        ui_store.view.welcome_open = is_welcome;

        // Apply persisted settings
        if let Some(s) = persistence.as_ref().and_then(|p| p.settings.as_ref()) {
            ui_store.rebuild_debounce_ms = s.rebuild_debounce_ms;
            ui_store.scrub_step_s = s.scrub_step_s;
            ui_store.nudge_step_px = s.nudge_step_px;
            ui_store.nudge_step_shift_px = s.nudge_step_shift_px;
            ui_store.rotation_snap_degrees = s.rotation_snap_degrees;
            ui_store.snap_fps = s.snap_fps;
            ui_store.keyframe_merge_window_s = s.keyframe_merge_window_s;
            preview.overlay.grid_size = s.grid_size;
            ui_store.view.app_theme = match s.app_theme.as_str() {
                "light" => eparts::AppThemeChoice::Light,
                "dark" => eparts::AppThemeChoice::Dark,
                _ => eparts::AppThemeChoice::Auto,
            };
            ui_store.view.reduce_motion = s.reduce_motion;
            ui_store.view.density = match s.density.as_str() {
                "compact" => eparts::Density::Compact,
                _ => eparts::Density::Default,
            };
            ui_store.view.theme_dir = s.theme_dir.clone();
            ui_store.view.theme_name = s.theme_name.clone();
            ui_store.shortcut_overrides = s.shortcuts.clone();
        }

        // Autosave preferences live in `app_state.ron` (not the workspace layout
        // file), defaulting on when absent so older profiles gain recovery.
        let autosave_prefs = crate::app::persistence::load_autosave_prefs();
        ui_store.view.autosave = crate::app::stores::ui_store::AutosaveState::from_prefs(
            autosave_prefs.enabled,
            autosave_prefs.interval_s,
        );

        let shortcut_registry = match ShortcutRegistry::with_overrides(&ui_store.shortcut_overrides)
        {
            Ok(registry) => registry,
            Err(error) => {
                tracing::warn!(
                    "Failed to apply persisted shortcuts, dropping invalid overrides: {error}"
                );
                ui_store.shortcut_overrides.clear();
                ShortcutRegistry::new()
            },
        };

        let mut shell = Self {
            document_store: DocumentStore::new(document, editor),
            workspace_store: WorkspaceStore::new(
                workspace_root,
                expanded_dirs,
                file_tree,
                persistence_path,
                hot_reloader,
                hot_reload_error,
            ),
            preview_store: PreviewStore::new(preview),
            ui_store,
            export_store: ExportStore::new(),
            rebuild_worker: RebuildWorker::start(),
            plugin_manager,
            external_commands: CommandQueue::new(),
            insertion_palette: InsertionPalette::default(),
            shortcut_registry,
            window_size,
            window_maximized,
        };
        if let Some(s) = persistence.as_ref().and_then(|p| p.settings.as_ref()) {
            shell.document_store.history.undo_limit = s.undo_limit;
        }

        let plugin_issues = shell.plugin_manager.snapshot().issues;
        if !plugin_issues.is_empty() {
            let message = plugin_issues
                .iter()
                .map(|issue| {
                    issue
                        .path
                        .as_ref()
                        .map(|path| format!("{}: {}", path.display(), issue.message))
                        .unwrap_or_else(|| issue.message.clone())
                })
                .collect::<Vec<_>>()
                .join("; ");
            shell
                .ui_store
                .toasts
                .push(Toast::warning(format!("Plugin load issue: {message}")));
        }

        if !is_welcome {
            shell.document_store.publish_rebuild_result(
                error.is_none()
                    && !has_source_load_failure(&shell.document_store.source.document.diagnostics),
            );
            // Offer crash recovery when a newer sidecar exists. Runs before the
            // UI loop so the prompt's `is_open` is set on the first frame.
            shell.detect_recovery_prompt();
        }
        shell
    }

    fn is_playing(&self) -> bool {
        self.preview_store.is_playing()
    }

    fn has_pending_rebuild(&self) -> bool {
        self.preview_store.has_pending_rebuild()
    }

    fn prepare_frame(&mut self) {
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.preview_store.last_frame_at);
        self.preview_store.last_frame_at = now;

        // Check for hot reload
        self.check_hot_reload(now);

        // Crash-recovery autosave: write the live editor text to the sidecar
        // when the document is dirty and the interval has elapsed.
        self.autosave_tick(now);

        // Poll plugin manifests/libraries for changes and reload atomically.
        if self.plugin_manager.poll() {
            self.apply_plugin_reload();
        }

        // Poll background export status
        self.export_store.poll_export_status();

        if self.preview_store.preview.playback.is_playing {
            self.preview_store.preview.playback.tick(delta);
            self.preview_store.preview_dirty = true;

            if self.ui_store.editor_sync_enabled {
                if let Some(line) = self
                    .document_store
                    .source
                    .document
                    .find_keyframe_line_at(self.preview_store.preview.playback.current_time_s())
                {
                    if self.document_store.source.editor.highlighted_line != Some(line) {
                        self.document_store.source.editor.scroll_to_line(line);
                        self.document_store.source.editor.set_highlighted_line(Some(line));
                    }
                }
            }
        }

        self.sync_active_scene_from_time();

        if let Some(deadline) = self.preview_store.pending_rebuild_at
            && now >= deadline
        {
            self.preview_store.pending_rebuild_at = None;
            self.preview_store.preview.error = None;
            if let Some(token) = crate::app::handlers::file::handle_rebuild_submit(
                &mut self.rebuild_worker,
                &mut self.document_store,
                &mut self.preview_store,
            ) {
                self.preview_store.in_flight_rebuild = Some(token);
            }
        }

        // Poll for completed rebuild responses
        for response in self.rebuild_worker.poll() {
            // Only accept the highest-token response (newest)
            if self.preview_store.in_flight_rebuild.is_none_or(|token| response.token == token) {
                let elapsed_ms = response.elapsed_ms as f64;
                let effects = crate::app::handlers::file::handle_rebuild_response(
                    &mut self.document_store,
                    &mut self.preview_store,
                    &mut self.ui_store,
                    response,
                );
                self.preview_store.performance_metrics.record_rebuild(elapsed_ms);
                self.apply_effects(effects);
                self.preview_store.in_flight_rebuild = None;
            }
        }
    }

    /// Log raw IME/text input events with the current editor focus state so
    /// native input-method failures can be diagnosed without deep egui logs.
    fn log_input_debug(&self, ctx: &egui::Context) {
        if !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }
        let events: Vec<egui::Event> = ctx.input(|i| {
            i.events
                .iter()
                .filter(|event| matches!(event, egui::Event::Ime(_) | egui::Event::Text(_)))
                .cloned()
                .collect()
        });
        if !events.is_empty() {
            tracing::debug!(
                events = ?events,
                text_edit_focused = ctx.text_edit_focused(),
                focused_cell = self.document_store.source.editor.focused_cell(),
                "IME/text input"
            );
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, preview_texture_id: Option<egui::TextureId>) {
        let theme = eparts::theme(ui);
        // Track the window size so layout presets and reset can size themselves.
        let screen = ui.ctx().content_rect();
        self.ui_store.view.layout_size = (screen.width(), screen.height());
        let mut commands: ActionQueue = ActionQueue::default();
        commands.append(&mut self.ui_store.pending_actions);
        self.external_commands.drain_into(&mut commands);

        // Global keyboard shortcuts are now handled via ShortcutRegistry
        // in runtime.rs::handle_keyboard_shortcuts. Only shell-local
        // modal Escapes remain inline below.

        // Compact toolbar — hidden during onboarding so no grid/zoom controls clutter
        // the welcome screen.
        if !self.ui_store.view.welcome_open {
            egui::Panel::top("toolbar")
                .resizable(false)
                .show_inside(ui, |ui| self.toolbar_ui(ui, &mut commands));
        }

        let diagnostics = self.document_store.combined_diagnostics();

        // Diagnostics panel (collapsible)
        if self.ui_store.view.diagnostics_panel_visible {
            egui::Panel::bottom("diagnostics_panel")
                .resizable(true)
                .default_size(180.0)
                .min_size(80.0)
                .max_size(400.0)
                .show_inside(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if diagnostics.is_empty() {
                        ui.add_space(SPACE_4);
                        ui.add(Alert::new(
                            format!(
                                "No diagnostics — all clear {}",
                                egui_phosphor::regular::CHECK_CIRCLE
                            ),
                            AlertLevel::Success,
                        ));
                    } else {
                        let has_error = diagnostics.iter().any(|d| d.is_error());
                        if let Some(message) = diagnostics_banner_message(&diagnostics) {
                            ui.add_space(SPACE_2);
                            ui.add(Alert::new(
                                message,
                                if has_error {
                                    AlertLevel::Error
                                } else {
                                    AlertLevel::Warning
                                },
                            ));
                            ui.add_space(SPACE_2);
                        }
                        if let Some(target) = components::diagnostics::diagnostics_list(
                            ui,
                            &diagnostics,
                            &mut self.ui_store.view.diagnostics_panel_visible,
                        ) {
                            self.ui_store.pending_actions.push_back(
                                ViewCommand::ScrollToLine(target.line, target.column).into(),
                            );
                        }
                    }
                });
        }

        // Status bar — thin bar at the bottom showing preview status and scene dimensions
        // Filled by the diagnostics chip below; the peek overlay anchors to it.
        let mut diagnostics_chip_rect: Option<egui::Rect> = None;

        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(theme.palette.surface.panel)
                    .inner_margin(egui::Margin::symmetric(8, 2)),
            )
            .resizable(false)
            .min_size(20.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let status = &self.preview_store.preview.status;
                    if !status.is_empty() {
                        let is_error =
                            self.preview_store.preview.status_severity == StatusSeverity::Error;
                        if is_error {
                            // Red accent pill + warning icon for errors
                            let (bg_rect, _) = ui
                                .allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            ui.painter().rect_filled(
                                bg_rect,
                                RADIUS_S,
                                theme.palette.status.diagnostic_error.linear_multiply(0.3),
                            );
                            ui.painter().text(
                                egui::pos2(bg_rect.center().x, bg_rect.center().y),
                                egui::Align2::CENTER_CENTER,
                                egui_phosphor::regular::WARNING,
                                TextRole::Micro.font_id(),
                                theme.palette.status.diagnostic_error,
                            );
                            ui.add_space(SPACE_2);
                        }
                        let color = if is_error {
                            theme.palette.status.diagnostic_error
                        } else {
                            theme.palette.text.muted
                        };
                        let label = ui.label(
                            egui::RichText::new(status.as_str())
                                .size(TextRole::Micro.size())
                                .color(color),
                        );
                        if is_error && self.preview_store.preview.error.is_some() {
                            text_tooltip(
                                ui,
                                ui.id().with("status_error_tooltip"),
                                &label,
                                self.preview_store.preview.error.as_deref().unwrap_or(""),
                            );
                        }
                    }
                    // Right side: diagnostics count + scene dimensions
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let dims = &self.document_store.source.document.scene_dimensions;
                        ui.label(
                            egui::RichText::new(format!("{}×{}", dims.width, dims.height))
                                .size(TextRole::Micro.size())
                                .color(theme.palette.text.muted),
                        );

                        // Diagnostics chip: always-visible error/warning counts that
                        // toggle the panel, so an open issue is never invisible.
                        let errors = diagnostics.iter().filter(|d| d.is_error()).count();
                        let warnings = diagnostics.len() - errors;
                        let (chip_icon, chip_color, chip_label) = if errors > 0 {
                            (
                                egui_phosphor::regular::X_CIRCLE,
                                theme.palette.status.error,
                                format!("{errors} error(s)"),
                            )
                        } else if warnings > 0 {
                            (
                                egui_phosphor::regular::WARNING,
                                theme.palette.status.warning,
                                format!("{warnings} warning(s)"),
                            )
                        } else {
                            (
                                egui_phosphor::regular::CHECK_CIRCLE,
                                theme.palette.status.success,
                                "No problems".to_string(),
                            )
                        };
                        let chip = ui.add(
                            Button::ghost("")
                                .with_icon(chip_icon)
                                .icon_color(chip_color)
                                .hover_icon_color(chip_color)
                                .active(
                                    self.ui_store.view.diagnostics_panel_visible
                                        || self.ui_store.view.diagnostics_peek_open,
                                ),
                        );
                        text_tooltip(
                            ui,
                            chip.id.with("diag_chip_tip"),
                            &chip,
                            &format!(
                                "{chip_label} — click to peek, double-click to pin the \
                                 diagnostics panel"
                            ),
                        );
                        diagnostics_chip_rect = Some(chip.rect);
                        if chip.double_clicked() {
                            self.ui_store.view.diagnostics_peek_open = false;
                            self.ui_store.view.diagnostics_panel_visible =
                                !self.ui_store.view.diagnostics_panel_visible;
                        } else if chip.clicked() {
                            self.ui_store.view.diagnostics_peek_open =
                                !self.ui_store.view.diagnostics_peek_open;
                        }
                    });
                });
            });

        if self.ui_store.view.diagnostics_peek_open {
            match diagnostics_chip_rect {
                Some(chip_rect) => self.diagnostics_peek_ui(ui, chip_rect, &diagnostics),
                // The chip is not drawn while the welcome screen owns the
                // workspace; a stranded overlay would outlive its anchor.
                None => self.ui_store.view.diagnostics_peek_open = false,
            }
        }

        // Central workspace — edge-to-edge tiles, no outer margin
        // When welcome screen is open, show it instead of the workspace.
        let workspace_rect = egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin::ZERO))
            .show_inside(ui, |ui| {
                if self.ui_store.view.welcome_open {
                    let mut welcome_cmds = ActionQueue::default();
                    self.welcome_screen_ui(ui, &mut welcome_cmds);
                    for cmd in welcome_cmds {
                        let effects = self.handle_action(cmd);
                        self.apply_effects(effects);
                    }
                } else {
                    self.workspace_ui(ui, preview_texture_id, &mut commands);
                }
            })
            .response
            .rect;

        // Update cursor time from editor position (bi-directional sync)
        self.ui_store.cursor_time_s =
            self.document_store.source.editor.cursor_line.and_then(|line| {
                self.document_store.source.document.timeline_index.time_s_for_line(line)
            });

        self.handle_actions(commands);

        // Compact-mode overlay drawers (icon-rail sidebar and detail region).
        // Rendered below the modals and skipped while one is open, so a modal
        // always owns the screen (and the Escape key). Anchored to the central
        // workspace rect so they do not cover the toolbar or status bar.
        if !self.ui_store.view.welcome_open && !self.modal_open() {
            let mut drawer_cmds = ActionQueue::default();
            self.compact_sidebar_drawer_ui(ui, workspace_rect, &mut drawer_cmds);
            self.compact_detail_drawer_ui(ui, workspace_rect, &mut drawer_cmds);
            self.handle_actions(drawer_cmds);
        }

        // Safety net: the preview panel clears the Library drag payload on
        // release, but it does not render on the welcome screen (or if the
        // preview pane is ever hidden). Drop any leftover payload once the
        // pointer is up so a stale drag cannot drop on a later frame.
        if ui.input(|i| i.pointer.any_released()) {
            panels::clear_library_drag(ui.ctx());
        }

        // Settings modal overlay (rendered on top of everything)
        if self.ui_store.view.settings_open {
            self.settings_dialog_ui(ui);
        }

        // Workspace switcher dialog overlay
        if self.ui_store.view.workspace_switcher_open {
            self.workspace_switcher_ui(ui);
        }

        // Export dialog overlay
        if self.export_store.export_dialog_open {
            self.export_dialog_ui(ui);
        }

        // Insertion palette overlay
        self.insertion_palette_ui(ui);

        // Shortcut cheat sheet overlay
        if self.ui_store.view.shortcuts_open {
            self.shortcut_cheat_sheet_ui(ui);
        }

        // Plugin status overlay
        if self.ui_store.view.plugin_status_open {
            self.plugin_status_ui(ui);
        }

        // Command palette overlay
        if self.ui_store.view.command_palette_open {
            self.command_palette_ui(ui);
        }

        // Find / Replace overlay
        if self.ui_store.view.find_replace_open {
            self.find_replace_ui(ui);
        }

        // Unsaved changes dialog
        if self.ui_store.unsaved_changes.is_open {
            self.unsaved_changes_dialog_ui(ui);
        }

        // Crash-recovery prompt (startup, when a newer sidecar exists)
        if self.ui_store.recovery_prompt.is_open {
            self.recovery_prompt_ui(ui);
        }

        // Toast notifications
        let now = Instant::now();
        self.ui_store.toasts.show(ui, now);
    }

    /// Welcome / onboarding screen shown when no document is loaded.
    fn welcome_screen_ui(&mut self, ui: &mut egui::Ui, commands: &mut ActionQueue) {
        let theme = eparts::theme(ui);
        let sp = spatial(ui);
        let avail = ui.available_rect_before_wrap();
        ui.painter().rect_filled(avail, 0.0, theme.palette.surface.base);

        ui.vertical_centered(|ui| {
            ui.add_space(avail.height() * WELCOME_TOP_OFFSET_FRAC);

            // ── Centered card ──
            egui::Frame::new()
                .fill(theme.palette.surface.surface)
                .stroke(Stroke::new(STROKE_WIDTH, theme.palette.border.default))
                .corner_radius(RADIUS_L)
                .inner_margin(egui::Margin::symmetric(40, 36))
                .show(ui, |ui| {
                    ui.set_max_width(280.0);

                    ui.vertical_centered(|ui| {
                        // Icon with circular background
                        let icon_size = 56.0;
                        let (icon_rect, _) = ui.allocate_exact_size(
                            egui::vec2(icon_size, icon_size),
                            egui::Sense::hover(),
                        );
                        ui.painter().circle_filled(
                            icon_rect.center(),
                            icon_size * 0.5,
                            theme.palette.surface.widget,
                        );
                        ui.painter().text(
                            icon_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            egui_phosphor::regular::FILM_STRIP,
                            TextRole::Display.font_id(),
                            theme.palette.accent.primary,
                        );
                        ui.add_space(sp.base.space_5 * 1.5);

                        // Title
                        ui.label(
                            egui::RichText::new("Welcome to Animatix")
                                .font(TextRole::Heading.font_id())
                                .color(theme.palette.text.primary)
                                .strong(),
                        );
                        ui.add_space(sp.base.space_2);

                        // Subtitle
                        ui.label(
                            egui::RichText::new("Layout-first animation for creative coders")
                                .size(TextRole::Body.size())
                                .color(theme.palette.text.secondary),
                        );
                        ui.add_space(sp.base.space_5 * 2.5);

                        let btn_w = ui.available_width();

                        // Primary: Create new scene
                        let new_resp = ui.add_sized(
                            egui::vec2(btn_w, sp.welcome.btn_height),
                            Button::primary("Create new scene")
                                .with_icon(egui_phosphor::regular::PLUS),
                        );
                        if new_resp.clicked() {
                            let path = default_file_path();
                            match std::fs::write(&path, "#0s\n") {
                                Ok(_) => {},
                                Err(e) => {
                                    self.ui_store
                                        .toasts
                                        .push(Toast::error(format!("Failed to create scene: {e}")));
                                    return; // don't proceed to open
                                },
                            }
                            commands.push_back(DocumentCommand::OpenFile(path).into());
                        }

                        ui.add_space(sp.base.space_3);

                        // Secondary: open existing file
                        let open_resp = ui.add_sized(
                            egui::vec2(btn_w, sp.welcome.btn_height),
                            Button::ghost("Open existing file")
                                .with_icon(egui_phosphor::regular::FOLDER_OPEN),
                        );
                        if open_resp.clicked() {
                            if let Some(path) =
                                rfd::FileDialog::new().add_filter("Animatix", &["amx"]).pick_file()
                            {
                                commands.push_back(DocumentCommand::OpenFile(path).into());
                            }
                        }

                        ui.add_space(sp.base.space_3);

                        // Tertiary: open workspace
                        let ws_resp = ui.add_sized(
                            egui::vec2(btn_w, sp.welcome.btn_height),
                            Button::ghost("Open workspace")
                                .with_icon(egui_phosphor::regular::FOLDER_NOTCH),
                        );
                        if ws_resp.clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                commands.push_back(DocumentCommand::SwitchWorkspace(path).into());
                                self.ui_store.view.welcome_open = false;
                            }
                        }
                    });
                });
        });
    }

    fn workspace_ui(
        &mut self,
        ui: &mut egui::Ui,
        preview_texture_id: Option<egui::TextureId>,
        commands: &mut ActionQueue,
    ) {
        // ── Compact (narrow-window) downgrade ──
        // Compute the decision from the live width and reconcile the dock only
        // when it flips. On entry the detail column is hidden (it renders as an
        // overlay drawer); on exit the pane visibility captured on entry is
        // restored so explicit user choices are not overridden.
        let avail = ui.available_size();
        let compact = crate::app::persistence::compact_for_width(avail.x);
        let was_compact = self.ui_store.view.compact;
        if compact != was_compact {
            let transition = crate::app::persistence::reconcile_compact(
                &mut self.ui_store.view.tree,
                was_compact,
                compact,
                &mut self.ui_store.view.compact_restore,
            );
            if transition != crate::app::persistence::CompactTransition::Unchanged {
                tracing::debug!(
                    compact,
                    width = avail.x,
                    ?transition,
                    "workspace compact-mode changed"
                );
            }
            // Overlay drawers are compact-only, and any open drawer is stale
            // after a breakpoint crossing.
            self.ui_store.set_compact(compact);
        }

        let preset = self.ui_store.view.layout_preset;
        if compact {
            // The sidebar is a fixed-width icon rail; ignore the preset's
            // sidebar pixel floor and give the rest to the preview.
            crate::app::persistence::enforce_compact_layout(
                &mut self.ui_store.view.tree,
                preset,
                avail.x,
                avail.y,
            );
        } else {
            // Keep region sizes inside the preset's pixel bounds before layout,
            // so a window smaller than the build-time reference does not scale
            // panels below their floors.
            crate::app::persistence::enforce_layout_bounds(
                &mut self.ui_store.view.tree,
                preset,
                avail.x,
                avail.y,
            );
        }

        // Refresh find-match decorations once per frame from the shared find
        // state, before the Code pane renders. The renderer overlays them on
        // the cached highlight jobs, so query/option changes take effect
        // without invalidating the highlight cache.
        self.document_store.source.editor.set_find_state(
            &self.ui_store.find_query,
            self.ui_store.find_case_sensitive,
            self.ui_store.find_whole_word,
            self.ui_store.find_regex,
            self.ui_store.find_last_match,
        );

        let tree = &mut self.ui_store.view.tree;
        let mut behavior = panels::behavior::WorkspaceBehavior {
            document_store: &mut self.document_store,
            workspace_store: &mut self.workspace_store,
            preview_store: &mut self.preview_store,
            commands,
            preview_texture_id,
            collapsed_actors: &mut self.ui_store.view.collapsed_actors,
            expanded_properties: &mut self.ui_store.view.expanded_properties,
            selected_actors: &mut self.ui_store.selection.selected_actors,
            hit_regions: &self.ui_store.selection.hit_regions,
            drag_state: &mut self.ui_store.interaction.drag_state,
            selection: &mut self.ui_store.selection.selection,
            pivot_offsets: &mut self.ui_store.pivot_offsets,
            tool_mode: &mut self.ui_store.view.tool_mode,
            sidebar_tab: &mut self.ui_store.sidebar_tab,
            compact,
            compact_drawer: &mut self.ui_store.view.compact_drawer,
            property_view_mode: &mut self.ui_store.property_view_mode,
            keyframe_view_mode: &mut self.ui_store.keyframe_view_mode,
            keyframe_mode: self.ui_store.keyframe_mode,
            rotation_snap_degrees: self.ui_store.rotation_snap_degrees,
            snap_fps: self.ui_store.snap_fps,
            debug_layout: self.ui_store.view.debug_layout,
            debug_spacing: self.ui_store.view.debug_spacing,
            timeline_focused: &mut self.ui_store.view.timeline_focused,
            selected_keyframes: &mut self.ui_store.selection.selected_keyframes,
        };
        tree.ui(&mut behavior, ui);
    }

    /// True when any modal/overlay other than the compact drawers is open.
    /// Used so drawer Escape handling does not race a modal's own Escape.
    fn modal_open(&self) -> bool {
        self.ui_store.view.settings_open
            || self.ui_store.view.workspace_switcher_open
            || self.export_store.export_dialog_open
            || self.insertion_palette.open
            || self.ui_store.view.shortcuts_open
            || self.ui_store.view.plugin_status_open
            || self.ui_store.view.command_palette_open
            || self.ui_store.view.find_replace_open
            || self.ui_store.unsaved_changes.is_open
            || self.ui_store.recovery_prompt.is_open
    }

    /// Compact-mode sidebar overlay drawer.
    ///
    /// Renders the same content as the docked sidebar (via
    /// `sidebar_tab_content_ui`) in a floating left panel, opened from the icon
    /// rail. Shown only while the drawer is the active compact drawer.
    /// Transient diagnostics overlay raised from the status-bar chip.
    ///
    /// Anchored above the chip so the counts stay readable without docking the
    /// bottom panel. Transient by design: Escape, a click outside, or picking a
    /// diagnostic (which also scrolls the editor to it) closes it.
    fn diagnostics_peek_ui(
        &mut self,
        ui: &mut egui::Ui,
        chip_rect: egui::Rect,
        diagnostics: &[Diagnostic],
    ) {
        let theme = eparts::theme(ui);
        let screen = ui.ctx().viewport_rect();
        let width = 440.0f32.min(screen.width() - 16.0);
        // Anchor above the chip, nudged inside the screen if it would overflow.
        let x = (chip_rect.right() - width).max(screen.left() + 8.0);
        let y = (chip_rect.top() - 12.0).max(screen.top() + 8.0);
        let errors = diagnostics.iter().filter(|d| d.is_error()).count();
        let warnings = diagnostics.len() - errors;
        let mut close = false;
        let mut scroll_to: Option<(usize, usize)> = None;

        let overlay = egui::Area::new(egui::Id::new("diagnostics_peek"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(x, y))
            .show(ui.ctx(), |ui| {
                ui.set_width(width);
                egui::Frame::new()
                    .fill(theme.palette.surface.panel)
                    .stroke(Stroke::new(STROKE_WIDTH, theme.palette.border.default))
                    .corner_radius(RADIUS_L)
                    .inner_margin(egui::Margin::same(8))
                    .shadow(theme.elevation_overlay())
                    .show(ui, |ui| {
                        ui.set_width(width - 16.0);
                        ui.horizontal(|ui| {
                            let (icon, color) = if errors > 0 {
                                (egui_phosphor::regular::X_CIRCLE, theme.palette.status.error)
                            } else if warnings > 0 {
                                (egui_phosphor::regular::WARNING, theme.palette.status.warning)
                            } else {
                                (egui_phosphor::regular::CHECK_CIRCLE, theme.palette.status.success)
                            };
                            ui.label(
                                egui::RichText::new(icon).size(TextRole::Body.size()).color(color),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "{errors} error(s) · {warnings} warning(s)"
                                ))
                                .size(TextRole::BodyS.size())
                                .color(theme.palette.text.primary),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let pin = ui.add(
                                        Button::ghost("")
                                            .with_icon(egui_phosphor::regular::ARROW_SQUARE_DOWN),
                                    );
                                    text_tooltip(
                                        ui,
                                        pin.id.with("diag_peek_pin"),
                                        &pin,
                                        "Pin to the bottom panel",
                                    );
                                    if pin.clicked() {
                                        self.ui_store.view.diagnostics_panel_visible = true;
                                        close = true;
                                    }
                                    let dismiss = ui.add(
                                        Button::ghost("").with_icon(egui_phosphor::regular::X),
                                    );
                                    if dismiss.clicked() {
                                        close = true;
                                    }
                                },
                            );
                        });
                        ui.separator();
                        egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                            if diagnostics.is_empty() {
                                ui.add_space(SPACE_2);
                                ui.label(
                                    egui::RichText::new("No diagnostics — all clear")
                                        .size(TextRole::BodyS.size())
                                        .color(theme.palette.status.success),
                                );
                            } else if let Some(target) = components::diagnostics::diagnostics_list(
                                ui,
                                diagnostics,
                                &mut close,
                            ) {
                                scroll_to = Some((target.line, target.column));
                                close = true;
                            }
                        });
                    });
            })
            .response;

        if let Some((line, column)) = scroll_to {
            self.ui_store
                .pending_actions
                .push_back(ViewCommand::ScrollToLine(line, column).into());
        }

        let escape = ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));
        let clicked_outside = ui.ctx().input(|i| {
            i.pointer.any_pressed()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|p| !overlay.rect.contains(p) && !chip_rect.contains(p))
        });
        if close || escape || clicked_outside {
            self.ui_store.view.diagnostics_peek_open = false;
        }
    }

    fn compact_sidebar_drawer_ui(
        &mut self,
        ui: &mut egui::Ui,
        screen: egui::Rect,
        commands: &mut ActionQueue,
    ) {
        if self.ui_store.view.compact_drawer != Some(panels::CompactDrawer::Sidebar) {
            return;
        }
        let theme = eparts::theme(ui);
        let width = (screen.width() * 0.40).clamp(240.0, 360.0);
        let margin = 8.0;
        let mut close = false;
        let active_tab = self.ui_store.sidebar_tab;

        egui::Area::new(egui::Id::new("compact_sidebar_drawer"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(screen.left() + margin, screen.top() + margin))
            .show(ui.ctx(), |ui| {
                ui.set_width(width);
                ui.set_max_height((screen.height() - 2.0 * margin).max(160.0));
                egui::Frame::new()
                    .fill(theme.palette.surface.panel)
                    .stroke(Stroke::new(STROKE_WIDTH, theme.palette.border.default))
                    .corner_radius(RADIUS_L)
                    .inner_margin(egui::Margin::same(8))
                    .shadow(theme.elevation_overlay())
                    .show(ui, |ui| {
                        ui.set_width(width - 16.0);
                        // Swallow canvas drags underneath the drawer. Drag-only
                        // sense: egui prefers a smaller clickable widget over a
                        // big drag background, so the drawer's own controls
                        // still win the hit test.
                        let _ = ui.interact(
                            ui.max_rect(),
                            ui.id().with("drawer_block"),
                            egui::Sense::drag(),
                        );
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(crate::app::panels::sidebar_tab_label(
                                    active_tab,
                                ))
                                .size(TextRole::Heading.size())
                                .color(theme.palette.text.primary),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            Button::icon(egui_phosphor::regular::X)
                                                .with_tooltip("Close (Esc)"),
                                        )
                                        .clicked()
                                    {
                                        close = true;
                                    }
                                },
                            );
                        });
                        ui.separator();

                        let timeline = self.document_store.source.document.timeline.as_ref();
                        let asset_cache = timeline.map(|t| t.asset_cache());
                        let mut ctx = panels::sidebar::SidebarContext {
                            active_scene: self
                                .document_store
                                .source
                                .document
                                .active_scene
                                .as_deref(),
                            is_composition: self.document_store.source.document.is_composition(),
                            composition: self.document_store.source.document.composition.as_ref(),
                            current_file: &self.document_store.source.document.file_path,
                            expanded_dirs: &mut self.workspace_store.expanded_dirs,
                            file_tree: &self.workspace_store.file_tree,
                            preview: &mut self.preview_store.preview,
                            commands,
                            scene_dimensions: self.document_store.source.document.scene_dimensions,
                            timeline,
                            selected_actors: &mut self.ui_store.selection.selected_actors,
                            collapsed_actors: &mut self.ui_store.view.collapsed_actors,
                            sidebar_tab: &mut self.ui_store.sidebar_tab,
                            editor: &mut self.document_store.source.editor,
                            components: &self.document_store.source.document.components,
                            asset_cache,
                            compact: false,
                            compact_drawer: &mut self.ui_store.view.compact_drawer,
                        };
                        panels::sidebar::sidebar_tab_content_ui(&mut ctx, ui, active_tab);
                    });
            });

        if close || (!self.modal_open() && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
            self.ui_store.view.compact_drawer = None;
        }
    }

    /// Compact-mode detail overlay drawer (Inspector or Code).
    ///
    /// The docked detail column is hidden while compact, so this is the only
    /// place the active detail tab renders — no double render. The tab is shared
    /// with the dock tree, so toolbar/preset switches land here too.
    fn compact_detail_drawer_ui(
        &mut self,
        ui: &mut egui::Ui,
        screen: egui::Rect,
        commands: &mut ActionQueue,
    ) {
        if self.ui_store.view.compact_drawer != Some(panels::CompactDrawer::Detail) {
            return;
        }
        // Fall back to Inspector if the tree has no active detail tab.
        let active_tab = crate::app::persistence::active_detail_tab(&self.ui_store.view.tree)
            .unwrap_or(WorkspaceTab::Inspector);

        let theme = eparts::theme(ui);
        let width = (screen.width() * 0.42).clamp(260.0, 420.0);
        let margin = 8.0;
        let mut close = false;
        let mut switch_to: Option<WorkspaceTab> = None;

        egui::Area::new(egui::Id::new("compact_detail_drawer"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(screen.right() - margin - width, screen.top() + margin))
            .show(ui.ctx(), |ui| {
                ui.set_width(width);
                ui.set_max_height((screen.height() - 2.0 * margin).max(160.0));
                egui::Frame::new()
                    .fill(theme.palette.surface.panel)
                    .stroke(Stroke::new(STROKE_WIDTH, theme.palette.border.default))
                    .corner_radius(RADIUS_L)
                    .inner_margin(egui::Margin::same(8))
                    .shadow(theme.elevation_overlay())
                    .show(ui, |ui| {
                        ui.set_width(width - 16.0);
                        // Swallow canvas drags underneath the drawer (see the
                        // sidebar drawer for the rationale).
                        let _ = ui.interact(
                            ui.max_rect(),
                            ui.id().with("drawer_block"),
                            egui::Sense::drag(),
                        );
                        ui.horizontal(|ui| {
                            for (tab, label) in [
                                (WorkspaceTab::Inspector, "Inspector"),
                                (WorkspaceTab::Code, "Code"),
                            ] {
                                let active = active_tab == tab;
                                let resp = ui.add(Button::ghost(label).active(active));
                                if resp.clicked() && !active {
                                    switch_to = Some(tab);
                                }
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            Button::icon(egui_phosphor::regular::X)
                                                .with_tooltip("Close (Esc)"),
                                        )
                                        .clicked()
                                    {
                                        close = true;
                                    }
                                },
                            );
                        });
                        ui.separator();

                        match active_tab {
                            WorkspaceTab::Code => {
                                let diagnostics = self.document_store.combined_diagnostics();
                                let mut ctx = panels::editor::EditorContext {
                                    editor: &mut self.document_store.source.editor,
                                    diagnostics: &diagnostics,
                                    source_dirty: &mut self
                                        .document_store
                                        .source
                                        .document
                                        .source_text,
                                    commands,
                                    is_playing: self.preview_store.preview.playback.is_playing,
                                };
                                panels::editor::editor_ui(&mut ctx, ui);
                            },
                            _ => {
                                let active_tl =
                                    self.document_store.source.document.active_timeline();
                                let mut ctx = panels::inspector::InspectorContext {
                                    preview: &mut self.preview_store.preview,
                                    timeline: active_tl,
                                    composition: self
                                        .document_store
                                        .source
                                        .document
                                        .composition
                                        .as_ref(),
                                    active_scene: self
                                        .document_store
                                        .source
                                        .document
                                        .active_scene
                                        .as_deref(),
                                    selected_actors: &mut self.ui_store.selection.selected_actors,
                                    commands,
                                    keyframe_mode: self.ui_store.keyframe_mode,
                                    scene_dimensions: self
                                        .document_store
                                        .source
                                        .document
                                        .scene_dimensions,
                                    pivot_offsets: &mut self.ui_store.pivot_offsets,
                                    property_view_mode: &mut self.ui_store.property_view_mode,
                                    keyframe_view_mode: &mut self.ui_store.keyframe_view_mode,
                                };
                                panels::inspector::inspector_panel_ui(&mut ctx, ui);
                            },
                        }
                    });
            });

        if let Some(tab) = switch_to {
            crate::app::persistence::set_active_detail_tab(&mut self.ui_store.view.tree, tab);
        }
        if close || (!self.modal_open() && ui.input(|i| i.key_pressed(egui::Key::Escape))) {
            self.ui_store.view.compact_drawer = None;
        }
    }

    /// Return a cloneable sender for commands submitted outside egui callbacks.
    ///
    /// Reserved for integration tests and future remote control; the queue is
    /// owned by the shell and drained every frame.
    #[allow(dead_code)] // Exposed for integration tests and future remote-control wiring.
    pub(crate) fn external_command_sender(&self) -> CommandSender {
        self.external_commands.sender()
    }

    fn handle_actions(&mut self, actions: ActionQueue) {
        for action in actions {
            let effects = self.handle_action(action);
            self.apply_effects(effects);
        }
    }

    /// Apply a collection of side effects produced by a command handler.
    ///
    /// Effects are applied *after* all state mutations for the command have been
    /// performed, ensuring that side-effect code (UI toasts, status text, editor
    /// sync, etc.) runs in a consistent state.
    fn apply_effects(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Toast(toast) => {
                    self.ui_store.toasts.push(toast);
                },
                Effect::Status(status) => {
                    self.preview_store.preview.set_status_info(status);
                },
                Effect::Repaint => {
                    self.preview_store.preview_dirty = true;
                },
                Effect::EditorScroll(line) => {
                    self.document_store.source.editor.scroll_to_line(line);
                },
                Effect::EditorHighlight(line) => {
                    self.document_store.source.editor.set_highlighted_line(Some(line));
                },
                Effect::RebuildScheduled => {
                    // The status has already been set; the pending rebuild
                    // timer is set directly in the command handler.
                },
            }
        }
    }

    /// Force-clear any active error state (parse or render).
    fn clear_any_error(&mut self, status: String) {
        self.document_store.history.render_diagnostics.clear();
        self.document_store.history.runtime_diagnostics.clear();
        self.preview_store.preview.error = None;
        self.preview_store.preview.status = status;
    }

    fn save_persistence(&self) {
        if let Some(parent) = self.workspace_store.persistence_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                tracing::warn!("Failed to create persistence directory: {}", e);
            }
        }
        let persistence = WorkspacePersistence {
            tree: self.ui_store.view.tree.clone(),
            window_size: Some(self.window_size),
            window_maximized: Some(self.window_maximized),
            settings: Some(SettingsPersistence {
                rebuild_debounce_ms: self.ui_store.rebuild_debounce_ms,
                scrub_step_s: self.ui_store.scrub_step_s,
                nudge_step_px: self.ui_store.nudge_step_px,
                nudge_step_shift_px: self.ui_store.nudge_step_shift_px,
                rotation_snap_degrees: self.ui_store.rotation_snap_degrees,
                snap_fps: self.ui_store.snap_fps,
                keyframe_merge_window_s: self.ui_store.keyframe_merge_window_s,
                undo_limit: self.document_store.history.undo_limit,
                grid_size: self.preview_store.preview.overlay.grid_size,
                app_theme: match self.ui_store.view.app_theme {
                    eparts::AppThemeChoice::Light => "light",
                    eparts::AppThemeChoice::Dark => "dark",
                    eparts::AppThemeChoice::Auto => "auto",
                }
                .to_string(),
                reduce_motion: self.ui_store.view.reduce_motion,
                density: match self.ui_store.view.density {
                    eparts::Density::Compact => "compact",
                    eparts::Density::Default => "default",
                }
                .to_string(),
                theme_dir: self.ui_store.view.theme_dir.clone(),
                theme_name: self.ui_store.view.theme_name.clone(),
                shortcuts: self.ui_store.shortcut_overrides.clone(),
                plugin_paths: self.plugin_manager.explicit_plugin_paths(),
            }),
        };
        if let Ok(serialized) =
            ron::ser::to_string_pretty(&persistence, ron::ser::PrettyConfig::default())
        {
            if let Err(e) = fs::write(&self.workspace_store.persistence_path, serialized) {
                tracing::warn!("Failed to write persistence file: {}", e);
            }
        }
    }

    /// Take a snapshot of the current source text for undo/redo.
    /// Call this BEFORE making a change to the source.
    fn snapshot(&mut self, label: UndoLabel) {
        let ui_before = self.ui_store.snapshot_with_preview(&self.preview_store);
        self.document_store.snapshot(label, ui_before);
    }

    fn sync_active_scene_from_time(&mut self) {
        if let Some(composition) = self.document_store.source.document.composition.as_ref() {
            let previous_scene = self.document_store.source.document.active_scene.clone();
            let (scene, _, _) =
                composition.evaluate(self.preview_store.preview.playback.current_time_s());
            let scene = (!scene.is_empty()).then_some(scene);
            if scene != previous_scene {
                // Keyframe selections are scene-qualified; automatic playback
                // crossing a scene boundary must not replay old-scene entries.
                self.ui_store.selection.selected_keyframes.clear();
            }
            self.document_store.source.document.active_scene = scene;
        }
    }

    fn set_status(&mut self, status: String, error: Option<String>) {
        if error.is_some() {
            self.preview_store.preview.set_status_error(status);
        } else {
            self.preview_store.preview.set_status_info(status);
        }
        self.preview_store.preview.error = error;
    }

    /// Apply a plugin reload to the current document and schedule a rebuild.
    fn apply_plugin_reload(&mut self) {
        let context = self.plugin_manager.context();
        let manifest = self.plugin_manager.manifest();
        let epoch = self.plugin_manager.epoch();
        self.document_store
            .source
            .document
            .set_extension_context(context, manifest, epoch);
        self.preview_store.pending_rebuild_at = Some(
            std::time::Instant::now()
                + std::time::Duration::from_millis(self.ui_store.rebuild_debounce_ms),
        );
        let issues = self.plugin_manager.snapshot().issues;
        if !issues.is_empty() {
            let message = issues
                .iter()
                .map(|issue| {
                    issue
                        .path
                        .as_ref()
                        .map(|path| format!("{}: {}", path.display(), issue.message))
                        .unwrap_or_else(|| issue.message.clone())
                })
                .collect::<Vec<_>>()
                .join("; ");
            self.ui_store
                .toasts
                .push(Toast::warning(format!("Plugin reload issue: {message}")));
            self.preview_store
                .preview
                .set_status_info("Plugin reload completed with issues");
        } else {
            self.preview_store.preview.set_status_info("Plugin reload completed");
        }
    }

    fn set_render_error(&mut self, error: String) {
        self.document_store.history.render_diagnostics = vec![Diagnostic::error(
            DiagnosticCode::RenderFailure,
            DiagnosticPhase::Render,
            error.clone(),
        )];
        self.preview_store.preview_dirty = false;
        self.set_status(format!("Render failed • {error}"), Some(error));
    }

    #[cfg(test)]
    fn clear_render_error(&mut self, status: String) {
        let active_render_error = self
            .document_store
            .history
            .render_diagnostics
            .first()
            .map(|diagnostic| diagnostic.message.clone());
        self.document_store.history.render_diagnostics.clear();

        if let Some(render_error) = active_render_error
            && self.preview_store.preview.error.as_deref() == Some(render_error.as_str())
            && self.preview_store.preview.status == format!("Render failed • {render_error}")
        {
            self.preview_store.preview.error = None;
            self.preview_store.preview.status = status;
        }
    }

    /// Copy currently selected actor labels into the clipboard buffer.
    fn copy_selected_actors(&mut self) {
        let count = self.ui_store.selection.selected_actors.len();
        self.ui_store.clipboard.clipboard_actors =
            self.ui_store.selection.selected_actors.iter().cloned().collect();
        self.preview_store.preview.status = format!("Copied {} actor(s)", count);
    }

    /// Workspace switcher dialog — small centered window for typing a directory path.
    fn workspace_switcher_ui(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let spec = dialog::DialogSpec::new("workspace_switcher", [400.0, 140.0])
            .with_min_size([360.0, 120.0]);

        let mut commands = ActionQueue::default();
        let open = dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let title_close = dialog::title_row(ui, "Switch Workspace");
            let mut body_close = false;

            ui.add_space(SPACE_3);
            ui.separator();
            ui.add_space(SPACE_3);

            ui.label(
                egui::RichText::new("Directory path")
                    .size(TextRole::BodyS.size())
                    .color(theme.palette.text.secondary),
            );
            ui.add_space(SPACE_2);
            eparts::TextField::new(&mut self.ui_store.workspace_switcher_path)
                .placeholder("/path/to/workspace")
                .show(ui);
            ui.add_space(SPACE_3);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let confirm = ui.add_sized([80.0, 28.0], Button::primary("Switch"));
                    if confirm.clicked() {
                        let path = PathBuf::from(&self.ui_store.workspace_switcher_path);
                        commands.push_back(DocumentCommand::SwitchWorkspace(path).into());
                        body_close = true;
                    }

                    let cancel = ui.add_sized([80.0, 28.0], Button::ghost("Cancel"));
                    if cancel.clicked() {
                        body_close = true;
                    }
                });
            });

            title_close || body_close
        });

        if !open {
            self.ui_store.view.workspace_switcher_open = false;
        }

        for cmd in commands {
            let effects = self.handle_action(cmd);
            self.apply_effects(effects);
        }
    }

    /// Confirmation dialog for unsaved changes (Save / Discard / Cancel).
    fn unsaved_changes_dialog_ui(&mut self, ui: &mut egui::Ui) {
        let theme = eparts::theme(ui);
        let spec = dialog::DialogSpec::new("unsaved_changes", [400.0, 200.0])
            .with_min_size([360.0, 180.0]);

        let open = dialog::modal(ui, &spec, |ui, _dc| -> bool {
            let title_close = dialog::title_row(
                ui,
                &format!("{}  Unsaved changes", egui_phosphor::regular::FLOPPY_DISK),
            );
            let mut body_close = false;

            ui.add_space(SPACE_3);
            ui.separator();
            ui.add_space(SPACE_3);

            ui.add(
                egui::Label::new(
                    egui::RichText::new(&self.ui_store.unsaved_changes.message)
                        .size(TextRole::Body.size())
                        .color(theme.palette.text.secondary),
                )
                .selectable(false),
            );
            ui.add_space(SPACE_5);

            let mut save_failed = false;
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Save button
                    let save = ui.add_sized(
                        [90.0, ROW_L],
                        Button::primary("Save").with_icon(egui_phosphor::regular::FLOPPY_DISK),
                    );
                    if save.clicked() {
                        if self.recovery_prompt_pending() {
                            // Saving here would clear a sidecar the user has not
                            // decided on yet; make them resolve that first.
                            self.ui_store.toasts.push(self.recovery_prompt_save_blocked());
                            save_failed = true;
                        } else if let Err(err) = file::save_document(&mut self.document_store) {
                            // Keep the dialog open if saving fails so unsaved edits are not lost.
                            self.preview_store
                                .preview
                                .set_status_error(format!("Save failed: {err}"));
                            self.ui_store.toasts.push(Toast::error(format!("Save failed: {err}")));
                            save_failed = true;
                        } else {
                            let was_close = self.ui_store.unsaved_changes.pending_close;
                            self.execute_unsaved_pending_action();
                            self.ui_store.unsaved_changes.close();
                            if was_close {
                                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            body_close = true;
                        }
                    }

                    // Discard button
                    let discard = ui.add_sized(
                        [90.0, ROW_L],
                        Button::danger("Discard").with_icon(egui_phosphor::regular::TRASH),
                    );
                    if discard.clicked() {
                        // Mark document as no longer dirty, then execute pending
                        self.document_store.source.document.is_dirty = false;
                        // The user explicitly threw the edits away, so the
                        // recovery sidecar must not resurrect them. Skipped when
                        // a recovery prompt is still pending, since that sidecar
                        // is a separate, undecided copy.
                        self.clear_recovery_for_current_document();
                        let was_close = self.ui_store.unsaved_changes.pending_close;
                        self.execute_unsaved_pending_action();
                        self.ui_store.unsaved_changes.close();
                        if was_close {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        body_close = true;
                    }

                    // Cancel button
                    let cancel = ui.add_sized([90.0, ROW_L], Button::ghost("Cancel"));
                    if cancel.clicked() {
                        self.ui_store.unsaved_changes.close();
                        body_close = true;
                    }
                });
            });

            if save_failed {
                return false;
            }

            title_close || body_close
        });

        if !open {
            self.ui_store.unsaved_changes.close();
        }
    }

    /// Execute the pending action stored in the unsaved changes dialog.
    fn execute_unsaved_pending_action(&mut self) {
        if let Some(action) = self.ui_store.unsaved_changes.pending_action.take() {
            let effects = self.handle_action(action);
            self.apply_effects(effects);
        }
    }
}

#[cfg(test)]
mod tests;
