use std::collections::{HashMap, HashSet};

use egui::{Color32, Rect, Stroke, Visuals};
use egui_tiles::{Behavior, SimplificationOptions, TileId, UiResponse};

use crate::app::commands::ActionQueue;
use crate::app::design_tokens::spatial::timeline::RULER_HEIGHT as TIMELINE_RULER_HEIGHT;
use crate::app::design_tokens::spatial::{RADIUS_M, STROKE_WIDTH};
use crate::app::panels::{editor, inspector, preview_panel, sidebar, timeline_panel};
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
    pub(crate) drag_state: &'a mut preview::DragState,
    pub(crate) selection: &'a mut selection::SelectionState,
    pub(crate) pivot_offsets: &'a mut HashMap<String, [f32; 2]>,
    pub(crate) tool_mode: &'a mut preview::ToolMode,
    pub(crate) sidebar_tab: &'a mut crate::app::panels::SidebarTab,
    pub(crate) property_view_mode: &'a mut crate::app::panels::inspector::PropertyViewMode,
    pub(crate) keyframe_view_mode: &'a mut crate::app::panels::inspector::KeyframeViewMode,
    pub(crate) keyframe_mode: bool,
    pub(crate) rotation_snap_degrees: f32,
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
                let diagnostics = self.document_store.combined_diagnostics();
                let is_playing = self.preview_store.preview.playback.is_playing;
                let timeline = self.document_store.source.document.timeline.as_ref();
                let asset_cache = timeline.map(|t| t.asset_cache());
                let mut ctx = sidebar::SidebarContext {
                    active_scene: self.document_store.source.document.active_scene.as_deref(),
                    is_composition: self.document_store.source.document.is_composition(),
                    composition: self.document_store.source.document.composition.as_ref(),
                    current_file: &self.document_store.source.document.file_path,
                    expanded_dirs: &mut self.workspace_store.expanded_dirs,
                    file_tree: &self.workspace_store.file_tree,
                    preview: &mut self.preview_store.preview,
                    commands: self.commands,
                    scene_dimensions: self.document_store.source.document.scene_dimensions,
                    timeline,
                    selected_actors: self.selected_actors,
                    collapsed_actors: self.collapsed_actors,
                    sidebar_tab: self.sidebar_tab,
                    editor: &mut self.document_store.source.editor,
                    diagnostics: &diagnostics,
                    source_dirty: &mut self.document_store.source.document.source_text,
                    is_playing,
                    components: &self.document_store.source.document.components,
                    asset_cache,
                };
                sidebar::sidebar_ui(&mut ctx, ui);
            },
            WorkspaceTab::Editor => {
                // Editor is now rendered inside the Sidebar pane via the
                // Editor tab. This branch remains for backward compatibility
                // with old persisted layouts that still have an Editor pane.
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
                let mut ctx = preview_panel::PreviewContext {
                    scene_dimensions: self.document_store.source.document.scene_dimensions,
                    preview: &mut self.preview_store.preview,
                    preview_texture_id: self.preview_texture_id,
                    commands: self.commands,
                    drag_state: self.drag_state,
                    selection: self.selection,
                    selected_actors: self.selected_actors,
                    hit_regions: self.hit_regions,
                    timeline: active_tl,
                    pivot_offsets: self.pivot_offsets,
                    tool_mode: self.tool_mode,
                    rotation_snap_degrees: self.rotation_snap_degrees,
                    composition: self.document_store.source.document.composition.as_ref(),
                    active_scene: self.document_store.source.document.active_scene.as_deref(),
                    keyframe_mode: self.keyframe_mode,
                    performance_metrics: &mut self.preview_store.performance_metrics,
                    debug_layout: self.debug_layout,
                    debug_spacing: self.debug_spacing,
                    rebuild_in_progress: self.preview_store.rebuild_in_progress,
                    has_scene,
                };
                preview_panel::preview_panel_ui(&mut ctx, ui);
            },
            WorkspaceTab::Inspector => {
                let active_tl = self.document_store.source.document.active_timeline();
                let mut ctx = inspector::InspectorContext {
                    preview: &mut self.preview_store.preview,
                    timeline: active_tl,
                    composition: self.document_store.source.document.composition.as_ref(),
                    active_scene: self.document_store.source.document.active_scene.as_deref(),
                    selected_actors: self.selected_actors,
                    commands: self.commands,
                    keyframe_mode: self.keyframe_mode,
                    scene_dimensions: self.document_store.source.document.scene_dimensions,
                    pivot_offsets: self.pivot_offsets,
                    property_view_mode: self.property_view_mode,
                    keyframe_view_mode: self.keyframe_view_mode,
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
                // Build per-scene keyframe time cache from composition
                let scene_keyframe_times: HashMap<String, Vec<f64>> = self.document_store.source.document.composition
                    .as_ref()
                    .map(|comp| {
                        comp.declaration_order.iter().filter_map(|name| {
                            let scene = comp.scenes.get(name)?;
                            // Collect all keyframe times (in seconds) from all tracks in this scene
                            let mut times: Vec<f64> = scene.timeline.tracks().values()
                                .flat_map(|track| {
                                    let mut track_times = Vec::new();
                                    macro_rules! push_kf_times {
                        ($container:expr; $($field:ident),* $(,)?) => {
                            $(
                                if let Some(pt) = &$container.$field {
                                    track_times.extend(
                                        pt.keyframes().keys().map(|ms| *ms as f64 / 1000.0)
                                    );
                                }
                            )*
                        };
                    }
                    push_kf_times!(track.geometry; position, motion_offset, rotation, scale, size, layout_size, placement_mode, position_binding);
                    push_kf_times!(track.style; color, opacity, stroke_width, stroke_color, stroke_progress, fill_opacity, line_cap, line_join, morph_options);
                    push_kf_times!(track.shape; shape_type, line_from, line_to, head_size, arc_angles, points, commands, vector_paths);
                    push_kf_times!(track.text; text_content, font_family, font_size);
                    push_kf_times!(track.filter; filter_blur, filter_brightness, filter_contrast, filter_saturate, filter_hue_rotate, filter_sepia);
                                    track_times
                                })
                                .collect();
                            times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                            Some((name.clone(), times))
                        }).collect()
                    })
                    .unwrap_or_default();

                let active_scene =
                    self.document_store.source.document.active_scene.as_deref().or_else(|| {
                        self.document_store
                            .source
                            .document
                            .composition
                            .as_ref()
                            .and_then(|c| c.declaration_order.first().map(String::as_str))
                    });
                let mut ctx = timeline_panel::TimelineContext {
                    preview: &mut self.preview_store.preview,
                    timeline: resolved_timeline,
                    composition: self.document_store.source.document.composition.as_ref(),
                    active_scene,
                    commands: self.commands,
                    collapsed_actors: self.collapsed_actors,
                    expanded_properties: self.expanded_properties,
                    selected_actors: self.selected_actors,
                    scene_keyframe_times: &scene_keyframe_times,
                    snap_fps: self.snap_fps,
                    timeline_focused: self.timeline_focused,
                };
                timeline_panel::timeline_panel_ui(&mut ctx, ui);
            },
        }
        UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &WorkspaceTab) -> egui::WidgetText {
        match pane {
            WorkspaceTab::Sidebar => "Sidebar".into(),
            WorkspaceTab::Editor => "Editor".into(),
            WorkspaceTab::Preview => "Preview".into(),
            WorkspaceTab::Inspector => "Inspector".into(),
            WorkspaceTab::Timeline => "Timeline".into(),
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
        // Subtle 1px border around each tile for definition
        painter.rect_stroke(
            rect,
            RADIUS_M,
            Stroke::new(STROKE_WIDTH, style.visuals.widgets.noninteractive.bg_stroke.color),
            egui::StrokeKind::Inside,
        );
    }
}
