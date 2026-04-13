use crate::document::{DocumentSession, default_file_path};
use crate::editor::EditorBuffer;
use crate::preview_surface::PreviewSurface;
use animatix::timeline::SceneDimensions;
use directories::ProjectDirs;
use egui::{Align, Color32, RichText, Stroke, Vec2};
use egui_dock::{DockArea, DockState, NodeIndex, Style, TabViewer};
use egui_wgpu_backend::{RenderPass, ScreenDescriptor};
use egui_winit::State as EguiWinitState;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const INITIAL_WINDOW_SIZE: (f64, f64) = (1440.0, 960.0);
const DEFAULT_PREVIEW_SIZE: SceneDimensions = SceneDimensions {
    width: 1280,
    height: 720,
};
const REBUILD_DEBOUNCE: Duration = Duration::from_millis(150);
const MAX_TREE_DEPTH: usize = 4;
const MAX_TREE_ENTRIES: usize = 200;

pub fn run_gui(path: Option<PathBuf>) {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    let initial_path = path.unwrap_or_else(default_file_path);
    let mut app = GuiRuntime::new(initial_path);
    event_loop.run_app(&mut app).expect("Failed to run GUI app");
}

struct GuiRuntime {
    initial_path: PathBuf,
    runtime: Option<WindowRuntime>,
}

impl GuiRuntime {
    fn new(initial_path: PathBuf) -> Self {
        Self {
            initial_path,
            runtime: None,
        }
    }
}

impl ApplicationHandler for GuiRuntime {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.runtime.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Animatix")
            .with_inner_size(LogicalSize::new(
                INITIAL_WINDOW_SIZE.0,
                INITIAL_WINDOW_SIZE.1,
            ));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("Failed to create window"),
        );

        self.runtime = Some(
            WindowRuntime::new(window, self.initial_path.clone())
                .expect("Failed to initialize egui runtime"),
        );
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        if runtime.window.id() != window_id {
            return;
        }

        if runtime.on_window_event(event_loop, &event) {
            event_loop.exit();
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(runtime) = self.runtime.as_ref()
            && runtime.needs_redraw()
        {
            runtime.window.request_redraw();
        }
    }
}

struct WindowRuntime {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    render_pass: RenderPass,
    egui_ctx: egui::Context,
    egui_winit: EguiWinitState,
    shell: GuiShell,
    preview_surface: PreviewSurface,
}

impl WindowRuntime {
    fn new(window: Arc<Window>, initial_path: PathBuf) -> Result<Self, String> {
        let (surface, surface_config, device, queue, surface_format) =
            pollster::block_on(create_graphics_state(window.clone()))?;

        let egui_ctx = egui::Context::default();
        egui_ctx.set_visuals(egui::Visuals::dark());
        install_theme(&egui_ctx);

        let egui_winit = EguiWinitState::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(device.limits().max_texture_dimension_2d as usize),
        );

        let render_pass = RenderPass::new(&device, surface_format, 1);
        let preview_surface = PreviewSurface::new(&device, &queue);
        let shell = GuiShell::load(initial_path);

        Ok(Self {
            window,
            surface,
            surface_config,
            device,
            queue,
            render_pass,
            egui_ctx,
            egui_winit,
            shell,
            preview_surface,
        })
    }

    fn needs_redraw(&self) -> bool {
        self.shell.is_playing() || self.shell.preview_dirty || self.shell.has_pending_rebuild()
    }

    fn on_window_event(&mut self, event_loop: &ActiveEventLoop, event: &WindowEvent) -> bool {
        let response = self.egui_winit.on_window_event(self.window.as_ref(), event);
        if response.repaint {
            self.window.request_redraw();
        }

        match event {
            WindowEvent::CloseRequested => {
                self.shell.save_persistence();
                return true;
            }
            WindowEvent::Resized(size) => {
                self.resize(*size);
                self.window.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.redraw() {
                    self.shell
                        .set_status(format!("Render failed • {error}"), Some(error));
                    self.window.request_redraw();
                }
            }
            _ => {}
        }

        if response.consumed {
            event_loop.set_control_flow(ControlFlow::Poll);
        }

        false
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }

        self.surface_config.width = new_size.width;
        self.surface_config.height = new_size.height;
        self.surface.configure(&self.device, &self.surface_config);
    }

    fn redraw(&mut self) -> Result<(), String> {
        self.shell.prepare_frame();
        self.sync_preview_surface()?;

        let raw_input = self.egui_winit.take_egui_input(self.window.as_ref());
        let preview_texture_id = self.preview_surface.texture_id();
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            self.shell.ui(ctx, preview_texture_id);
        });

        self.egui_winit
            .handle_platform_output(self.window.as_ref(), full_output.platform_output.clone());

        let pixels_per_point = egui_winit::pixels_per_point(&self.egui_ctx, self.window.as_ref());
        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, pixels_per_point);
        let screen_descriptor = ScreenDescriptor {
            physical_width: self.surface_config.width,
            physical_height: self.surface_config.height,
            scale_factor: pixels_per_point,
        };

        self.render_pass
            .add_textures(&self.device, &self.queue, &full_output.textures_delta)
            .map_err(|err| err.to_string())?;
        self.render_pass
            .update_buffers(&self.device, &self.queue, &paint_jobs, &screen_descriptor);

        let surface_texture = match self.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.surface_config);
                return Ok(());
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                return Err("The GPU ran out of memory while acquiring the frame".to_string());
            }
            Err(wgpu::SurfaceError::Timeout) | Err(wgpu::SurfaceError::Other) => return Ok(()),
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Animatix Egui Encoder"),
            });

        self.render_pass
            .execute(
                &mut encoder,
                &view,
                &paint_jobs,
                &screen_descriptor,
                Some(clear_color()),
            )
            .map_err(|err| err.to_string())?;

        self.queue.submit(std::iter::once(encoder.finish()));
        surface_texture.present();

        self.render_pass
            .remove_textures(full_output.textures_delta)
            .map_err(|err| err.to_string())?;

        if self.needs_redraw() {
            self.window.request_redraw();
        }

        Ok(())
    }

    fn sync_preview_surface(&mut self) -> Result<(), String> {
        let dimensions = self.shell.preview.dimensions;
        self.preview_surface
            .set_dimensions(&self.device, dimensions);

        if self.shell.preview_dirty {
            if let Some(timeline) = self.shell.document.timeline.as_ref() {
                self.preview_surface.render(
                    &self.device,
                    &self.queue,
                    timeline,
                    self.shell.preview.current_time_s,
                )?;
                let _ = self
                    .preview_surface
                    .sync_egui_texture(&self.device, &mut self.render_pass)?;
                self.shell.preview_dirty = false;
                self.shell.preview.error = None;
                self.shell.preview.status = format!(
                    "Live preview • t = {:.2}s / {:.2}s",
                    self.shell.preview.current_time_s, self.shell.preview.duration_s
                );
            }
        } else if self.preview_surface.texture_id().is_none()
            && self.preview_surface.dimensions().width > 0
            && self.preview_surface.dimensions().height > 0
        {
            let _ = self
                .preview_surface
                .sync_egui_texture(&self.device, &mut self.render_pass)?;
        }

        Ok(())
    }
}

async fn create_graphics_state(
    window: Arc<Window>,
) -> Result<
    (
        wgpu::Surface<'static>,
        wgpu::SurfaceConfiguration,
        wgpu::Device,
        wgpu::Queue,
        wgpu::TextureFormat,
    ),
    String,
> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });
    let size = window.inner_size();
    let surface = instance
        .create_surface(window.clone())
        .map_err(|err| format!("Failed to create window surface: {err}"))?;

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .map_err(|err| format!("Failed to find a compatible GPU adapter: {err}"))?;

    let limits = wgpu::Limits::default().using_resolution(adapter.limits());
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Animatix GUI Device"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            memory_hints: Default::default(),
            ..Default::default()
        })
        .await
        .map_err(|err| format!("Failed to create GPU device: {err}"))?;

    let caps = surface.get_capabilities(&adapter);
    let format = caps
        .formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .unwrap_or(caps.formats[0]);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: caps.present_modes[0],
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    Ok((surface, config, device, queue, format))
}

fn clear_color() -> wgpu::Color {
    let [r, g, b, a] = Color32::from_rgb(18, 20, 24).to_normalized_gamma_f32();
    wgpu::Color {
        r: r as f64,
        g: g as f64,
        b: b as f64,
        a: a as f64,
    }
}

fn install_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(12.0, 8.0);
    style.spacing.window_margin = egui::Margin::same(8);
    style.visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(22, 25, 31);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(35, 39, 47);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(49, 55, 66);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(70, 78, 92);
    style.visuals.panel_fill = Color32::from_rgb(18, 20, 24);
    ctx.set_style(style);
}

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
    fn new(duration_s: f64) -> Self {
        Self {
            current_time_s: 0.0,
            duration_s,
            is_playing: false,
            status: "Loaded file".to_string(),
            error: None,
            dimensions: DEFAULT_PREVIEW_SIZE,
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
    editor: EditorBuffer,
    workspace_root: PathBuf,
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
            Ok(document) => (document, None, None),
            Err(error) => (
                DocumentSession::from_error(initial_path.clone()),
                Some("Failed to initialize session".to_string()),
                Some(error),
            ),
        };

        let workspace_root = workspace_root_for(&document.file_path);
        let file_tree = build_file_tree(&workspace_root, &document.file_path);
        let persistence_path = persistence_path();
        let dock_state =
            load_workspace_persistence(&persistence_path).unwrap_or_else(default_dock_state);
        let duration_s = document.duration_s.max(0.1);
        let mut preview = PreviewPaneState::new(duration_s);
        if let Some(status) = status {
            preview.status = status;
        }
        preview.error = error;

        Self {
            editor: EditorBuffer::new(&document.file_path, document.source_text.clone()),
            document,
            workspace_root,
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
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Animatix").strong().size(18.0));
            ui.separator();

            ui.menu_button("File", |ui| {
                if ui.button("Save").clicked() {
                    actions.save = true;
                    ui.close();
                }
                if ui.button("Reload").clicked() {
                    actions.reload = true;
                    ui.close();
                }
            });
            ui.menu_button("Edit", |ui| {
                ui.label("Text editing lives in the center editor pane.");
            });
            ui.menu_button("Preview", |ui| {
                if ui
                    .button(if self.preview.is_playing {
                        "Pause"
                    } else {
                        "Play"
                    })
                    .clicked()
                {
                    actions.toggle_playback = true;
                    ui.close();
                }
                if ui.button("Rebuild").clicked() {
                    actions.rebuild = true;
                    ui.close();
                }
            });
            ui.menu_button("Run", |ui| {
                if ui.button("Rebuild now").clicked() {
                    actions.rebuild = true;
                    ui.close();
                }
            });

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
                action_button(ui, "Reload", false, || {
                    actions.reload = true;
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
        ui.add_space(4.0);
    }

    fn status_bar_ui(&self, ui: &mut egui::Ui) {
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
        let mut viewer = WorkspaceViewer {
            current_file: &self.document.file_path,
            workspace_root: &self.workspace_root,
            file_tree: &self.file_tree,
            editor: &mut self.editor,
            preview: &mut self.preview,
            preview_texture_id,
            actions,
            source_dirty: &mut self.document.source_text,
        };

        DockArea::new(&mut self.dock_state)
            .style(Style::from_egui(ui.style().as_ref()))
            .show_inside(ui, &mut viewer);
    }

    fn handle_actions(&mut self, actions: UiActions) {
        if let Some(path) = actions.open_file {
            self.open_document(path);
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
            self.preview_dirty = true;
        }
        if let Some(dimensions) = actions.preview_dimensions {
            if dimensions.width > 0
                && dimensions.height > 0
                && self.preview.dimensions != dimensions
            {
                self.preview.dimensions = dimensions;
                self.preview_dirty = true;
            }
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
                self.workspace_root = workspace_root_for(&path);
                self.file_tree = build_file_tree(&self.workspace_root, &path);
                self.editor
                    .set_document(&document.file_path, document.source_text.clone());
                self.preview.duration_s = document.duration_s.max(0.1);
                self.preview.current_time_s = 0.0;
                self.preview.is_playing = false;
                self.preview.status = format!("Opened {}", document.file_path.display());
                self.preview.error = None;
                self.document = document;
                self.preview_dirty = true;
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
        self.preview.duration_s = self.document.duration_s.max(0.1);
        self.preview.clamp_time();
        self.preview.status = format!("Reloaded {}", self.document.file_path.display());
        self.preview.error = None;
        self.preview_dirty = true;
        self.file_tree = build_file_tree(&self.workspace_root, &self.document.file_path);
        Ok(())
    }

    fn rebuild(&mut self) -> Result<(), String> {
        self.document.rebuild()?;
        self.preview.duration_s = self.document.duration_s.max(0.1);
        self.preview.clamp_time();
        self.preview.status = format!(
            "Built timeline • {:.2}s total duration",
            self.preview.duration_s
        );
        self.preview.error = None;
        self.preview_dirty = true;
        Ok(())
    }

    fn set_status(&mut self, status: String, error: Option<String>) {
        self.preview.status = status;
        self.preview.error = error;
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

#[derive(Default)]
struct UiActions {
    open_file: Option<PathBuf>,
    save: bool,
    reload: bool,
    rebuild: bool,
    toggle_playback: bool,
    scrub_to: Option<f64>,
    preview_dimensions: Option<SceneDimensions>,
    editor_changed: bool,
    request_repaint: bool,
}

struct WorkspaceViewer<'a> {
    current_file: &'a Path,
    workspace_root: &'a Path,
    file_tree: &'a [FileTreeEntry],
    editor: &'a mut EditorBuffer,
    preview: &'a mut PreviewPaneState,
    preview_texture_id: Option<egui::TextureId>,
    actions: &'a mut UiActions,
    source_dirty: &'a mut String,
}

impl TabViewer for WorkspaceViewer<'_> {
    type Tab = WorkspaceTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            WorkspaceTab::Explorer => "Explorer".into(),
            WorkspaceTab::Editor => "Editor".into(),
            WorkspaceTab::Preview => "Preview".into(),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        match tab {
            WorkspaceTab::Explorer => self.explorer_ui(ui),
            WorkspaceTab::Editor => self.editor_ui(ui),
            WorkspaceTab::Preview => self.preview_ui(ui),
        }
    }

    fn closeable(&mut self, _tab: &mut Self::Tab) -> bool {
        false
    }
}

impl WorkspaceViewer<'_> {
    fn explorer_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.label(RichText::new("Workspace").strong());
            ui.label(
                RichText::new(self.workspace_root.display().to_string())
                    .monospace()
                    .small(),
            );
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                for entry in self.file_tree {
                    let selected = entry.path == self.current_file;
                    ui.horizontal(|ui| {
                        ui.add_space(entry.depth as f32 * 14.0);
                        let label = if entry.is_dir {
                            format!("▾ {}", entry.name)
                        } else {
                            format!("• {}", entry.name)
                        };
                        let response = ui.selectable_label(selected, label);
                        if response.clicked() && !entry.is_dir {
                            self.actions.open_file = Some(entry.path.clone());
                        }
                    });
                }
            });
        });
    }

    fn editor_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(
                        self.current_file
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("Untitled"),
                    )
                    .strong(),
                );
                ui.label(
                    RichText::new(self.current_file.display().to_string())
                        .monospace()
                        .small()
                        .weak(),
                );
            });
            ui.separator();

            let response = self.editor.show(ui);
            if response.changed() {
                *self.source_dirty = self.editor.text().to_string();
                self.actions.editor_changed = true;
            }
        });
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Preview").strong());
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    if self.preview.is_playing {
                        badge(
                            ui,
                            "Playing",
                            Color32::from_rgb(46, 106, 80),
                            Color32::from_rgb(216, 249, 235),
                        );
                    } else {
                        badge(
                            ui,
                            "Paused",
                            Color32::from_rgb(60, 64, 76),
                            Color32::from_rgb(228, 232, 243),
                        );
                    }
                });
            });
            ui.label(RichText::new(&self.preview.status).small().weak());
            ui.separator();

            let available = ui.available_size_before_wrap();
            let image_height = (available.y - 96.0).max(220.0);
            let desired = fit_16_9(Vec2::new(available.x.max(200.0), image_height));
            let dimensions = SceneDimensions {
                width: (desired.x * ui.ctx().pixels_per_point()).round().max(1.0) as u32,
                height: (desired.y * ui.ctx().pixels_per_point()).round().max(1.0) as u32,
            };
            self.actions.preview_dimensions = Some(dimensions);

            egui::Frame::canvas(ui.style())
                .stroke(Stroke::new(1.0, Color32::from_rgb(58, 63, 74)))
                .show(ui, |ui| {
                    ui.set_min_size(desired);
                    match self.preview_texture_id {
                        Some(texture_id) => {
                            ui.centered_and_justified(|ui| {
                                ui.image((texture_id, desired));
                            });
                        }
                        None => {
                            ui.centered_and_justified(|ui| {
                                ui.label(RichText::new("Preview initializing…").weak());
                            });
                        }
                    }
                });

            ui.horizontal(|ui| {
                ui.label(RichText::new(format!(
                    "t = {:.2}s / {:.2}s",
                    self.preview.current_time_s, self.preview.duration_s
                )));
                if ui
                    .button(if self.preview.is_playing {
                        "Pause"
                    } else {
                        "Play"
                    })
                    .clicked()
                {
                    self.actions.toggle_playback = true;
                }
                if ui.button("Rebuild").clicked() {
                    self.actions.rebuild = true;
                }
            });

            let mut scrub = self.preview.current_time_s;
            let slider = egui::Slider::new(&mut scrub, 0.0..=self.preview.duration_s.max(0.1))
                .show_value(false)
                .step_by(0.01);
            if ui.add(slider).changed() {
                self.actions.scrub_to = Some(scrub);
            }

            if let Some(error) = &self.preview.error {
                ui.separator();
                ui.colored_label(Color32::from_rgb(255, 136, 136), error);
            }
        });
    }
}

fn default_dock_state() -> DockState<WorkspaceTab> {
    let mut dock_state = DockState::new(vec![WorkspaceTab::Editor]);
    let surface = dock_state.main_surface_mut();
    let [editor, _explorer] =
        surface.split_left(NodeIndex::root(), 0.18, vec![WorkspaceTab::Explorer]);
    let [_editor, _preview] = surface.split_right(editor, 0.37, vec![WorkspaceTab::Preview]);
    dock_state
}

fn persistence_path() -> PathBuf {
    if let Some(project_dirs) = ProjectDirs::from("dev", "animatix", "animatix") {
        return project_dirs.config_dir().join("workspace_layout.ron");
    }

    PathBuf::from(".animatix-workspace-layout.ron")
}

fn load_workspace_persistence(path: &Path) -> Option<DockState<WorkspaceTab>> {
    let content = fs::read_to_string(path).ok()?;
    let persistence = ron::from_str::<WorkspacePersistence>(&content).ok()?;
    Some(persistence.dock_state)
}

fn workspace_root_for(file_path: &Path) -> PathBuf {
    for ancestor in file_path.ancestors() {
        if ancestor.join(".git").exists() || ancestor.join("Cargo.toml").exists() {
            return ancestor.to_path_buf();
        }
    }
    file_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn build_file_tree(workspace_root: &Path, current_file: &Path) -> Vec<FileTreeEntry> {
    let mut entries = Vec::new();
    let mut remaining = MAX_TREE_ENTRIES;
    collect_tree_entries(
        workspace_root,
        current_file,
        0,
        &mut remaining,
        &mut entries,
    );
    entries
}

fn collect_tree_entries(
    dir: &Path,
    current_file: &Path,
    depth: usize,
    remaining: &mut usize,
    entries: &mut Vec<FileTreeEntry>,
) {
    if depth > MAX_TREE_DEPTH || *remaining == 0 {
        return;
    }

    let read_dir = match fs::read_dir(dir) {
        Ok(read_dir) => read_dir,
        Err(_) => return,
    };

    let mut children: Vec<_> = read_dir.filter_map(Result::ok).collect();
    children.sort_by(|a, b| match (a.path().is_dir(), b.path().is_dir()) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.file_name().cmp(&b.file_name()),
    });

    for child in children {
        if *remaining == 0 {
            return;
        }

        let path = child.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') && !current_file.starts_with(&path) {
            continue;
        }

        let is_dir = path.is_dir();
        entries.push(FileTreeEntry {
            path: path.clone(),
            name: name.to_string(),
            depth,
            is_dir,
        });
        *remaining = remaining.saturating_sub(1);

        if is_dir {
            collect_tree_entries(&path, current_file, depth + 1, remaining, entries);
        }
    }
}

fn fit_16_9(available: Vec2) -> Vec2 {
    let aspect = 16.0 / 9.0;
    let width_limited_height = available.x / aspect;
    if width_limited_height <= available.y {
        Vec2::new(available.x, width_limited_height)
    } else {
        Vec2::new(available.y * aspect, available.y)
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

#[cfg(test)]
mod tests {
    use super::{WorkspaceTab, default_dock_state, fit_16_9};
    use egui::Vec2;

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
        let fitted = fit_16_9(Vec2::new(400.0, 400.0));
        assert!((fitted.x / fitted.y - 16.0 / 9.0).abs() < 0.001);
    }
}
