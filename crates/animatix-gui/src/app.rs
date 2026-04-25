mod file_tree;
mod persistence;
mod preview;
mod runtime;
mod workspace;

use crate::document::{DocumentSession, default_file_path, timeline_keyframe_times_s};
use crate::editor::EditorBuffer;
use crate::preview_surface::PreviewSurface;
use animatix::diagnostics::{
    Diagnostic, DiagnosticCode, DiagnosticPhase, diagnostics_phase_summary,
    diagnostics_summary_by_phase, format_diagnostic,
};
use animatix::timeline::SceneDimensions;
use animatix::timeline::actions::get_action_signatures;
use directories::ProjectDirs;
use egui::{Align, Color32, Pos2, Rect, RichText, Stroke, Vec2};
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};
use egui_wgpu_backend::{RenderPass, ScreenDescriptor};
use egui_winit::State as EguiWinitState;
use file_tree::{build_file_tree, workspace_root_for};
use persistence::{default_dock_state, load_workspace_persistence, persistence_path};
use preview::{fit_preview, paint_timeline_scrubber};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};
use workspace::{UiActions, WorkspaceViewer};

const INITIAL_WINDOW_SIZE: (f64, f64) = (1440.0, 960.0);
const DEFAULT_PREVIEW_SIZE: SceneDimensions = SceneDimensions {
    width: 1920,
    height: 1080,
};
const REBUILD_DEBOUNCE: Duration = Duration::from_millis(150);
const MAX_TREE_DEPTH: usize = 4;
const MAX_TREE_ENTRIES: usize = 200;
const EXPLORER_INDENT_PX: f32 = 10.0;
const TIMELINE_HEIGHT: f32 = 36.0;
const PREVIEW_TRANSPORT_HEIGHT: f32 = 84.0;
const PREVIEW_NON_CANVAS_HEIGHT: f32 = 170.0;
const PREVIEW_MAX_HEIGHT_RATIO: f32 = 0.62;

pub use runtime::run_gui;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum WorkspaceTab {
    Explorer,
    Editor,
    Preview,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkspacePersistence {
    dock_state: DockState<WorkspaceTab>,
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
        }
    }

    fn clamp_time(&mut self) {
        let max_duration = self.duration_s.max(0.1);
        self.current_time_s = self.current_time_s.clamp(0.0, max_duration);
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
    dock_state: DockState<WorkspaceTab>,
    preview: PreviewPaneState,
    preview_dirty: bool,
    pending_rebuild_at: Option<Instant>,
    last_frame_at: Instant,
    persistence_path: PathBuf,
}

impl GuiShell {
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
        let dock_state =
            load_workspace_persistence(&persistence_path).unwrap_or_else(default_dock_state);
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
            dock_state,
            preview,
            preview_dirty: true,
            pending_rebuild_at: None,
            last_frame_at: Instant::now(),
            persistence_path,
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

        if self.preview.is_playing {
            self.preview.tick(delta);
            self.preview_dirty = true;
        }

        if let Some(deadline) = self.pending_rebuild_at
            && now >= deadline
        {
            self.pending_rebuild_at = None;
            let _ = self.rebuild();
        }
    }

    fn ui(&mut self, ctx: &egui::Context, preview_texture_id: Option<egui::TextureId>) {
        let mut actions = UiActions::default();

        egui::TopBottomPanel::top("toolbar")
            .resizable(false)
            .show(ctx, |ui| self.toolbar_ui(ui, &mut actions));
        egui::TopBottomPanel::bottom("status_bar")
            .resizable(false)
            .show(ctx, |ui| self.status_bar_ui(ui));
        egui::CentralPanel::default().show(ctx, |ui| {
            self.workspace_ui(ui, preview_texture_id, &mut actions);
        });

        self.handle_actions(actions);
    }

    fn toolbar_ui(&mut self, ui: &mut egui::Ui, actions: &mut UiActions) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Animatix").strong().size(18.0));
            ui.separator();
            ui.label(
                RichText::new(
                    self.document
                        .file_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Untitled"),
                )
                .small()
                .weak(),
            );

            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                action_button(
                    ui,
                    if self.preview.is_playing {
                        "Pause"
                    } else {
                        "Play"
                    },
                    true,
                    || {
                        actions.toggle_playback = true;
                    },
                );
                action_button(ui, "Rebuild", false, || {
                    actions.rebuild = true;
                });
                action_button(ui, "Save", true, || {
                    actions.save = true;
                });
                ui.add_space(8.0);
                if self.document.is_dirty {
                    badge(
                        ui,
                        "Modified",
                        Color32::from_rgb(120, 74, 26),
                        Color32::from_rgb(255, 217, 153),
                    );
                } else {
                    badge(
                        ui,
                        "Saved",
                        Color32::from_rgb(32, 84, 54),
                        Color32::from_rgb(188, 247, 214),
                    );
                }
            });
        });
        ui.add_space(6.0);
    }

    fn status_bar_ui(&self, ui: &mut egui::Ui) {
        let diagnostics = self.combined_diagnostics();

        ui.horizontal(|ui| {
            ui.label(RichText::new(self.document.file_path.display().to_string()).monospace());
            ui.separator();
            ui.label(if self.document.is_dirty {
                "Dirty"
            } else {
                "Saved"
            });
            ui.separator();
            ui.label(format!(
                "{:.2}s / {:.2}s",
                self.preview.current_time_s, self.preview.duration_s
            ));
            ui.separator();
            ui.label(&self.preview.status);
            if !diagnostics.is_empty() {
                ui.separator();
                ui.colored_label(
                    diagnostics_summary_color(&diagnostics),
                    diagnostics_phase_summary(&diagnostics),
                );
            }
            if let Some(error) = &self.preview.error {
                ui.separator();
                ui.colored_label(Color32::from_rgb(255, 136, 136), error);
            }
        });
    }

    fn workspace_ui(
        &mut self,
        ui: &mut egui::Ui,
        preview_texture_id: Option<egui::TextureId>,
        actions: &mut UiActions,
    ) {
        let diagnostics = self.combined_diagnostics();

        let scene_dimensions = self.document.scene_dimensions;
        let mut viewer = WorkspaceViewer {
            current_file: &self.document.file_path,
            workspace_root: &self.workspace_root,
            expanded_dirs: &mut self.expanded_dirs,
            file_tree: &self.file_tree,
            timeline_markers: self
                .document
                .timeline
                .as_ref()
                .map(timeline_keyframe_times_s)
                .unwrap_or_default(),
            editor: &mut self.editor,
            preview: &mut self.preview,
            diagnostics: &diagnostics,
            preview_texture_id,
            actions,
            source_dirty: &mut self.document.source_text,
            scene_dimensions,
        };

        DockArea::new(&mut self.dock_state)
            .style(Style::from_egui(ui.style().as_ref()))
            .show_inside(ui, &mut viewer);
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
        if actions.save {
            let _ = self.save();
        }
        if actions.reload {
            let _ = self.reload();
        }
        if actions.rebuild {
            let _ = self.rebuild();
        }
        if actions.toggle_playback {
            self.preview.toggle_playback();
            self.preview_dirty = true;
        }
        if let Some(next_time) = actions.scrub_to {
            self.preview.current_time_s = next_time;
            self.preview.clamp_time();
            self.preview.is_playing = false;
            self.preview_dirty = true;
        }
        if actions.editor_changed {
            self.document
                .set_source_text(self.editor.text().to_string());
            self.pending_rebuild_at = Some(Instant::now() + REBUILD_DEBOUNCE);
            self.preview.status = "Editing source • rebuild scheduled".to_string();
        }
        if actions.request_repaint {
            self.preview_dirty = true;
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
        } else {
            self.preview.clamp_time();
        }
        if stop_playback {
            self.preview.is_playing = false;
        }
        self.render_diagnostics.clear();
        self.preview.status = status;
        self.preview.error = None;
        self.preview_dirty = true;
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

    fn save_persistence(&self) {
        if let Some(parent) = self.persistence_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let persistence = WorkspacePersistence {
            dock_state: self.dock_state.clone(),
        };
        if let Ok(serialized) =
            ron::ser::to_string_pretty(&persistence, ron::ser::PrettyConfig::default())
        {
            let _ = fs::write(&self.persistence_path, serialized);
        }
    }
}

fn action_button(ui: &mut egui::Ui, label: &str, primary: bool, on_click: impl FnOnce()) {
    let button = if primary {
        egui::Button::new(label).fill(Color32::from_rgb(84, 110, 255))
    } else {
        egui::Button::new(label)
    };

    if ui.add(button).clicked() {
        on_click();
    }
}

fn badge(ui: &mut egui::Ui, label: &str, fill: Color32, text: Color32) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(label).color(text).strong());
        });
}

fn diagnostics_summary_color(diagnostics: &[Diagnostic]) -> Color32 {
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == animatix::diagnostics::DiagnosticSeverity::Error)
    {
        Color32::from_rgb(255, 136, 136)
    } else {
        Color32::from_rgb(255, 214, 102)
    }
}

fn has_source_load_failure(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.phase == animatix::diagnostics::DiagnosticPhase::Parse
            && diagnostic.severity == animatix::diagnostics::DiagnosticSeverity::Error
            && diagnostic.code == animatix::diagnostics::DiagnosticCode::SourceLoadFailure
    })
}

fn primary_diagnostic_phase(diagnostics: &[Diagnostic]) -> Option<DiagnosticPhase> {
    let summaries = diagnostics_summary_by_phase(diagnostics);

    summaries
        .iter()
        .find(|summary| summary.errors > 0)
        .or_else(|| summaries.first())
        .map(|summary| summary.phase)
}

fn diagnostics_banner_message(diagnostics: &[Diagnostic]) -> Option<&'static str> {
    match primary_diagnostic_phase(diagnostics) {
        Some(DiagnosticPhase::Parse)
            if diagnostics.iter().any(|diagnostic| {
                diagnostic.phase == DiagnosticPhase::Parse
                    && diagnostic.severity == animatix::diagnostics::DiagnosticSeverity::Error
            }) =>
        {
            Some("Parse errors are blocking rebuild. Fix parse issues first.")
        }
        Some(DiagnosticPhase::Parse) => {
            Some("Parse diagnostics need attention before trusting later build feedback.")
        }
        Some(DiagnosticPhase::Build) => {
            Some("Build diagnostics reflect runtime contract issues in the current scene.")
        }
        Some(DiagnosticPhase::Render) => {
            Some("Render diagnostics affect preview output rather than source parsing.")
        }
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GuiShell, WorkspaceTab, default_dock_state, diagnostics_banner_message,
        diagnostics_summary_color, fit_preview, has_source_load_failure, preview,
        primary_diagnostic_phase,
    };
    use animatix::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
    use animatix::timeline::SceneDimensions;
    use egui::Color32;
    use egui::Vec2;
    use std::path::PathBuf;

    #[test]
    fn default_workspace_has_three_tabs() {
        let dock_state = default_dock_state();
        let tabs: Vec<_> = dock_state.iter_all_tabs().map(|(_, tab)| *tab).collect();
        assert_eq!(tabs.len(), 3);
        assert!(tabs.contains(&WorkspaceTab::Explorer));
        assert!(tabs.contains(&WorkspaceTab::Editor));
        assert!(tabs.contains(&WorkspaceTab::Preview));
    }

    #[test]
    fn preview_fit_preserves_aspect_ratio() {
        let fitted = fit_preview(
            SceneDimensions {
                width: 1920,
                height: 1080,
            },
            Vec2::new(400.0, 400.0),
        );
        assert!((fitted.x / fitted.y - 16.0 / 9.0).abs() < 0.001);
    }

    #[test]
    fn preview_fit_uses_scene_dimensions_aspect_ratio() {
        let fitted = fit_preview(
            SceneDimensions {
                width: 1000,
                height: 1000,
            },
            Vec2::new(400.0, 200.0),
        );
        assert!((fitted.x - fitted.y).abs() < 0.001);
    }

    #[test]
    fn timeline_fraction_clamps_to_bounds() {
        assert_eq!(preview::timeline_fraction(-1.0, 10.0), 0.0);
        assert_eq!(preview::timeline_fraction(5.0, 10.0), 0.5);
        assert_eq!(preview::timeline_fraction(20.0, 10.0), 1.0);
    }

    #[test]
    fn pointer_position_maps_to_scrub_time() {
        let rect = egui::Rect::from_min_max(egui::pos2(10.0, 0.0), egui::pos2(210.0, 20.0));
        assert_eq!(preview::time_from_pointer_x(rect, 10.0, 8.0), 0.0);
        assert!((preview::time_from_pointer_x(rect, 110.0, 8.0) - 4.0).abs() < 0.001);
        assert_eq!(preview::time_from_pointer_x(rect, 210.0, 8.0), 8.0);
    }

    #[test]
    fn diagnostics_summary_color_turns_red_when_errors_exist() {
        let diagnostics = vec![Diagnostic::error(
            DiagnosticCode::SourceLoadFailure,
            DiagnosticPhase::Parse,
            "parse failed",
        )];

        assert_eq!(
            diagnostics_summary_color(&diagnostics),
            Color32::from_rgb(255, 136, 136)
        );
    }

    #[test]
    fn has_source_load_failure_detects_blocking_parse_entries() {
        let diagnostics = vec![Diagnostic::error(
            DiagnosticCode::SourceLoadFailure,
            DiagnosticPhase::Parse,
            "parse failed",
        )];

        assert!(has_source_load_failure(&diagnostics));
    }

    #[test]
    fn primary_diagnostic_phase_prefers_parse_then_build_then_render() {
        let diagnostics = vec![
            Diagnostic::warning(
                DiagnosticCode::MediaLoadFailure,
                DiagnosticPhase::Render,
                "render warning",
            ),
            Diagnostic::error(
                DiagnosticCode::UnknownAction,
                DiagnosticPhase::Build,
                "build error",
            ),
            Diagnostic::error(
                DiagnosticCode::SourceLoadFailure,
                DiagnosticPhase::Parse,
                "parse error",
            ),
        ];

        assert_eq!(
            primary_diagnostic_phase(&diagnostics),
            Some(DiagnosticPhase::Parse)
        );
    }

    #[test]
    fn diagnostics_banner_message_calls_out_parse_first() {
        let diagnostics = vec![Diagnostic::error(
            DiagnosticCode::SourceLoadFailure,
            DiagnosticPhase::Parse,
            "parse error",
        )];

        assert_eq!(
            diagnostics_banner_message(&diagnostics),
            Some("Parse errors are blocking rebuild. Fix parse issues first.")
        );
    }

    #[test]
    fn diagnostics_banner_message_calls_out_build_when_no_parse_errors() {
        let diagnostics = vec![Diagnostic::warning(
            DiagnosticCode::UnknownAction,
            DiagnosticPhase::Build,
            "build warning",
        )];

        assert_eq!(
            diagnostics_banner_message(&diagnostics),
            Some("Build diagnostics reflect runtime contract issues in the current scene.")
        );
    }

    #[test]
    fn diagnostics_banner_message_does_not_claim_parse_warnings_block_rebuild() {
        let diagnostics = vec![Diagnostic::warning(
            DiagnosticCode::InvalidConfigValue,
            DiagnosticPhase::Parse,
            "parse warning",
        )];

        assert_eq!(
            diagnostics_banner_message(&diagnostics),
            Some("Parse diagnostics need attention before trusting later build feedback.")
        );
    }

    #[test]
    fn diagnostics_banner_message_calls_out_render_diagnostics() {
        let diagnostics = vec![Diagnostic::error(
            DiagnosticCode::RenderFailure,
            DiagnosticPhase::Render,
            "render failed",
        )];

        assert_eq!(
            diagnostics_banner_message(&diagnostics),
            Some("Render diagnostics affect preview output rather than source parsing.")
        );
    }

    #[test]
    fn clear_render_error_removes_render_failure_state() {
        let mut shell = GuiShell::load(PathBuf::from("examples/showcase.amx"));
        shell.set_render_error("preview failed".to_string());

        shell.clear_render_error("Live preview restored".to_string());

        assert!(shell.render_diagnostics.is_empty());
        assert!(shell.preview.error.is_none());
        assert_eq!(shell.preview.status, "Live preview restored");
    }

    #[test]
    fn clear_render_error_preserves_non_render_preview_failures() {
        let mut shell = GuiShell::load(PathBuf::from("examples/showcase.amx"));
        shell.set_status(
            "Open failed • missing.amx".to_string(),
            Some("missing file".to_string()),
        );

        shell.clear_render_error("Live preview restored".to_string());

        assert!(shell.render_diagnostics.is_empty());
        assert_eq!(shell.preview.error.as_deref(), Some("missing file"));
        assert_eq!(shell.preview.status, "Open failed • missing.amx");
    }

    #[test]
    fn clear_render_error_preserves_newer_non_render_failure_when_render_diagnostic_is_stale() {
        let mut shell = GuiShell::load(PathBuf::from("examples/showcase.amx"));
        shell.set_render_error("preview failed".to_string());
        shell.set_status(
            "Rebuild blocked • parse/load error".to_string(),
            Some("duplicate export".to_string()),
        );

        shell.clear_render_error("Live preview restored".to_string());

        assert!(shell.render_diagnostics.is_empty());
        assert_eq!(shell.preview.error.as_deref(), Some("duplicate export"));
        assert_eq!(shell.preview.status, "Rebuild blocked • parse/load error");
    }
}
