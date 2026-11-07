mod actions;
pub(crate) mod audio;
pub(crate) mod commands;
pub(crate) mod components;
pub mod design_tokens;
mod dialogs;
pub(crate) mod document;
mod document_controller;
mod drawers;
mod file_tree;
pub(crate) mod handlers;
pub(crate) mod icons;
pub(crate) mod insertion;
pub(crate) mod interaction;
mod layout_preset;
pub(crate) mod panels;
mod perf_log;
mod persistence;
mod playback;
pub(crate) mod preview;
mod preview_pane;
pub(crate) mod primitives;
mod runtime;
pub(crate) mod shell;
pub(crate) mod stores;
#[cfg(test)]
pub(crate) mod test_support;
mod utils;

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use animatix::timeline::SceneDimensions;
use animatix_syntax::diagnostics::{
    Diagnostic, DiagnosticCode, DiagnosticPhase, diagnostics_phase_summary,
};
use directories::ProjectDirs;
use egui::{Color32, Vec2};
use file_tree::{build_file_tree, workspace_root_for};
use persistence::{
    SettingsPersistence, WorkspacePersistence, default_tree, load_workspace_persistence,
    persistence_path,
};
#[cfg(test)]
use preview::fit_preview;
use serde::{Deserialize, Serialize};

use crate::app::commands::{ActionQueue, Command, CommandQueue, CommandSender};
use crate::app::components::button::Button;
use crate::app::components::toast::Toast;
use crate::app::components::{Alert, AlertLevel, text_tooltip};
use crate::app::design_tokens::spatial::{RADIUS_S, SPACE_2, SPACE_4};
use crate::app::design_tokens::typography::TextRole;
use crate::app::document::load_worker::{LoadToken, LoadWorker};
use crate::app::document::plugins::DocumentPluginManager;
use crate::app::document::rebuild::RebuildWorker;
use crate::app::document::{DocumentSession, default_file_path};
use crate::app::interaction::keyboard::ShortcutRegistry;
use crate::app::shell::insertion_palette::InsertionPalette;
use crate::app::stores::*;
use crate::app::utils::*;
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
    pub presentation_mode: bool,
    pub pause_points_s: Vec<f64>,
    pub paused_at_step: bool,
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

struct GuiShell {
    document_store: DocumentStore,
    workspace_store: WorkspaceStore,
    preview_store: PreviewStore,
    ui_store: UiStore,
    export_store: ExportStore,
    rebuild_worker: RebuildWorker,
    load_worker: LoadWorker,
    pending_open: Option<LoadToken>,
    plugin_manager: DocumentPluginManager,
    external_commands: CommandQueue,
    insertion_palette: InsertionPalette,
    shortcut_registry: ShortcutRegistry,
    pub(crate) window_size: [f32; 2],
    pub(crate) window_maximized: bool,
}
#[cfg(test)]
mod tests;
