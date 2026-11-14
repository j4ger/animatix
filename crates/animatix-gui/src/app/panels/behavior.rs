use std::collections::HashSet;

use egui::{Color32, Rect, Stroke, Visuals};
use egui_tiles::{Behavior, SimplificationOptions, TileId, UiResponse};

use crate::app::commands::ActionQueue;
use crate::app::design_tokens::spatial::STROKE_WIDTH;
use crate::app::design_tokens::spatial::timeline::RULER_HEIGHT as TIMELINE_RULER_HEIGHT;
use crate::app::panels::{curves_panel, editor, inspector, preview_panel, sidebar, timeline_panel};
use crate::app::preview::selection;
use crate::app::stores::{DocumentStore, PreviewStore, WorkspaceStore};
use crate::app::{WorkspaceTab, preview};

pub(crate) struct WorkspaceBehavior<'a> {
    pub(crate) document_store: &'a mut DocumentStore,
    pub(crate) workspace_store: &'a mut WorkspaceStore,
    pub(crate) preview_store: &'a mut PreviewStore,
    pub(crate) commands: &'a mut ActionQueue,
    pub(crate) preview_texture_id: Option<egui::TextureId>,
    pub(crate) collapsed_actors: &'a mut HashSet<String>,
    pub(crate) expanded_properties: &'a mut HashSet<String>,
    pub(crate) selected_actors: &'a mut HashSet<String>,
    pub(crate) hit_regions: &'a [(String, kurbo::Rect)],
    pub(crate) selection: &'a mut selection::SelectionState,
    pub(crate) tool_mode: &'a mut preview::ToolMode,
    pub(crate) sidebar_tab: &'a mut crate::app::panels::SidebarTab,
    /// True when the window is narrow enough for the compact sidebar icon rail.
    pub(crate) compact: bool,
    /// Compact-mode overlay drawer slot (opened by the sidebar rail).
    pub(crate) compact_drawer: &'a mut Option<crate::app::panels::CompactDrawer>,
    pub(crate) snap_fps: f32,
    pub(crate) debug_layout: bool,
    pub(crate) debug_spacing: bool,
    /// Set by the timeline panel each frame; true when the panel has pointer interaction.
    pub(crate) timeline_focused: &'a mut bool,
}

impl<'a> Behavior<WorkspaceTab> for WorkspaceBehavior<'a> {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: TileId,
        pane: &mut WorkspaceTab,
    ) -> UiResponse {
        match pane {
            WorkspaceTab::Sidebar => {
                let timeline = self.document_store.source.document.timeline.as_ref();
                let asset_cache = timeline.map(|t| t.asset_cache());
                let mut ctx = sidebar::SidebarContext {
                    active_scene: self.document_store.source.document.active_scene.as_deref(),
                    composition: self.document_store.source.document.composition.as_ref(),
                    current_file: &self.document_store.source.document.file_path,
                    expanded_dirs: &mut self.workspace_store.expanded_dirs,
                    file_tree: &self.workspace_store.file_tree,
                    commands: self.commands,
                    timeline,
                    selected_actors: self.selected_actors,
                    collapsed_actors: self.collapsed_actors,
                    sidebar_tab: self.sidebar_tab,
                    compact: self.compact,
                    compact_drawer: self.compact_drawer,
                    components: &self.document_store.source.document.components,
                    asset_cache,
                };
                sidebar::sidebar_ui(&mut ctx, ui);
            },
            WorkspaceTab::Code => {
                // Source editor; shares the right detail tab group with the
                // Inspector.
                let diagnostics = self.document_store.combined_diagnostics();
                let mut ctx = editor::EditorContext {
                    editor: &mut self.document_store.source.editor,
                    diagnostics: &diagnostics,
                    source_dirty: &mut self.document_store.source.document.source_text,
                    commands: self.commands,
                    is_playing: self.preview_store.preview.playback.is_playing,
                };
                editor::editor_ui(&mut ctx, ui);
            },
            WorkspaceTab::Preview => {
                let active_tl = self.document_store.source.document.active_timeline();
                let has_scene = active_tl.map(|tl| !tl.tracks().is_empty()).unwrap_or(false);
                let source_current = !self.preview_store.has_pending_rebuild()
                    && !self.preview_store.rebuild_in_progress
                    && self.preview_store.in_flight_rebuild.is_none()
                    && self.document_store.source.document.last_rebuild_error.is_none();
                let mut ctx = preview_panel::PreviewContext {
                    scene_dimensions: self.document_store.source.document.scene_dimensions,
                    preview: &mut self.preview_store.preview,
                    preview_texture_id: self.preview_texture_id,
                    commands: self.commands,
                    selection: self.selection,
                    selected_actors: self.selected_actors,
                    hit_regions: self.hit_regions,
                    timeline: active_tl,
                    tool_mode: self.tool_mode,
                    composition: self.document_store.source.document.composition.as_ref(),
                    active_scene: self.document_store.source.document.active_scene.as_deref(),
                    performance_metrics: &mut self.preview_store.performance_metrics,
                    debug_layout: self.debug_layout,
                    debug_spacing: self.debug_spacing,
                    rebuild_in_progress: self.preview_store.rebuild_in_progress,
                    has_scene,
                    source_text: source_current
                        .then_some(self.document_store.source.document.source_text.as_str()),
                };
                preview_panel::preview_panel_ui(&mut ctx, ui);
            },
            WorkspaceTab::Inspector => {
                let active_tl = self.document_store.source.document.active_timeline();
                let mut ctx = inspector::InspectorContext {
                    preview: &self.preview_store.preview,
                    timeline: active_tl,
                    composition: self.document_store.source.document.composition.as_ref(),
                    active_scene: self.document_store.source.document.active_scene.as_deref(),
                    selected_actors: self.selected_actors,
                    commands: self.commands,
                    raw_statements: self.document_store.source.document.raw_statements.as_deref(),
                    source_path: &self.document_store.source.document.file_path,
                    source_text: &self.document_store.source.document.source_text,
                    source_current: !self.preview_store.has_pending_rebuild()
                        && !self.preview_store.rebuild_in_progress
                        && self.preview_store.in_flight_rebuild.is_none()
                        && self.document_store.source.document.last_rebuild_error.is_none(),
                    scene_dimensions: self.document_store.source.document.scene_dimensions,
                };
                inspector::inspector_panel_ui(&mut ctx, ui);
            },
            WorkspaceTab::Timeline => {
                // Resolve the active timeline using the centralized API.
                let resolved_timeline = self.document_store.source.document.active_timeline();
                // Populate hot-path caches if stale (use free fn to avoid borrow conflict)
                if !self.document_store.source.cache_valid {
                    crate::app::source_store::rebuild_cache(
                        &mut self.document_store.source.cached_hit_regions,
                        &mut self.document_store.source.cached_actor_bounds,
                        &mut self.document_store.source.cache_valid,
                        resolved_timeline,
                    );
                }
                let active_scene =
                    self.document_store.source.document.active_scene.as_deref().or_else(|| {
                        self.document_store
                            .source
                            .document
                            .composition
                            .as_ref()
                            .and_then(|c| c.declaration_order.first().map(String::as_str))
                    });
                let audio_segments = self.document_store.source.document.all_audio_segments();
                let audio_revision = self
                    .document_store
                    .source
                    .document
                    .audio_waveform_revision(self.document_store.generation.0);
                let mut ctx = timeline_panel::TimelineContext {
                    source_text: &self.document_store.source.document.source_text,
                    source_current: !self.preview_store.has_pending_rebuild()
                        && !self.preview_store.rebuild_in_progress
                        && self.preview_store.in_flight_rebuild.is_none()
                        && self.document_store.source.document.last_rebuild_error.is_none(),
                    statements: self
                        .document_store
                        .source
                        .document
                        .raw_statements
                        .as_deref()
                        .unwrap_or(&[]),
                    audio_segments: &audio_segments,
                    audio_revision,
                    preview: &mut self.preview_store.preview,
                    timeline: resolved_timeline,
                    composition: self.document_store.source.document.composition.as_ref(),
                    active_scene,
                    commands: self.commands,
                    collapsed_actors: self.collapsed_actors,
                    expanded_properties: self.expanded_properties,
                    selected_actors: self.selected_actors,
                    snap_fps: self.snap_fps,
                    timeline_focused: self.timeline_focused,
                };
                timeline_panel::render_timeline_content(&mut ctx, ui);
            },
            WorkspaceTab::Curves => {
                let resolved_timeline = self.document_store.source.document.active_timeline();
                let active_scene =
                    self.document_store.source.document.active_scene.as_deref().or_else(|| {
                        self.document_store
                            .source
                            .document
                            .composition
                            .as_ref()
                            .and_then(|c| c.declaration_order.first().map(String::as_str))
                    });
                let composition = self.document_store.source.document.composition.as_ref();
                let scene =
                    composition.and_then(|c| active_scene.and_then(|name| c.scenes.get(name)));
                let mut ctx = curves_panel::CurvesContext {
                    preview: &self.preview_store.preview,
                    timeline: resolved_timeline,
                    scene_offset_s: composition
                        .and_then(|c| active_scene.and_then(|name| c.scene_start_times.get(name)))
                        .copied()
                        .unwrap_or(0.0),
                    scene_duration_s: scene
                        .map(|scene| scene.duration_s)
                        .unwrap_or(self.preview_store.preview.playback.duration_s),
                    selected_actors: self.selected_actors,
                    commands: self.commands,
                };
                curves_panel::curves_panel_ui(&mut ctx, ui);
            },
        }
        UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &WorkspaceTab) -> egui::WidgetText {
        match pane {
            WorkspaceTab::Sidebar => "Sidebar".into(),
            WorkspaceTab::Code => "Code".into(),
            WorkspaceTab::Preview => "Preview".into(),
            WorkspaceTab::Inspector => "Inspector".into(),
            WorkspaceTab::Timeline => "Timeline".into(),
            WorkspaceTab::Curves => "Curves".into(),
        }
    }

    fn simplification_options(&self) -> SimplificationOptions {
        SimplificationOptions {
            all_panes_must_have_tabs: false,
            ..Default::default()
        }
    }

    // ─── Modern Minimal Tile Styling ───────────────────────────────────────

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        1.0
    }

    /// Pixel floor for every tile: panes never collapse into unusable slivers
    /// (design doc §9 "px 兜底").
    fn min_size(&self) -> f32 {
        // The compact sidebar is intentionally a narrow rail. A larger global
        // tile floor overrides its explicit share and leaves a blank column.
        if self.compact {
            super::COMPACT_RAIL_WIDTH
        } else {
            120.0
        }
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        TIMELINE_RULER_HEIGHT
    }

    fn tab_bar_color(&self, visuals: &Visuals) -> Color32 {
        visuals.extreme_bg_color
    }

    fn tab_bg_color(
        &self,
        visuals: &Visuals,
        _tiles: &egui_tiles::Tiles<WorkspaceTab>,
        _tile_id: TileId,
        state: &egui_tiles::TabState,
    ) -> Color32 {
        if state.active {
            visuals.panel_fill
        } else {
            Color32::TRANSPARENT
        }
    }

    fn tab_outline_stroke(
        &self,
        visuals: &Visuals,
        _tiles: &egui_tiles::Tiles<WorkspaceTab>,
        _tile_id: TileId,
        state: &egui_tiles::TabState,
    ) -> Stroke {
        if state.active {
            Stroke::new(STROKE_WIDTH, visuals.widgets.noninteractive.bg_stroke.color)
        } else {
            Stroke::NONE
        }
    }

    fn tab_bar_hline_stroke(&self, visuals: &Visuals) -> Stroke {
        Stroke::new(STROKE_WIDTH, visuals.widgets.noninteractive.bg_stroke.color)
    }

    fn tab_text_color(
        &self,
        visuals: &Visuals,
        _tiles: &egui_tiles::Tiles<WorkspaceTab>,
        _tile_id: TileId,
        state: &egui_tiles::TabState,
    ) -> Color32 {
        if state.active {
            visuals.widgets.active.text_color()
        } else {
            visuals.widgets.noninteractive.text_color()
        }
    }

    fn resize_stroke(&self, style: &egui::Style, resize_state: egui_tiles::ResizeState) -> Stroke {
        let accent = style.visuals.hyperlink_color;
        match resize_state {
            egui_tiles::ResizeState::Idle => {
                Stroke::new(STROKE_WIDTH, style.visuals.widgets.noninteractive.bg_stroke.color)
            },
            egui_tiles::ResizeState::Hovering => Stroke::new(STROKE_WIDTH, accent),
            egui_tiles::ResizeState::Dragging => Stroke::new(STROKE_WIDTH, accent),
        }
    }

    fn drag_preview_stroke(&self, visuals: &Visuals) -> Stroke {
        Stroke::new(STROKE_WIDTH, visuals.hyperlink_color)
    }

    fn drag_preview_color(&self, visuals: &Visuals) -> Color32 {
        visuals.selection.bg_fill
    }

    fn paint_on_top_of_tile(
        &self,
        painter: &egui::Painter,
        style: &egui::Style,
        _tile_id: TileId,
        rect: Rect,
    ) {
        // Subtle 1px border around each tile for definition (sharp 0.0 radius for rectangular dock tiles)
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(STROKE_WIDTH, style.visuals.widgets.noninteractive.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
    }
}
