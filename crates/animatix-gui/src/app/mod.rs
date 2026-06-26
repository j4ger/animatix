mod actions;
pub(crate) mod audio;
pub(crate) mod command_bus;
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
mod persistence;
pub(crate) mod preview;
mod runtime;
pub(crate) mod services;
pub(crate) mod shell;
pub(crate) mod stores;
mod utils;

use crate::app::design_tokens::semantic::{accent, border, status, surface, text};
use crate::app::design_tokens::spatial::welcome::TOP_OFFSET_FRAC as WELCOME_TOP_OFFSET_FRAC;
use crate::app::design_tokens::spatial::{spatial, RADIUS_L, RADIUS_M, RADIUS_S, ROW_L, SPACE_2, SPACE_3, SPACE_4, SPACE_5, STROKE_WIDTH};
use crate::app::design_tokens::typography::TextRole;
use crate::document::{DocumentSession, default_file_path};
use crate::editor::EditorBuffer;
use crate::hot_reload::{HotReloader, ReloadStatus};
use crate::preview_surface::PreviewSurface;
use animatix::timeline::SceneDimensions;
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase, diagnostics_phase_summary};
use directories::ProjectDirs;
use egui::{Color32, Stroke, Vec2};
use file_tree::{build_file_tree, workspace_root_for};
use persistence::{SettingsPersistence, WorkspacePersistence, default_tree, load_workspace_persistence, persistence_path};
#[cfg(test)]
use preview::fit_preview;

use crate::app::commands::{ActionQueue, DocumentCommand, Effect, UndoLabel, ViewCommand};
use crate::app::components::dialog;
use crate::app::components::toast::Toast;
use crate::app::document::rebuild::RebuildWorker;
use crate::app::handlers::file;
use crate::app::shell::insertion_palette::InsertionPalette;
use crate::app::stores::*;
use crate::app::utils::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
    Editor,
    Preview,
    Inspector,
    Timeline,
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

    fn clamp_time(&mut self) {
        let max_duration = self.duration_s.max(0.1);
        self.current_time_s = self.current_time_s.clamp(0.0, max_duration);
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
    insertion_palette: InsertionPalette,
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
                        self.preview_store.preview.status = "File reloaded".to_string();
                        self.preview_store.preview.error = None;
                        self.document_store.publish_rebuild_result(
                            self.document_store.source.document.last_rebuild_error.is_none(),
                        );
                    }
                },
                ReloadStatus::NoChange => {},
            }
        }
    }

    fn load(initial_path: PathBuf, show_welcome: bool) -> Self {
        let (document, status, error, is_welcome) = if show_welcome {
            // No recent file persisted — show welcome screen
            let doc = DocumentSession::from_error(initial_path.clone());
            (doc, None, None, true)
        } else {
            match DocumentSession::load(initial_path.clone()) {
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

        let workspace_root = workspace_root_for(&document.file_path);
        let expanded_dirs = HashSet::from([workspace_root.clone()]);
        let file_tree = build_file_tree(&workspace_root, &document.file_path, &expanded_dirs);
        let persistence_path = persistence_path();
        let persistence = load_workspace_persistence(&persistence_path);
        let tree = persistence.as_ref().map(|p| p.tree.clone()).unwrap_or_else(default_tree);
        let window_size =
            persistence.as_ref().and_then(|p| p.window_size).unwrap_or([1440.0, 960.0]);
        let window_maximized =
            persistence.as_ref().and_then(|p| p.window_maximized).unwrap_or(false);
        let hot_reloader = HotReloader::new(&document.file_path).ok();
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
            // undo_limit is on DocumentStore created below inside Self {} — skipped for now (default 100 is fine)
        }

        let mut shell = Self {
            document_store: DocumentStore::new(document, editor),
            workspace_store: WorkspaceStore::new(
                workspace_root,
                expanded_dirs,
                file_tree,
                persistence_path,
                hot_reloader,
            ),
            preview_store: PreviewStore::new(preview),
            ui_store,
            export_store: ExportStore::new(),
            rebuild_worker: RebuildWorker::start(),
            insertion_palette: InsertionPalette::default(),
            window_size,
            window_maximized,
        };
        if !is_welcome {
            shell.document_store.publish_rebuild_result(
                error.is_none()
                    && !has_source_load_failure(&shell.document_store.source.document.diagnostics),
            );
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
            let token = crate::app::handlers::file::handle_rebuild_submit(
                &mut self.rebuild_worker,
                &mut self.document_store,
                &mut self.preview_store,
            );
            self.preview_store.in_flight_rebuild = Some(token);
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

    fn ui(&mut self, ui: &mut egui::Ui, preview_texture_id: Option<egui::TextureId>) {
        let mut commands: ActionQueue = ActionQueue::default();
        commands.append(&mut self.ui_store.pending_actions);

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
                        ui.vertical_centered(|ui| {
                            ui.add_space(SPACE_4);
                            ui.label(
                                egui::RichText::new(format!(
                                "No diagnostics — all clear {}",
                                egui_phosphor::regular::CHECK_CIRCLE,
                            ))
                                    .size(TextRole::BodyS.size())
                                    .color(text::MUTED),
                            );
                        });
                    } else if let Some(target) = components::diagnostics::diagnostics_list(
                        ui,
                        &diagnostics,
                        &mut self.ui_store.view.diagnostics_panel_visible,
                    ) {
                        self.ui_store.pending_actions.push_back(
                            ViewCommand::ScrollToLine(target.line, target.column).into(),
                        );
                    }
                });
        }

        // Status bar — thin bar at the bottom showing preview status and scene dimensions
        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(surface::PANEL)
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
                            let (bg_rect, _) = ui.allocate_exact_size(
                                egui::vec2(14.0, 14.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                bg_rect,
                                RADIUS_S,
                                status::DIAGNOSTIC_ERROR.linear_multiply(0.3),
                            );
                            ui.painter().text(
                                egui::pos2(bg_rect.center().x, bg_rect.center().y),
                                egui::Align2::CENTER_CENTER,
                                egui_phosphor::regular::WARNING,
                                egui::FontId::new(10.0, egui::FontFamily::Proportional),
                                status::DIAGNOSTIC_ERROR,
                            );
                            ui.add_space(SPACE_2);
                        }
                        let color = if is_error {
                            status::DIAGNOSTIC_ERROR
                        } else {
                            text::MUTED
                        };
                        let label = ui.label(
                            egui::RichText::new(status.as_str())
                                .size(TextRole::Micro.size())
                                .color(color),
                        );
                        if is_error && self.preview_store.preview.error.is_some() {
                            label.on_hover_text(
                                self.preview_store.preview.error.as_deref().unwrap_or(""),
                            );
                        }
                    }
                    // Right side: scene dimensions
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let dims = &self.document_store.source.document.scene_dimensions;
                        ui.label(
                            egui::RichText::new(format!("{}×{}", dims.width, dims.height))
                                .size(TextRole::Micro.size())
                                .color(text::MUTED),
                        );
                    });
                });
            });

        // Central workspace — edge-to-edge tiles, no outer margin
        // When welcome screen is open, show it instead of the workspace.
        egui::CentralPanel::default()
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
            });

        // Update cursor time from editor position (bi-directional sync)
        self.ui_store.cursor_time_s =
            self.document_store.source.editor.cursor_line.and_then(|line| {
                self.document_store.source.document.timeline_index.time_s_for_line(line)
            });

        self.handle_actions(commands);

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

        // Toast notifications
        let now = Instant::now();
        self.ui_store.toasts.show(ui, now);
    }

    /// Welcome / onboarding screen shown when no document is loaded.
    fn welcome_screen_ui(&mut self, ui: &mut egui::Ui, commands: &mut ActionQueue) {
        let sp = spatial(ui);
        let avail = ui.available_rect_before_wrap();
        ui.painter().rect_filled(avail, 0.0, surface::BASE);

        ui.vertical_centered(|ui| {
            ui.add_space(avail.height() * WELCOME_TOP_OFFSET_FRAC);

            // ── Centered card ──
            egui::Frame::new()
                .fill(surface::SURFACE)
                .stroke(Stroke::new(STROKE_WIDTH, border::DEFAULT))
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
                            surface::WIDGET,
                        );
                        ui.painter().text(
                            icon_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            egui_phosphor::regular::FILM_STRIP,
                            egui::FontId::proportional(28.0), // 28px welcome icon: no TextRole
                            accent::PRIMARY,
                        );
                        ui.add_space(sp.base.space_5 * 1.5);

                        // Title
                        ui.label(
                            egui::RichText::new("Welcome to Animatix")
                                .size(27.0) // 27px welcome title: no TextRole
                                .color(text::PRIMARY)
                                .strong(),
                        );
                        ui.add_space(sp.base.space_2);

                        // Subtitle
                        ui.label(
                            egui::RichText::new("Layout-first animation for creative coders")
                                .size(TextRole::Body.size())
                                .color(text::SECONDARY),
                        );
                        ui.add_space(sp.base.space_5 * 2.5);

                        let btn_w = ui.available_width();

                        // Primary: Create new scene
                        let new_resp = ui.add_sized(
                            egui::vec2(btn_w, sp.welcome.btn_height),
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "{}  Create new scene",
                                    egui_phosphor::regular::PLUS
                                ))
                                .size(TextRole::Body.size())
                                .color(text::PRIMARY),
                            )
                            .fill(accent::PRIMARY)
                            .corner_radius(RADIUS_M),
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
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "{}  Open existing file",
                                    egui_phosphor::regular::FOLDER_OPEN
                                ))
                                .size(TextRole::Body.size())
                                .color(text::PRIMARY),
                            )
                            .fill(surface::WIDGET)
                            .stroke(Stroke::new(STROKE_WIDTH, border::HOVER))
                            .corner_radius(RADIUS_M),
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
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "{}  Open workspace",
                                    egui_phosphor::regular::FOLDER_NOTCH
                                ))
                                .size(TextRole::Body.size())
                                .color(text::PRIMARY),
                            )
                            .fill(surface::WIDGET)
                            .stroke(Stroke::new(STROKE_WIDTH, border::HOVER))
                            .corner_radius(RADIUS_M),
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
        // Create CommandBus alongside ActionQueue for incremental migration to view models.
        // Panels will emit() into the bus once they migrate to immutable view models.
        let mut command_bus = crate::app::command_bus::CommandBus::new();

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
            property_view_mode: &mut self.ui_store.property_view_mode,
            keyframe_view_mode: &mut self.ui_store.keyframe_view_mode,
            keyframe_mode: self.ui_store.keyframe_mode,
            rotation_snap_degrees: self.ui_store.rotation_snap_degrees,
            snap_fps: self.ui_store.snap_fps,
            debug_layout: self.ui_store.view.debug_layout,
            debug_spacing: self.ui_store.view.debug_spacing,
            timeline_focused: &mut self.ui_store.view.timeline_focused,
        };
        tree.ui(&mut behavior, ui);

        // Drain CommandBus into ActionQueue (used once panels migrate to emit()).
        for action in command_bus.drain() {
            commands.push_back(action);
        }
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
        self.document_store.snapshot(label);
    }

    fn sync_active_scene_from_time(&mut self) {
        if let Some(composition) = self.document_store.source.document.composition.as_ref() {
            let (scene, _, _) =
                composition.evaluate(self.preview_store.preview.playback.current_time_s());
            self.document_store.source.document.active_scene = (!scene.is_empty()).then_some(scene);
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
                    .color(text::SECONDARY),
            );
            ui.add_space(SPACE_2);
            ui.add(
                egui::TextEdit::singleline(&mut self.ui_store.workspace_switcher_path)
                    .desired_width(f32::INFINITY)
                    .hint_text("/path/to/workspace"),
            );
            ui.add_space(SPACE_3);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let confirm = ui.add_sized(
                        [80.0, 28.0],
                        egui::Button::new(
                            egui::RichText::new("Switch")
                                .size(TextRole::BodyS.size())
                                .color(text::PRIMARY),
                        )
                        .fill(accent::PRIMARY),
                    );
                    if confirm.clicked() {
                        let path = PathBuf::from(&self.ui_store.workspace_switcher_path);
                        commands.push_back(DocumentCommand::SwitchWorkspace(path).into());
                        body_close = true;
                    }

                    let cancel = ui.add_sized(
                        [80.0, 28.0],
                        egui::Button::new(
                            egui::RichText::new("Cancel")
                                .size(TextRole::BodyS.size())
                                .color(text::SECONDARY),
                        )
                        .fill(surface::WIDGET),
                    );
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
                        .color(text::SECONDARY),
                )
                .selectable(false),
            );
            ui.add_space(SPACE_5);

            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Save button
                    let save = ui.add_sized(
                        [90.0, ROW_L],
                        egui::Button::new(
                            egui::RichText::new(format!(
                                "{}  Save",
                                egui_phosphor::regular::FLOPPY_DISK
                            ))
                            .size(TextRole::BodyS.size())
                            .color(text::PRIMARY),
                        )
                        .fill(accent::PRIMARY),
                    );
                    if save.clicked() {
                        // Save first, then execute pending action
                        let effects = file::handle_save(
                            &mut self.document_store,
                            &mut self.preview_store,
                        );
                        self.apply_effects(effects);
                        let was_close = self.ui_store.unsaved_changes.pending_close;
                        self.execute_unsaved_pending_action();
                        self.ui_store.unsaved_changes.close();
                        if was_close {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        body_close = true;
                    }

                    // Discard button
                    let discard = ui.add_sized(
                        [90.0, ROW_L],
                        egui::Button::new(
                            egui::RichText::new(format!(
                                "{}  Discard",
                                egui_phosphor::regular::TRASH
                            ))
                            .size(TextRole::BodyS.size())
                            .color(text::SECONDARY),
                        )
                        .fill(surface::WIDGET),
                    );
                    if discard.clicked() {
                        // Mark document as no longer dirty, then execute pending
                        self.document_store.source.document.is_dirty = false;
                        let was_close = self.ui_store.unsaved_changes.pending_close;
                        self.execute_unsaved_pending_action();
                        self.ui_store.unsaved_changes.close();
                        if was_close {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        body_close = true;
                    }

                    // Cancel button
                    let cancel = ui.add_sized(
                        [90.0, ROW_L],
                        egui::Button::new(
                            egui::RichText::new("Cancel")
                                .size(TextRole::BodyS.size())
                                .color(text::SECONDARY),
                        )
                        .fill(surface::WIDGET),
                    );
                    if cancel.clicked() {
                        self.ui_store.unsaved_changes.close();
                        body_close = true;
                    }
                });
            });

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
