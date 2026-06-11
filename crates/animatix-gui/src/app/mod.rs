
mod actions;
pub(crate) mod command_handlers;
pub(crate) mod commands;
pub(crate) mod handlers;
pub(crate) mod components;
mod document_controller;
mod file_tree;
pub(crate) mod icons;
pub(crate) mod panels;
mod persistence;
pub(crate) mod preview;
mod runtime;
pub(crate) mod shell;
pub(crate) mod stores;
pub mod design_tokens;
mod utils;
pub(crate) mod insertion;
pub(crate) mod document;
pub(crate) mod audio;
pub(crate) mod command_bus;
pub(crate) mod services;

use crate::document::{DocumentSession, default_file_path};
use crate::hot_reload::{HotReloader, ReloadStatus};
use crate::editor::EditorBuffer;
use crate::preview_surface::PreviewSurface;
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase, diagnostics_phase_summary};
use animatix::timeline::SceneDimensions;
use directories::ProjectDirs;
use egui::{Color32, Stroke, Vec2};
use egui_tiles::Tree;
use file_tree::{build_file_tree, workspace_root_for};
use persistence::{default_tree, load_workspace_persistence, persistence_path};
use crate::app::design_tokens::*;
#[cfg(test)]
use preview::fit_preview;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use crate::app::commands::{ActionQueue, Command, Effect, ShellAction};
use crate::app::shell::insertion_palette::{InsertionPalette, PaletteMode};
use crate::app::utils::*;
use crate::app::stores::*;
use crate::app::document::rebuild::RebuildWorker;

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

#[derive(Debug, Serialize, Deserialize)]
struct WorkspacePersistence {
    tree: Tree<WorkspaceTab>,
}

#[derive(Debug, Clone)]
pub(crate) struct FileTreeEntry {
    path: PathBuf,
    name: String,
    depth: usize,
    is_dir: bool,
}

/// Playback controller: time, duration, play/pause, speed, loop region.
#[derive(Debug, Clone)]
pub(crate) struct PlaybackController {
    current_time_s: f64,
    pub duration_s: f64,
    pub is_playing: bool,
    pub playback_speed: f32,
    pub loop_start_s: Option<f64>,
    pub loop_end_s: Option<f64>,
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

        self.current_time_s += delta.as_secs_f64() * self.playback_speed as f64;

        // Loop region: if A and B are set and we've reached B, jump back to A.
        if let (Some(start), Some(end)) = (self.loop_start_s, self.loop_end_s) {
            if end > start && self.current_time_s >= end {
                self.current_time_s = start;
                // Looping takes priority over end-of-timeline stop.
                return;
            }
        }

        if self.current_time_s >= self.duration_s {
            self.current_time_s = self.duration_s;
            self.is_playing = false;
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

pub(crate) struct PreviewPaneState {
    pub playback: PlaybackController,
    pub viewport: ViewportState,
    pub guides: GuideState,
    pub snap: SnapState,
    pub status: String,
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
            },
            viewport: ViewportState {
                preview_zoom: 1.0,
                preview_pan: Vec2::new(dimensions.width as f32 / 2.0, dimensions.height as f32 / 2.0),
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
}

struct GuiShell {
    document_store: DocumentStore,
    workspace_store: WorkspaceStore,
    preview_store: PreviewStore,
    ui_store: UiStore,
    export_store: ExportStore,
    rebuild_worker: RebuildWorker,
    insertion_palette: InsertionPalette,
}

impl GuiShell {
    fn check_hot_reload(&mut self, app_time: Instant) {
        if let Some(ref mut reloader) = self.workspace_store.hot_reloader {
            match reloader.update(app_time) {
                ReloadStatus::ShouldReload { path: _ } => {
                    // LiveDocument: editor is the source of truth. If the editor
                    // has unsaved changes, do NOT silently overwrite them.
                    if self.document_store.source.document.is_dirty {
                        self.preview_store.preview.status = "External file changed • reload blocked (unsaved edits)".to_string();
                        return;
                    }
                    if let Err(err) = self.document_store.source.document.reload_from_disk() {
                        self.preview_store.preview.error = Some(err.to_string());
                        self.preview_store.preview.status = "Hot reload failed".to_string();
                    } else {
                        self.document_store.source.invalidate_cache();
                        self.document_store.source.editor
                            .set_document(&self.document_store.source.document.file_path, self.document_store.source.document.source_text.clone());
                        self.workspace_store.last_reload_time = Some(app_time);
                        self.preview_store.preview.status = "File reloaded".to_string();
                        self.preview_store.preview.error = None;
                        self.document_store.publish_rebuild_result(
                            self.document_store.source.document.last_rebuild_error.is_none()
                        );
                    }
                }
                ReloadStatus::NoChange => {}
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
                }
                Err(error) => {
                    // Persisted file missing/deleted — fall back to welcome
                    let doc = DocumentSession::from_error(initial_path.clone());
                    (doc, None, Some(error.to_string()), true)
                }
            }
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
        preview.error = error.clone();

        let editor = EditorBuffer::new(&document.file_path, document.source_text.clone());

        let mut ui_store = UiStore::new(tree);
        ui_store.view.welcome_open = is_welcome;

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
        };
        if !is_welcome {
            shell.document_store.publish_rebuild_result(
                error.is_none() && !has_source_load_failure(&shell.document_store.source.document.diagnostics)
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
                if let Some(line) = self.document_store.source.document.find_keyframe_line_at(self.preview_store.preview.playback.current_time_s()) {
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
            if self.preview_store.in_flight_rebuild
                .map_or(true, |token| response.token == token)
            {
                let effects = crate::app::handlers::file::handle_rebuild_response(
                    &mut self.document_store,
                    &mut self.preview_store,
                    &mut self.ui_store,
                    response,
                );
                self.apply_effects(effects);
                self.preview_store.in_flight_rebuild = None;
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, preview_texture_id: Option<egui::TextureId>) {
        let mut commands: ActionQueue = ActionQueue::default();
        commands.append(&mut self.ui_store.pending_actions);

        // Global keyboard shortcuts (non-tool-mode shortcuts only;
        // tool mode switching is handled in runtime.rs with proper
        // wants_keyboard checks to avoid conflicts with text input).
        let wants_keyboard = ui.ctx().egui_wants_keyboard_input();
        ui.input(|i| {
            if !wants_keyboard && i.key_pressed(egui::Key::Y) && !i.modifiers.command {
                commands.push_back(ShellAction::Command(Command::ToggleEditorSync));
            }
            if !wants_keyboard && !i.modifiers.command {
                if i.key_pressed(egui::Key::A) && !self.ui_store.selection.selected_actors.is_empty() {
                    self.insertion_palette.open(PaletteMode::Actions);
                }
                if i.key_pressed(egui::Key::Slash) {
                    self.insertion_palette.open(PaletteMode::Universal);
                }
            }
            // Copy selected actors (Ctrl+C)
            if i.modifiers.command && i.key_pressed(egui::Key::C)
                && !self.ui_store.selection.selected_actors.is_empty() {
                    self.copy_selected_actors();
                }
            // Paste actors (Ctrl+V)
            if i.modifiers.command && i.key_pressed(egui::Key::V)
                && !self.ui_store.clipboard.clipboard_actors.is_empty() {
                    commands.push_back(ShellAction::Command(Command::PasteActors));
                }
        });

        // Compact toolbar — hidden during onboarding so no grid/zoom controls clutter
        // the welcome screen.
        if !self.ui_store.view.welcome_open {
            egui::Panel::top("toolbar")
                .resizable(false)
                .show_inside(ui, |ui| self.toolbar_ui(ui, &mut commands));
        }

        let diagnostics = self.document_store.combined_diagnostics();

        // Diagnostics panel (collapsible)
        if self.ui_store.view.diagnostics_panel_visible && !diagnostics.is_empty() {
            egui::Panel::bottom("diagnostics_panel")
                .resizable(true)
                .default_size(180.0)
                .min_size(80.0)
                .max_size(400.0)
                .show_inside(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if let Some(target) =
                        components::diagnostics::diagnostics_list(
                            ui,
                            &diagnostics,
                            &mut self.ui_store.view.diagnostics_panel_visible,
                        )
                    {
                        self.ui_store.pending_actions.push_back(
                            ShellAction::Command(Command::ScrollToLine(target.line, target.column))
                        );
                    }
                });
        }

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
        self.ui_store.cursor_time_s = self
            .document_store
            .source
            .editor
            .cursor_line
            .and_then(|line| self.document_store.source.document.timeline_index.time_s_for_line(line));

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

        // Toast notifications
        let now = Instant::now();
        self.ui_store.toasts.show(ui, now);
    }

    /// Welcome / onboarding screen shown when no document is loaded.
    fn welcome_screen_ui(&mut self, ui: &mut egui::Ui, commands: &mut ActionQueue) {
        let avail = ui.available_rect_before_wrap();
        ui.painter().rect_filled(avail, 0.0, BG_BASE);

        ui.vertical_centered(|ui| {
            ui.add_space(avail.height() * WELCOME_TOP_OFFSET_FRAC);

            // ── Centered card ──
            egui::Frame::new()
                .fill(BG_SURFACE)
                .stroke(Stroke::new(STROKE_WIDTH, BORDER))
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
                            BG_WIDGET,
                        );
                        ui.painter().text(
                            icon_rect.center(),
                            egui::Align2::CENTER_CENTER,
                            egui_phosphor::regular::FILM_STRIP,
                            egui::FontId::proportional(28.0),
                            ACCENT_BLUE,
                        );
                        ui.add_space(SPACE_XL * 1.5);

                        // Title
                        ui.label(
                            egui::RichText::new("Welcome to Animatix")
                                .size(FONT_SIZE_XL * 1.5)
                                .color(TEXT_PRIMARY)
                                .strong(),
                        );
                        ui.add_space(SPACE_S);

                        // Subtitle
                        ui.label(
                            egui::RichText::new("Layout-first animation for creative coders")
                                .size(FONT_SIZE_M)
                                .color(TEXT_SECONDARY),
                        );
                        ui.add_space(SPACE_XL * 2.5);

                        let btn_w = ui.available_width();

                        // Primary: Create new scene
                        let new_resp = ui.add_sized(
                            egui::vec2(btn_w, WELCOME_BTN_HEIGHT),
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "{}  Create new scene",
                                    egui_phosphor::regular::PLUS
                                ))
                                .size(FONT_SIZE_M)
                                .color(TEXT_PRIMARY),
                            )
                            .fill(ACCENT_BLUE)
                            .corner_radius(RADIUS_M),
                        );
                        if new_resp.clicked() {
                            let path = default_file_path();
                            std::fs::write(&path, "#0s\n").ok();
                            commands.push_back(ShellAction::Command(Command::OpenFile(path)));
                        }

                        ui.add_space(SPACE_M);

                        // Secondary: open existing file
                        let open_resp = ui.add_sized(
                            egui::vec2(btn_w, WELCOME_BTN_HEIGHT),
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "{}  Open existing file",
                                    egui_phosphor::regular::FOLDER_OPEN
                                ))
                                .size(FONT_SIZE_M)
                                .color(TEXT_PRIMARY),
                            )
                            .fill(BG_WIDGET)
                            .stroke(Stroke::new(STROKE_WIDTH, BORDER_HOVER))
                            .corner_radius(RADIUS_M),
                        );
                        if open_resp.clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Animatix", &["amx"])
                                .pick_file()
                            {
                                commands.push_back(ShellAction::Command(Command::OpenFile(path)));
                            }
                        }

                        ui.add_space(SPACE_M);

                        // Tertiary: open workspace
                        let ws_resp = ui.add_sized(
                            egui::vec2(btn_w, WELCOME_BTN_HEIGHT),
                            egui::Button::new(
                                egui::RichText::new(format!(
                                    "{}  Open workspace",
                                    egui_phosphor::regular::FOLDER_NOTCH
                                ))
                                .size(FONT_SIZE_M)
                                .color(TEXT_PRIMARY),
                            )
                            .fill(BG_WIDGET)
                            .stroke(Stroke::new(STROKE_WIDTH, BORDER_HOVER))
                            .corner_radius(RADIUS_M),
                        );
                        if ws_resp.clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                commands.push_back(ShellAction::Command(Command::SwitchWorkspace(path)));
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
        let tree = &mut self.ui_store.view.tree;
        let mut behavior = panels::behavior::WorkspaceBehavior {
            document_store: &mut self.document_store,
            workspace_store: &mut self.workspace_store,
            preview_store: &mut self.preview_store,
            commands,
            preview_texture_id,
            collapsed_actors: &mut self.ui_store.view.collapsed_actors,
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
        };
        tree.ui(&mut behavior, ui);
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
                }
                Effect::Status(status) => {
                    self.preview_store.preview.status = status;
                }
                Effect::Repaint => {
                    self.preview_store.preview_dirty = true;
                }
                Effect::EditorScroll(line) => {
                    self.document_store.source.editor.scroll_to_line(line);
                }
                Effect::EditorHighlight(line) => {
                    self.document_store.source.editor.set_highlighted_line(Some(line));
                }
                Effect::RebuildScheduled => {
                    // The status has already been set; the pending rebuild
                    // timer is set directly in the command handler.
                }
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
    fn snapshot(&mut self, command: Command) {
        self.document_store.snapshot(command);
    }

    fn sync_active_scene_from_time(&mut self) {
        if let Some(composition) = self.document_store.source.document.composition.as_ref() {
            let (scene, _, _) = composition.evaluate(self.preview_store.preview.playback.current_time_s());
            self.document_store.source.document.active_scene = (!scene.is_empty()).then_some(scene);
        }
    }

    fn set_status(&mut self, status: String, error: Option<String>) {
        self.preview_store.preview.status = status;
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
        self.ui_store.clipboard.clipboard_actors = self.ui_store.selection.selected_actors.iter().cloned().collect();
        self.preview_store.preview.status = format!("Copied {} actor(s)", count);
    }

    /// Workspace switcher dialog — small centered window for typing a directory path.
    fn workspace_switcher_ui(&mut self, ui: &mut egui::Ui) {
        let screen_rect = ui.ctx().viewport_rect();
        ui.painter().rect_filled(screen_rect, 0.0, overlay_backdrop());

        // Close on Escape or backdrop click
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.ui_store.view.workspace_switcher_open = false;
        }
        let backdrop = ui.interact(screen_rect, ui.id().with("ws_backdrop"), egui::Sense::click());
        if backdrop.clicked() {
            self.ui_store.view.workspace_switcher_open = false;
        }

        let mut commands = ActionQueue::default();
        egui::Window::new("Switch Workspace")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_size([400.0, 140.0])
            .min_size([360.0, 120.0])
            .resizable(false)
            .collapsible(false)
            .title_bar(false)
            .frame(
                egui::Frame::new()
                    .fill(BG_BASE)
                    .stroke(Stroke::new(STROKE_WIDTH, BORDER))
                    .corner_radius(RADIUS_XL)
                    .inner_margin(egui::Margin::same(SPACE_XL as i8)),
            )
            .show(ui.ctx(), |ui| {
                ui.set_min_width(320.0);

                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Switch Workspace")
                            .size(FONT_SIZE_XL)
                            .color(TEXT_PRIMARY),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(egui_phosphor::regular::X).clicked() {
                            self.ui_store.view.workspace_switcher_open = false;
                        }
                    });
                });
                ui.add_space(SPACE_M);
                ui.separator();
                ui.add_space(SPACE_M);

                ui.label(
                    egui::RichText::new("Directory path")
                        .size(FONT_SIZE_S)
                        .color(TEXT_SECONDARY),
                );
                ui.add_space(SPACE_S);
                ui.add(
                    egui::TextEdit::singleline(&mut self.ui_store.workspace_switcher_path)
                        .desired_width(f32::INFINITY)
                        .hint_text("/path/to/workspace"),
                );
                ui.add_space(SPACE_M);

                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let confirm = ui
                            .add_sized(
                                [80.0, 28.0],
                                egui::Button::new(
                                    egui::RichText::new("Switch")
                                        .size(FONT_SIZE_S)
                                        .color(TEXT_PRIMARY),
                                )
                                .fill(ACCENT_BLUE),
                            );
                        if confirm.clicked() {
                            // P0.3: warn if there are unsaved changes before switching workspace
                            if self.document_store.source.is_dirty() {
                                self.preview_store.preview.status =
                                    "Save changes before switching workspace".to_string();
                                self.ui_store.toasts.push(
                                    crate::app::components::toast::Toast::warning(
                                        "Save changes before switching workspace"
                                    )
                                );
                            } else {
                                let path = PathBuf::from(&self.ui_store.workspace_switcher_path);
                                commands.push_back(ShellAction::Command(Command::SwitchWorkspace(path)));
                                self.ui_store.view.workspace_switcher_open = false;
                            }
                        }

                        let cancel = ui
                            .add_sized(
                                [80.0, 28.0],
                                egui::Button::new(
                                    egui::RichText::new("Cancel")
                                        .size(FONT_SIZE_S)
                                        .color(TEXT_SECONDARY),
                                )
                                .fill(BG_WIDGET),
                            );
                        if cancel.clicked() {
                            self.ui_store.view.workspace_switcher_open = false;
                        }
                    });
                });
            });

        for cmd in commands {
            let effects = self.handle_action(cmd);
            self.apply_effects(effects);
        }
    }
}

#[cfg(test)]
mod tests;