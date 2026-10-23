mod actions;
pub(crate) mod audio;
pub(crate) mod command_handlers;
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
pub(crate) mod panels;
mod perf_log;
mod persistence;
pub(crate) mod preview;
pub(crate) mod primitives;
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
use egui::{Color32, Vec2};
use file_tree::{build_file_tree, workspace_root_for};
use persistence::{
    SettingsPersistence, WorkspacePersistence, default_tree, load_workspace_persistence,
    persistence_path,
};
#[cfg(test)]
use preview::fit_preview;
use serde::{Deserialize, Serialize};

use crate::app::commands::{ActionQueue, CommandQueue, CommandSender, UndoLabel, ViewCommand};
use crate::app::components::button::Button;
use crate::app::components::toast::Toast;
use crate::app::components::{Alert, AlertLevel, text_tooltip};
use crate::app::design_tokens::spatial::{RADIUS_S, SPACE_2, SPACE_4};
use crate::app::design_tokens::typography::TextRole;
use crate::app::document::plugins::DocumentPluginManager;
use crate::app::document::rebuild::RebuildWorker;
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
    pub presentation_mode: bool,
    pub pause_points_s: Vec<f64>,
    pub paused_at_step: bool,
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

        let dt = delta.as_secs_f64() * self.playback_speed as f64 * self.ping_pong_direction as f64;
        let old_time = self.current_time_s;
        let next_time = old_time + dt;

        if self.presentation_mode && dt > 0.0 {
            if let Some(&p) = self
                .pause_points_s
                .iter()
                .find(|&&p| p > old_time + 0.0001 && p <= next_time + 0.0001)
            {
                self.current_time_s = p;
                self.is_playing = false;
                self.paused_at_step = true;
                return;
            }
        }

        self.current_time_s = next_time;

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

    pub(crate) fn presentation_step_next(&mut self) {
        if let Some(&next_p) =
            self.pause_points_s.iter().find(|&&p| p > self.current_time_s + 0.001)
        {
            if self.is_playing {
                self.current_time_s = next_p;
                self.is_playing = false;
                self.paused_at_step = true;
            } else {
                self.is_playing = true;
                self.paused_at_step = false;
            }
        } else if self.current_time_s >= self.duration_s {
            self.current_time_s = 0.0;
            self.is_playing = true;
            self.paused_at_step = false;
        } else {
            self.is_playing = true;
            self.paused_at_step = false;
        }
    }

    pub(crate) fn presentation_step_prev(&mut self) {
        if let Some(&prev_p) =
            self.pause_points_s.iter().rev().find(|&&p| p < self.current_time_s - 0.001)
        {
            self.current_time_s = prev_p;
            self.is_playing = false;
            self.paused_at_step = true;
        } else {
            self.current_time_s = 0.0;
            self.is_playing = false;
            self.paused_at_step = true;
        }
    }

    pub(crate) fn toggle_presentation_mode(&mut self) {
        self.presentation_mode = !self.presentation_mode;
        if self.presentation_mode {
            self.is_playing = false;
            self.paused_at_step = true;
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
                presentation_mode: false,
                pause_points_s: Vec::new(),
                paused_at_step: false,
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

#[cfg(test)]
mod tests;
