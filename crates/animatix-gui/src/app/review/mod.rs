//! Standalone dogfood review mode.
//!
//! `animatix-gui --review dogfood/runs/<slug>` opens an A/B review workspace:
//! live preview, read-only highlighted source, and comments anchored to
//! variant/time/source line. Comments are persisted as `review.json` in the
//! run directory so an agent can consume them after the session.

mod source_viewer;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use animatix::timeline::{DebugRenderOptions, SceneDimensions};
use eframe::egui;
use egui_phosphor::regular::{
    ARROW_LEFT, ARROW_RIGHT, CHAT_CIRCLE_TEXT, CHECK_CIRCLE, PAUSE, PLAY, TRASH,
};
use eparts::widget::UiExt;
use serde::{Deserialize, Serialize};

use super::runtime::{detect_system_dark, install_theme};
use super::*;
use crate::app::components::text_tooltip;
use crate::document::DocumentSession;
use crate::preview_surface::PreviewSurface;
use source_viewer::SourceViewer;

const REVIEW_WINDOW_SIZE: (f64, f64) = (1600.0, 960.0);
const REVIEW_FPS: f32 = 60.0;
const REVIEW_DONE_FILE: &str = "review.done";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReviewMode {
    Single,
    Compare,
}

impl ReviewMode {
    fn label(self) -> &'static str {
        match self {
            Self::Single => "Single",
            Self::Compare => "Compare",
        }
    }
}

/// Public entry point for `animatix-gui --review <run>`.
pub fn run_review(run_path: PathBuf) {
    let viewport = egui::ViewportBuilder::default()
        .with_title("Animatix Dogfood Review")
        .with_inner_size(egui::vec2(REVIEW_WINDOW_SIZE.0 as f32, REVIEW_WINDOW_SIZE.1 as f32));
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    if let Err(error) = eframe::run_native(
        "Animatix Dogfood Review",
        options,
        Box::new(move |cc| {
            let app = ReviewApp::new(cc, run_path)?;
            Ok(Box::new(app))
        }),
    ) {
        tracing::error!("Failed to run dogfood review: {error}");
        std::process::exit(1);
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
enum CommentSeverity {
    Blocker,
    Major,
    Minor,
    Question,
}

impl CommentSeverity {
    fn color(self) -> egui::Color32 {
        match self {
            Self::Blocker => egui::Color32::from_rgb(224, 108, 117),
            Self::Major => egui::Color32::from_rgb(229, 192, 123),
            Self::Minor => egui::Color32::from_rgb(152, 195, 121),
            Self::Question => egui::Color32::from_rgb(97, 175, 239),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct ReviewComment {
    id: String,
    variant: String,
    time_ms: Option<u64>,
    severity: CommentSeverity,
    note: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct ReviewComments {
    comments: Vec<ReviewComment>,
}

struct ReviewVariant {
    id: String,
    label: String,
    path: PathBuf,
    document: DocumentSession,
}

struct ReviewRun {
    name: String,
    path: PathBuf,
    variants: Vec<ReviewVariant>,
}

struct RunLoader;

#[derive(Debug)]
enum LoadError {
    Io(std::io::Error),
    MissingVariants,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "failed to read review run: {error}"),
            Self::MissingVariants => {
                write!(f, "review run needs at least two .amx variants")
            },
        }
    }
}

impl From<std::io::Error> for LoadError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl RunLoader {
    fn load(run_path: &Path) -> Result<ReviewRun, LoadError> {
        let name = run_path.file_name().and_then(|name| name.to_str()).unwrap_or("run").to_string();
        let variants = load_variants(run_path)?;
        if variants.len() < 2 {
            return Err(LoadError::MissingVariants);
        }

        Ok(ReviewRun {
            name,
            path: run_path.to_path_buf(),
            variants,
        })
    }
}

fn load_variants(run_path: &Path) -> Result<Vec<ReviewVariant>, LoadError> {
    let mut files = Vec::new();
    for entry in fs::read_dir(run_path)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("amx") {
            files.push(path);
        }
    }
    files.sort();

    files
        .into_iter()
        .map(|path| {
            let document = DocumentSession::load(path.clone()).map_err(|error| {
                LoadError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{}: {error}", path.display()),
                ))
            })?;
            let id =
                path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("variant").to_string();
            let label = id.to_uppercase();
            Ok(ReviewVariant {
                id,
                label,
                path,
                document,
            })
        })
        .collect()
}

fn load_comments(run_path: &Path) -> Vec<ReviewComment> {
    let comments_path = run_path.join("review.json");
    match fs::read_to_string(&comments_path) {
        Ok(source) => match serde_json::from_str::<ReviewComments>(&source) {
            Ok(parsed) => parsed.comments,
            Err(error) => {
                tracing::warn!("Failed to parse review comments: {error}");
                Vec::new()
            },
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            tracing::warn!("Failed to read review comments: {error}");
            Vec::new()
        },
    }
}

struct CommentUiState {
    open: bool,
    note: String,
    severity: CommentSeverity,
    finished: bool,
    capture_time: bool,
    focus_requested: bool,
}

impl Default for CommentUiState {
    fn default() -> Self {
        Self {
            open: false,
            note: String::new(),
            severity: CommentSeverity::Question,
            finished: false,
            capture_time: false,
            focus_requested: false,
        }
    }
}

struct ReviewApp {
    run: Option<ReviewRun>,
    current_variant: usize,
    mode: ReviewMode,
    playback: PlaybackController,
    preview_surfaces: Vec<PreviewSurface>,
    preview_texture_ids: Vec<Option<egui::TextureId>>,
    comments: Vec<ReviewComment>,
    selected_line: Option<usize>,
    scroll_to_line: bool,
    source_viewer: SourceViewer,
    comment_ui: CommentUiState,
    last_frame_at: Instant,
    preview_dirty: bool,
    error: Option<String>,
}

impl ReviewApp {
    fn new(cc: &eframe::CreationContext<'_>, run_path: PathBuf) -> Result<Self, String> {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .ok_or_else(|| "eframe wgpu render state not available".to_string())?;

        crate::fonts::install_fonts(&cc.egui_ctx);

        let theme = eparts::AppThemeChoice::Dark.resolve(detect_system_dark());
        eparts::set_theme(&cc.egui_ctx, theme);
        install_theme(&cc.egui_ctx, &theme, true);

        let (run, error) = match RunLoader::load(&run_path) {
            Ok(run) => (Some(run), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let comments = run.as_ref().map(|run| load_comments(&run.path)).unwrap_or_default();
        let finished = run
            .as_ref()
            .map(|run| run.path.join(REVIEW_DONE_FILE).exists())
            .unwrap_or(false);

        let initial_duration = run
            .as_ref()
            .and_then(|run| run.variants.first())
            .map(|variant| document_duration(&variant.document))
            .unwrap_or(5.0);

        let device = render_state.device.clone();
        let queue = render_state.queue.clone();
        let variant_count = run.as_ref().map_or(1, |run| run.variants.len());
        let mut preview_surfaces = Vec::with_capacity(variant_count);
        for _ in 0..variant_count {
            preview_surfaces
                .push(PreviewSurface::new(&device, &queue).map_err(|error| error.to_string())?);
        }
        let preview_texture_ids = vec![None; variant_count];

        if let Some(run) = run.as_ref() {
            for (index, variant) in run.variants.iter().enumerate() {
                preview_surfaces[index].set_dimensions(&device, variant.document.scene_dimensions);
            }
        }

        let playback = PlaybackController {
            current_time_s: 0.0,
            duration_s: initial_duration.max(0.1),
            is_playing: false,
            playback_speed: 1.0,
            loop_start_s: None,
            loop_end_s: None,
            ping_pong: false,
            ping_pong_direction: 1,
            fps: REVIEW_FPS,
        };

        let mut app = Self {
            run,
            current_variant: 0,
            mode: ReviewMode::Single,
            playback,
            preview_surfaces,
            preview_texture_ids,
            comments,
            selected_line: None,
            scroll_to_line: false,
            source_viewer: SourceViewer::default(),
            comment_ui: CommentUiState::default(),
            last_frame_at: Instant::now(),
            preview_dirty: true,
            error,
        };
        app.comment_ui.finished = finished;
        Ok(app)
    }

    fn current_document(&self) -> Option<&DocumentSession> {
        self.run
            .as_ref()
            .and_then(|run| run.variants.get(self.current_variant).map(|variant| &variant.document))
    }

    fn set_variant(&mut self, index: usize) {
        let Some(run) = self.run.as_mut() else {
            return;
        };
        if index >= run.variants.len() {
            return;
        }
        self.current_variant = index;
        let document = &run.variants[index].document;
        self.playback.duration_s = document_duration(document).max(0.1);
        self.playback.scrub_to(self.playback.current_time_s());

        if let Some(line) = self.selected_line {
            if line >= document.source_text.lines().count() {
                self.selected_line = None;
            } else {
                self.scroll_to_line = true;
            }
        }
        self.preview_dirty = true;
    }

    fn set_mode(&mut self, mode: ReviewMode) {
        if self.mode != mode {
            self.mode = mode;
            self.preview_dirty = true;
        }
    }

    fn scrub_to(&mut self, time_s: f64) {
        self.playback.scrub_to(time_s);
        self.preview_dirty = true;
    }

    fn select_source_line(&mut self, line: Option<usize>) {
        self.selected_line = line;
        self.scroll_to_line = true;
        if let (Some(line), Some(document)) = (line, self.current_document()) {
            if let Some(time_ms) = document.timeline_index.time_for_line(line) {
                self.scrub_to(time_ms as f64 / 1000.0);
            }
        }
    }

    fn add_comment(&mut self) {
        let note = self.comment_ui.note.trim();
        if note.is_empty() {
            return;
        }
        let Some(run) = self.run.as_ref() else {
            return;
        };
        let variant = run
            .variants
            .get(self.current_variant)
            .map(|variant| variant.id.clone())
            .unwrap_or_else(|| "variant".to_string());
        let id = format!(
            "{}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default(),
            self.comments.len()
        );
        let comment = ReviewComment {
            id,
            variant,
            time_ms: if self.comment_ui.capture_time {
                Some((self.playback.current_time_s() * 1000.0) as u64)
            } else {
                None
            },
            severity: self.comment_ui.severity,
            note: note.to_string(),
        };
        self.comments.push(comment);
        self.comment_ui.note.clear();
        self.comment_ui.open = false;
        self.save_comments();
    }

    fn delete_comment(&mut self, index: usize) {
        if index < self.comments.len() {
            self.comments.remove(index);
            self.save_comments();
        }
    }

    fn jump_to_comment(&mut self, index: usize) {
        let Some(comment) = self.comments.get(index) else {
            return;
        };
        let comment_variant = comment.variant.clone();
        let time_ms = comment.time_ms;

        let Some(run) = self.run.as_ref() else {
            return;
        };
        if let Some(variant_index) =
            run.variants.iter().position(|variant| variant.id == comment_variant)
        {
            self.current_variant = variant_index;
            self.playback.duration_s =
                document_duration(&run.variants[variant_index].document).max(0.1);
        }
        if let Some(time_ms) = time_ms {
            self.scrub_to(time_ms as f64 / 1000.0);
        }
    }

    fn save_comments(&mut self) {
        let Some(run) = self.run.as_ref() else {
            return;
        };
        let path = run.path.join("review.json");
        let Ok(serialized) = serde_json::to_string_pretty(&ReviewComments {
            comments: self.comments.clone(),
        }) else {
            tracing::warn!("Failed to serialize review comments");
            return;
        };
        if let Err(error) = fs::write(&path, serialized) {
            tracing::warn!("Failed to write review comments: {error}");
        }
    }

    fn mark_review_finished(&mut self) {
        let Some(run) = self.run.as_ref() else {
            return;
        };
        let marker_path = run.path.join(REVIEW_DONE_FILE);
        match fs::write(&marker_path, format!("finished={}\n", true)) {
            Ok(()) => {
                self.comment_ui.finished = true;
                self.error = None;
            },
            Err(error) => {
                tracing::warn!("Failed to write review done marker: {error}");
                self.error = Some(format!("Failed to write review.done: {error}"));
            },
        }
    }

    fn sync_preview(&mut self, frame: &mut eframe::Frame) -> Result<(), String> {
        let render_state = frame
            .wgpu_render_state()
            .ok_or_else(|| "wgpu render state not available".to_string())?;
        let device = &render_state.device;
        let queue = &render_state.queue;

        let Some(run) = self.run.as_ref() else {
            return Ok(());
        };
        let variant_count = run.variants.len();
        let render_indices: Vec<usize> = match self.mode {
            ReviewMode::Single => {
                if self.current_variant < variant_count {
                    vec![self.current_variant]
                } else {
                    Vec::new()
                }
            },
            ReviewMode::Compare => (0..variant_count).collect(),
        };

        if !self.preview_dirty {
            return Ok(());
        }

        let debug = DebugRenderOptions {
            draw_bounds: false,
            compute_hit_regions: false,
            draw_layout_debug: false,
            draw_spacing: false,
        };
        let time_s = self.playback.current_time_s();

        for index in render_indices {
            let Some(variant) = run.variants.get(index) else {
                continue;
            };
            let document = &variant.document;
            let surface = &mut self.preview_surfaces[index];

            if document.scene_dimensions.width > 0 && document.scene_dimensions.height > 0 {
                surface.set_dimensions(device, document.scene_dimensions);
            }

            let render_result = if let Some(composition) = document.composition.as_ref() {
                surface.render_composition(device, queue, composition, time_s, debug)
            } else if let Some(timeline) = document.active_timeline() {
                surface.render(device, queue, timeline, time_s, debug)
            } else {
                Ok(())
            };
            if let Err(error) = render_result {
                self.error = Some(error);
                return Ok(());
            }

            if let Some(sample_view) = surface.sample_view() {
                let mut renderer = render_state.renderer.write();
                let texture_id = match self.preview_texture_ids[index] {
                    Some(id) => {
                        renderer.update_egui_texture_from_wgpu_texture(
                            device,
                            sample_view,
                            wgpu::FilterMode::Linear,
                            id,
                        );
                        id
                    },
                    None => renderer.register_native_texture(
                        device,
                        sample_view,
                        wgpu::FilterMode::Linear,
                    ),
                };
                self.preview_texture_ids[index] = Some(texture_id);
            }
        }

        self.preview_dirty = false;
        Ok(())
    }
}

impl eframe::App for ReviewApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        let [r, g, b, a] = visuals.panel_fill.to_normalized_gamma_f32();
        [r, g, b, a]
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let now = Instant::now();
        let delta = now.saturating_duration_since(self.last_frame_at);
        self.last_frame_at = now;
        self.playback.tick(delta);
        if self.playback.is_playing {
            self.preview_dirty = true;
        }

        let wants_keyboard = ui.ctx().egui_wants_keyboard_input();
        let mut comment_form_requested = false;
        let mut new_variant = None;
        let mut mode_requested = None;
        let mut finish_review = false;
        if !wants_keyboard {
            ui.input(|input| {
                if input.key_pressed(egui::Key::Space) {
                    self.playback.toggle_playback();
                    self.preview_dirty = true;
                }
                if input.key_pressed(egui::Key::ArrowLeft) {
                    self.playback.step_frame(-1.0 / REVIEW_FPS as f64);
                    self.preview_dirty = true;
                }
                if input.key_pressed(egui::Key::ArrowRight) {
                    self.playback.step_frame(1.0 / REVIEW_FPS as f64);
                    self.preview_dirty = true;
                }
                if input.key_pressed(egui::Key::C) {
                    comment_form_requested = true;
                }
                if input.key_pressed(egui::Key::D) {
                    finish_review = true;
                }
                if input.key_pressed(egui::Key::A) {
                    new_variant = Some(0);
                }
                if input.key_pressed(egui::Key::B) {
                    new_variant = Some(1);
                }
                if input.key_pressed(egui::Key::M) {
                    mode_requested = Some(match self.mode {
                        ReviewMode::Single => ReviewMode::Compare,
                        ReviewMode::Compare => ReviewMode::Single,
                    });
                }
            });
        }

        if comment_form_requested {
            self.comment_ui.open = true;
            self.comment_ui.focus_requested = true;
        }

        let mut new_time = None;
        let mut clicked_line = None;
        let mut add_comment = false;
        let mut delete_comment = None;
        let mut jump_comment = None;
        let mut clear_error = false;

        egui::Panel::top("review_status").resizable(false).show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let run_name = self
                    .run
                    .as_ref()
                    .map(|run| run.name.clone())
                    .unwrap_or_else(|| "Review".to_string());
                ui.label(egui::RichText::new(run_name).strong().size(18.0));
            });
            ui.add_space(4.0);
        });

        egui::Panel::bottom("review_console")
            .resizable(true)
            .default_size(360.0)
            .min_size(220.0)
            .max_size(560.0)
            .show_inside(ui, |ui| {
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    if let Some(run) = self.run.as_ref() {
                        for (index, variant) in run.variants.iter().enumerate() {
                            let response = ui.stable_selectable_label(
                                index == self.current_variant,
                                &variant.label,
                            );
                            text_tooltip(
                                ui,
                                response.id.with(("variant_tip", index)),
                                &response,
                                &format!(
                                    "Comment target: {} (key {})",
                                    variant.label,
                                    variant.id.to_uppercase()
                                ),
                            );
                            if response.clicked() {
                                new_variant = Some(index);
                            }
                        }
                    }

                    ui.separator();

                    for mode in [ReviewMode::Single, ReviewMode::Compare] {
                        let response = ui.stable_selectable_label(self.mode == mode, mode.label());
                        text_tooltip(
                            ui,
                            response.id.with(("mode_tip", mode.label())),
                            &response,
                            if mode == ReviewMode::Compare {
                                "Show all variants side by side (M)"
                            } else {
                                "Show only the focused variant (M)"
                            },
                        );
                        if response.clicked() {
                            mode_requested = Some(mode);
                        }
                    }

                    ui.separator();

                    let play_label = if self.playback.is_playing {
                        PAUSE
                    } else {
                        PLAY
                    };
                    let play_btn =
                        ui.add(egui::Button::new(egui::RichText::new(play_label).size(16.0)));
                    text_tooltip(ui, play_btn.id.with("play_tip"), &play_btn, "Play/Pause (Space)");
                    if play_btn.clicked() {
                        self.playback.toggle_playback();
                        self.preview_dirty = true;
                    }
                    let prev_btn =
                        ui.add(egui::Button::new(egui::RichText::new(ARROW_LEFT).size(16.0)));
                    text_tooltip(ui, prev_btn.id.with("prev_tip"), &prev_btn, "Previous frame");
                    if prev_btn.clicked() {
                        self.playback.step_frame(-1.0 / REVIEW_FPS as f64);
                        self.preview_dirty = true;
                    }
                    let next_btn =
                        ui.add(egui::Button::new(egui::RichText::new(ARROW_RIGHT).size(16.0)));
                    text_tooltip(ui, next_btn.id.with("next_tip"), &next_btn, "Next frame");
                    if next_btn.clicked() {
                        self.playback.step_frame(1.0 / REVIEW_FPS as f64);
                        self.preview_dirty = true;
                    }

                    let duration = self.playback.duration_s.max(0.1);
                    let mut time = self.playback.current_time_s();
                    ui.add(
                        egui::Slider::new(&mut time, 0.0..=duration)
                            .show_value(false)
                            .custom_formatter(|value, _| format!("{value:.2}s")),
                    );
                    if (time - self.playback.current_time_s()).abs() > f64::EPSILON {
                        new_time = Some(time);
                    }
                    ui.label(format!("{:.2}s / {:.2}s", self.playback.current_time_s(), duration));

                    let mut speed = self.playback.playback_speed;
                    egui::ComboBox::from_id_salt("review_playback_speed")
                        .selected_text(format!("{speed:.2}x"))
                        .show_ui(ui, |ui| {
                            for preset in [0.25f32, 0.5, 0.75, 1.0, 1.5, 2.0] {
                                ui.selectable_value(&mut speed, preset, format!("{preset:.2}x"));
                            }
                        });
                    if (speed - self.playback.playback_speed).abs() > f32::EPSILON {
                        self.playback.playback_speed = speed;
                    }

                    ui.separator();

                    if self.comment_ui.finished {
                        ui.label(
                            egui::RichText::new(format!("{} Review marked done", CHECK_CIRCLE))
                                .color(CommentSeverity::Minor.color()),
                        );
                    } else {
                        let done_btn = ui.add(
                            egui::Button::new(egui::RichText::new(CHECK_CIRCLE).size(16.0))
                                .fill(egui::Color32::from_rgb(152, 195, 121)),
                        );
                        text_tooltip(
                            ui,
                            done_btn.id.with("done_tip"),
                            &done_btn,
                            "Mark this review as done for the agent and close (D)",
                        );
                        if done_btn.clicked() {
                            finish_review = true;
                        }
                    }
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Comments").strong());
                    ui.label(
                        egui::RichText::new("C opens the form, D finishes the review")
                            .weak()
                            .small(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.comment_ui.open {
                            if ui.button("Cancel").clicked() {
                                self.comment_ui.open = false;
                                self.comment_ui.note.clear();
                            }
                        } else {
                            let comment_btn = ui.add(
                                egui::Button::new(
                                    egui::RichText::new(format!("{CHAT_CIRCLE_TEXT} New comment"))
                                        .size(14.0),
                                )
                                .fill(egui::Color32::from_rgb(97, 175, 239)),
                            );
                            text_tooltip(
                                ui,
                                comment_btn.id.with("comment_tip"),
                                &comment_btn,
                                "Open the comment form (C)",
                            );
                            if comment_btn.clicked() {
                                self.comment_ui.open = true;
                                self.comment_ui.focus_requested = true;
                            }
                        }
                    });
                });

                if self.comment_ui.open {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        let capture_btn = ui.checkbox(
                            &mut self.comment_ui.capture_time,
                            format!("Capture time {:.2}s", self.playback.current_time_s()),
                        );
                        text_tooltip(
                            ui,
                            capture_btn.id.with("capture_tip"),
                            &capture_btn,
                            "Anchors this comment to the current playback time",
                        );
                    });
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            egui::TextEdit::multiline(&mut self.comment_ui.note)
                                .id_salt("review_comment_note")
                                .hint_text("What did you observe?")
                                .desired_width(ui.available_width() - 120.0)
                                .desired_rows(2),
                        );
                        if self.comment_ui.focus_requested {
                            response.request_focus();
                            self.comment_ui.focus_requested = false;
                        }
                        if ui.button("Add").clicked() {
                            add_comment = true;
                        }
                    });
                }

                ui.add_space(4.0);
                egui::ScrollArea::vertical()
                    .id_salt("review_comments_list")
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for (index, comment) in self.comments.iter().enumerate() {
                            ui.horizontal_wrapped(|ui| {
                                let time_label = comment
                                    .time_ms
                                    .map(|ms| format!("{:.2}s", ms as f64 / 1000.0))
                                    .unwrap_or_else(|| "-".to_string());
                                let variant_label = comment.variant.to_uppercase();
                                let label = format!(
                                    "{} @ {} | {}",
                                    variant_label, time_label, comment.note
                                );
                                let response = ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(label).color(comment.severity.color()),
                                    )
                                    .selectable(true),
                                );
                                text_tooltip(
                                    ui,
                                    response.id.with("comment_jump_tip"),
                                    &response,
                                    if comment.time_ms.is_some() {
                                        "Jump to this comment"
                                    } else {
                                        "No time anchor"
                                    },
                                );
                                if response.clicked() {
                                    jump_comment = Some(index);
                                }
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        let delete_btn =
                                            ui.small_button(egui::RichText::new(TRASH).size(12.0));
                                        text_tooltip(
                                            ui,
                                            delete_btn.id.with("delete_tip"),
                                            &delete_btn,
                                            "Delete comment",
                                        );
                                        if delete_btn.clicked() {
                                            delete_comment = Some(index);
                                        }
                                    },
                                );
                            });
                        }
                    });
                ui.add_space(4.0);
            });

        if let Some(index) = new_variant {
            self.set_variant(index);
        }
        if let Some(mode) = mode_requested {
            self.set_mode(mode);
        }

        egui::Panel::left("review_source")
            .resizable(true)
            .default_size(520.0)
            .size_range(300.0..=760.0)
            .show_inside(ui, |ui| {
                if let Some(run) = self.run.as_ref() {
                    if let Some(variant) = run.variants.get(self.current_variant) {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Source {}", variant.label))
                                    .strong()
                                    .size(16.0),
                            );
                            ui.label(
                                egui::RichText::new(variant.path.display().to_string())
                                    .weak()
                                    .small(),
                            );
                        });

                        if !variant.document.diagnostics.is_empty() {
                            ui.add_space(4.0);
                            egui::CollapsingHeader::new(format!(
                                "Diagnostics ({})",
                                variant.document.diagnostics.len()
                            ))
                            .default_open(false)
                            .show(ui, |ui| {
                                for diagnostic in &variant.document.diagnostics {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{:?}: {}",
                                            diagnostic.severity, diagnostic.message
                                        ))
                                        .color(match diagnostic.severity {
                                            animatix_syntax::diagnostics::DiagnosticSeverity::Error => {
                                                egui::Color32::from_rgb(224, 108, 117)
                                            },
                                            animatix_syntax::diagnostics::DiagnosticSeverity::Warning => {
                                                egui::Color32::from_rgb(229, 192, 123)
                                            },
                                            animatix_syntax::diagnostics::DiagnosticSeverity::Info => {
                                                egui::Color32::from_rgb(97, 175, 239)
                                            },
                                            animatix_syntax::diagnostics::DiagnosticSeverity::Hint => {
                                                egui::Color32::from_rgb(97, 175, 239)
                                            },
                                        }),
                                    );
                                }
                            });
                        }

                        ui.add_space(4.0);
                        clicked_line = self.source_viewer.show(
                            ui,
                            &variant.document.source_text,
                            &variant.document.diagnostics,
                            self.selected_line,
                            self.scroll_to_line,
                        );
                        self.scroll_to_line = false;
                    }
                } else {
                    ui.label("Review run could not be loaded.");
                    if let Some(error) = self.error.as_ref() {
                        ui.label(error);
                    }
                    if ui.button("Clear error").clicked() {
                        clear_error = true;
                    }
                }
            });

        if let Err(error) = self.sync_preview(frame) {
            self.error = Some(error);
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            let time_label = format!(
                "{:.2}s / {:.2}s",
                self.playback.current_time_s(),
                self.playback.duration_s
            );

            match self.mode {
                ReviewMode::Single => {
                    if let Some(run) = self.run.as_ref() {
                        if let Some(variant) = run.variants.get(self.current_variant) {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} {}",
                                        self.mode.label(),
                                        variant.label
                                    ))
                                    .strong()
                                    .color(CommentSeverity::Question.color()),
                                );
                                ui.label(egui::RichText::new("Comment target").weak().small());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(egui::RichText::new(time_label).weak());
                                    },
                                );
                            });
                            ui.add_space(4.0);

                            if let Some(texture_id) = self.preview_texture_ids[self.current_variant]
                            {
                                let dims = self.preview_surfaces[self.current_variant].dimensions();
                                show_preview_image(ui, texture_id, dims);
                            } else {
                                ui.label("Preview will render after the first frame.");
                            }
                        }
                    }
                },
                ReviewMode::Compare => {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Compare")
                                .strong()
                                .color(CommentSeverity::Question.color()),
                        );
                        ui.label(
                            egui::RichText::new("Click a preview to set comment target")
                                .weak()
                                .small(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(time_label).weak());
                        });
                    });
                    ui.add_space(4.0);

                    let entries = self
                        .run
                        .as_ref()
                        .map(|run| {
                            run.variants
                                .iter()
                                .enumerate()
                                .map(|(index, variant)| {
                                    (
                                        index,
                                        variant.label.clone(),
                                        self.preview_texture_ids[index],
                                        self.preview_surfaces[index].dimensions(),
                                    )
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();

                    if entries.is_empty() {
                        return;
                    }

                    egui::ScrollArea::horizontal().id_salt("review_compare_previews").show(
                        ui,
                        |ui| {
                            ui.columns(entries.len(), |columns| {
                                for (index, (variant_index, label, texture_id, dims)) in
                                    entries.into_iter().enumerate()
                                {
                                    let column = &mut columns[index];
                                    let active = variant_index == self.current_variant;
                                    let variant_btn = column.stable_selectable_label(
                                        active,
                                        egui::RichText::new(label).strong(),
                                    );
                                    text_tooltip(
                                        column,
                                        variant_btn.id.with("variant_target_tip"),
                                        &variant_btn,
                                        "Set this variant as the comment target",
                                    );
                                    if variant_btn.clicked() {
                                        new_variant = Some(variant_index);
                                    }

                                    if let Some(texture_id) = texture_id {
                                        show_preview_image(column, texture_id, dims);
                                    } else {
                                        column.label("Preview will render after the first frame.");
                                    }
                                }
                            });
                        },
                    );
                },
            }
        });

        if let Some(index) = new_variant {
            self.set_variant(index);
        }
        if let Some(time) = new_time {
            self.scrub_to(time);
        }
        if let Some(line) = clicked_line {
            self.select_source_line(Some(line));
        }
        if add_comment {
            self.add_comment();
        }
        if let Some(index) = delete_comment {
            self.delete_comment(index);
        }
        if let Some(index) = jump_comment {
            self.jump_to_comment(index);
        }
        if finish_review {
            self.mark_review_finished();
            if self.comment_ui.finished {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if clear_error {
            self.error = None;
        }

        if self.playback.is_playing || self.preview_dirty {
            ui.ctx().request_repaint();
        }
    }
}

fn show_preview_image(ui: &mut egui::Ui, texture_id: egui::TextureId, dims: SceneDimensions) {
    let available = ui.available_size();
    if dims.width > 0 && dims.height > 0 {
        let scale = (available.x / dims.width as f32)
            .min(available.y / dims.height as f32)
            .clamp(0.01, 1.0);
        let size = egui::vec2(dims.width as f32 * scale, dims.height as f32 * scale);
        ui.centered_and_justified(|ui| {
            ui.add(
                egui::Image::new((texture_id, size))
                    .fit_to_exact_size(size)
                    .maintain_aspect_ratio(true),
            );
        });
    }
}

fn document_duration(document: &DocumentSession) -> f64 {
    document
        .composition
        .as_ref()
        .map(|composition| composition.global_duration_s)
        .or_else(|| document.active_timeline().map(|timeline| timeline.duration_seconds()))
        .unwrap_or(document.duration_s)
}

#[cfg(test)]
mod tests;
