#![allow(dead_code)]

pub mod behavior;
pub mod inspector;

fn default_actor_type() -> &'static str {
    animatix::primitives::actor_kind_registry()
        .iter()
        .find(|meta| {
            meta.category == animatix::timeline::ActorCategory::Shape && !meta.advanced
        })
        .map(|meta| meta.type_name)
        .unwrap_or("Rect")
}

use crate::app::components;
use crate::app::components::widgets;
use crate::app::icons::actor_icon_str;
use crate::app::theme::*;
use crate::app::preview::{self, selection, ActorProps, DragState, fit_preview};
use crate::app::{FileTreeEntry, PreviewPaneState};
use crate::editor::EditorBuffer;
use animatix::diagnostics::Diagnostic;
use animatix::timeline::{PositionBinding, SceneDimensions, Timeline, TrackAccessor};
use egui::{Color32, Pos2, RichText, Stroke, Vec2};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SidebarTab {
    Explorer,
    Layers,
    Scenes,
}

/// Describes a property edit made in the inspector panel.
#[derive(Debug, Clone)]
pub(super) struct PropertyEdit {
    pub(super) actor: String,
    pub(super) property: String,
    pub(super) value: PropertyValue,
    /// When true, create a keyframe at current time instead of overwriting defaults.
    pub(super) create_keyframe: bool,
}

/// The typed value of a property edit.
#[derive(Debug, Clone)]
pub(crate) enum PropertyValue {
    Vec2([f32; 2]),
    Float(f32),
    Color([f32; 4]),
    Text(String),
    StringList(Vec<String>),
}

impl From<PropertyValue> for animatix::ast::Expr {
    fn from(pv: PropertyValue) -> Self {
        match pv {
            PropertyValue::Vec2([x, y]) => {
                animatix::ast::Expr::Tuple(vec![
                    animatix::ast::Expr::Num(x as f64),
                    animatix::ast::Expr::Num(y as f64),
                ])
            }
            PropertyValue::Float(v) => animatix::ast::Expr::Num(v as f64),
            PropertyValue::Color([r, g, b, a]) => {
                if (a - 1.0).abs() < 0.001
                    && r.fract() == 0.0
                    && g.fract() == 0.0
                    && b.fract() == 0.0
                {
                    // Opaque integer color — use rgb() shorthand.
                    animatix::ast::Expr::Call(
                        "rgb".into(),
                        vec![
                            animatix::ast::Expr::Num((r * 255.0) as i64 as f64),
                            animatix::ast::Expr::Num((g * 255.0) as i64 as f64),
                            animatix::ast::Expr::Num((b * 255.0) as i64 as f64),
                        ],
                    )
                } else {
                    animatix::ast::Expr::Call(
                        "rgba".into(),
                        vec![
                            animatix::ast::Expr::Num(r as f64),
                            animatix::ast::Expr::Num(g as f64),
                            animatix::ast::Expr::Num(b as f64),
                            animatix::ast::Expr::Num(a as f64),
                        ],
                    )
                }
            }
            PropertyValue::Text(s) => animatix::ast::Expr::Str(s),
            PropertyValue::StringList(items) => {
                animatix::ast::Expr::Tuple(items.into_iter().map(animatix::ast::Expr::Ident).collect())
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct UiActions {
    pub(super) open_file: Option<PathBuf>,
    pub(super) toggle_expand_dir: Option<PathBuf>,
    pub(super) show_inspector: bool,
    pub(super) save: bool,
    pub(super) reload: bool,
    pub(super) rebuild: bool,
    pub(super) toggle_playback: bool,
    pub(super) scrub_to: Option<f64>,
    pub(super) editor_changed: bool,
    pub(super) request_repaint: bool,
    pub(super) prev_keyframe: bool,
    pub(super) next_keyframe: bool,
    pub(super) prev_scene: bool,
    pub(super) next_scene: bool,
    pub(super) select_actor: Option<String>,
    pub(super) select_scene: Option<String>,
    pub(super) add_scene: bool,
    pub(super) rename_scene: Option<(String, String)>,
    pub(super) reorder_scenes: Option<Vec<String>>,
    pub(super) property_edits: Vec<PropertyEdit>,
    pub(super) drag_ended: bool,
    pub(super) undo: bool,
    pub(super) redo: bool,
    pub(super) toggle_editor_sync: bool,
    pub(super) toggle_keyframe_mode: bool,
    /// Scroll editor to this 0-indexed line (set by clicking a diagnostic).
    pub(super) scroll_to_line: Option<usize>,
    /// True when an inspector DragValue/Slider drag started this frame.
    pub(super) inspector_input_drag_started: bool,
    /// True when an inspector DragValue/Slider drag ended this frame.
    pub(super) inspector_input_drag_ended: bool,
    /// Toggle the bottom diagnostics panel visibility.
    pub(super) toggle_diagnostics_panel: bool,
    /// Create a new actor: (type, label, position_in_scene)
    pub(super) create_actor: Option<(String, String, [f32; 2])>,
    /// Rename an actor: (old_label, new_label)
    pub(super) rename_actor: Option<(String, String)>,
    /// Open the export dialog.
    pub(super) open_export_dialog: bool,
}

pub(super) struct WorkspaceViewer<'a> {
    pub(super) scene_names: Vec<String>,
    pub(super) import_aliases: Vec<String>,
    pub(super) active_scene: Option<String>,
    pub(super) is_composition: bool,
    pub(super) current_file: &'a Path,
    pub(super) workspace_root: &'a Path,
    pub(super) expanded_dirs: &'a mut HashSet<PathBuf>,
    pub(super) file_tree: &'a [FileTreeEntry],
    pub(super) editor: &'a mut EditorBuffer,
    pub(super) preview: &'a mut PreviewPaneState,
    pub(super) diagnostics: &'a [Diagnostic],
    pub(super) preview_texture_id: Option<egui::TextureId>,
    pub(super) actions: &'a mut UiActions,
    pub(super) source_dirty: &'a mut String,
    pub(super) scene_dimensions: SceneDimensions,
    pub(super) timeline: Option<&'a Timeline>,
    pub(super) selected_actor: &'a mut Option<String>,
    pub(super) hit_regions: &'a [(String, kurbo::Rect)],
    pub(super) drag_state: &'a mut DragState,
    pub(super) selection: &'a mut selection::SelectionState,
    /// When true, property edits should create keyframes instead of overwriting defaults.
    pub(super) keyframe_mode: bool,
    /// Actor labels that the user has explicitly collapsed in the layer tree.
    pub(super) collapsed_actors: &'a mut HashSet<String>,
}

/// Uniform panel frame: 8 px padding, transparent fill.
fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::same(8))
}

fn render_sidebar_tab_bar(ui: &mut egui::Ui, active_tab: &mut SidebarTab) {
    let tabs = [
        (SidebarTab::Explorer, egui_phosphor::regular::FOLDER, "Explorer"),
        (SidebarTab::Layers, egui_phosphor::regular::STACK, "Layers"),
        (SidebarTab::Scenes, egui_phosphor::regular::FILM_STRIP, "Scenes"),
    ];
    if let Some(new_tab) = widgets::pill_tab_bar(ui, *active_tab, &tabs) {
        *active_tab = new_tab;
    }
}

fn preview_screen_to_scene(
    scene_dimensions: SceneDimensions,
    preview_rect: egui::Rect,
    screen: egui::Pos2,
) -> kurbo::Point {
    let desired = preview_rect.size();
    let scale_x = scene_dimensions.width as f64 / desired.x.max(1.0) as f64;
    let scale_y = scene_dimensions.height as f64 / desired.y.max(1.0) as f64;

    kurbo::Point::new(
        (screen.x - preview_rect.min.x) as f64 * scale_x,
        (screen.y - preview_rect.min.y) as f64 * scale_y,
    )
}

fn preview_scene_to_screen(
    scene_dimensions: SceneDimensions,
    preview_rect: egui::Rect,
    scene: kurbo::Point,
) -> egui::Pos2 {
    let desired = preview_rect.size();
    let scale_x = scene_dimensions.width as f64 / desired.x.max(1.0) as f64;
    let scale_y = scene_dimensions.height as f64 / desired.y.max(1.0) as f64;

    Pos2::new(
        (preview_rect.min.x as f64 + scene.x / scale_x) as f32,
        (preview_rect.min.y as f64 + scene.y / scale_y) as f32,
    )
}

impl WorkspaceViewer<'_> {
    fn get_actor_props(&self, actor: &str) -> Option<ActorProps> {
        let timeline = self.timeline?;
        let track = timeline.get_track(actor)?;
        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
        let half = track.size.as_ref().map(|pt| pt.evaluate(time_ms))?;
        let local_size = [half[0] * 2.0, half[1] * 2.0];
        let world_affine = timeline.actor_world_affine(actor, time_ms, self.scene_dimensions)?;
        let coeffs = world_affine.as_coeffs();
        let position = [coeffs[4] as f32, coeffs[5] as f32];
        let rotation = (coeffs[1] as f32).atan2(coeffs[0] as f32);
        let scale = ((coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt()) as f32;
        let size = [local_size[0] * scale, local_size[1] * scale];
        Some(ActorProps { position, size, rotation })
    }

    fn is_layout_managed(&self, actor: &str) -> bool {
        let Some(timeline) = self.timeline else { return false; };
        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
        preview::is_layout_managed(actor, timeline, time_ms)
    }

    fn find_layout_container(&self, actor: &str) -> Option<(String, animatix::timeline::LayoutType, usize)> {
        let timeline = self.timeline?;
        let container = timeline
            .tracks
            .iter()
            .find(|(_, track)| track.children.iter().any(|child| child == actor))?
            .0
            .clone();
        let metadata = timeline.container_metadata.get(&container)?;
        let source_index = timeline
            .get_track(&container)?
            .children
            .iter()
            .position(|child| child == actor)?;
        Some((container, metadata.layout_type, source_index))
    }

    pub(super) fn sidebar_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            let tab_id = ui.id().with("sidebar_tab");
            let mut active_tab = ui
                .data(|d| d.get_temp::<SidebarTab>(tab_id))
                .unwrap_or(SidebarTab::Explorer);

            render_sidebar_tab_bar(ui, &mut active_tab);
            ui.add_space(6.0);

            ui.allocate_ui_with_layout(
                ui.available_size(),
                egui::Layout::top_down(egui::Align::Min),
                |ui| match active_tab {
                    SidebarTab::Explorer => self.explorer_content_ui(ui),
                    SidebarTab::Layers => self.layers_content_ui(ui),
                    SidebarTab::Scenes => self.scenes_content_ui(ui),
                },
            );

            ui.data_mut(|d| d.insert_temp(tab_id, active_tab));
        });
    }

    fn explorer_content_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
            for entry in self.file_tree {
                let is_selected = !entry.is_dir && entry.path == self.current_file;
                let is_expanded = entry.is_dir && self.expanded_dirs.contains(&entry.path);
                let has_children = entry.is_dir;

                let (icon, label_color) = if entry.is_dir {
                    let folder_icon = if is_expanded {
                        egui_phosphor::regular::FOLDER_OPEN
                    } else {
                        egui_phosphor::regular::FOLDER
                    };
                    (Some(folder_icon), None)
                } else {
                    let is_amx = entry.path.extension().and_then(|e| e.to_str()) == Some("amx");
                    let file_icon = if is_amx {
                        egui_phosphor::regular::FILM_STRIP
                    } else {
                        egui_phosphor::regular::FILE
                    };
                    let color = if is_amx { Some(ACCENT_BLUE) } else { None };
                    (Some(file_icon), color)
                };

                let row_id = ui.id().with(entry.path.display().to_string());
                let path = entry.path.clone();
                let is_dir = entry.is_dir;
                let clicked = widgets::tree_row(
                    ui,
                    row_id,
                    entry.depth,
                    has_children,
                    is_expanded,
                    is_selected,
                    icon,
                    &entry.name,
                    label_color,
                    || {
                        self.actions.toggle_expand_dir = Some(path.clone());
                    },
                );

                if clicked {
                    if is_dir {
                        self.actions.toggle_expand_dir = Some(path);
                    } else {
                        self.actions.open_file = Some(path);
                    }
                }
            }
        });
    }

    fn layers_content_ui(&mut self, ui: &mut egui::Ui) {
        let Some(timeline) = self.timeline else {
            ui.vertical_centered(|ui| {
                ui.add_space(SPACE_XL * 3.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(egui_phosphor::regular::FILM_STRIP)
                            .size(ROW_L)
                            .color(TEXT_MUTED),
                    )
                    .selectable(false),
                );
                ui.add_space(SPACE_M);
                ui.add(
                    egui::Label::new(
                        RichText::new("No timeline loaded")
                            .size(FONT_SIZE_L)
                            .color(TEXT_SECONDARY),
                    )
                    .selectable(false),
                );
            });
            return;
        };

        let root_nodes = timeline.root_actor_labels();
        if root_nodes.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(SPACE_XL * 3.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(egui_phosphor::regular::FILM_STRIP)
                            .size(ROW_L)
                            .color(TEXT_MUTED),
                    )
                    .selectable(false),
                );
                ui.add_space(SPACE_M);
                ui.add(
                    egui::Label::new(
                        RichText::new("No actors in scene")
                            .size(FONT_SIZE_L)
                            .color(TEXT_SECONDARY),
                    )
                    .selectable(false),
                );
                ui.add_space(12.0);
                if ui
                    .button(
                        RichText::new(format!("{} Add Actor", egui_phosphor::regular::PLUS))
                            .size(12.0)
                            .color(ACCENT_BLUE),
                    )
                    .clicked()
                {
                    let label = format!("rect1");
                    let pos = [
                        self.scene_dimensions.width as f32 / 2.0,
                        self.scene_dimensions.height as f32 / 2.0,
                    ];
                    self.actions.create_actor = Some((default_actor_type().into(), label, pos));
                }
            });
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for root_label in root_nodes {
                    render_actor_tree(
                        ui,
                        timeline,
                        root_label,
                        self.selected_actor,
                        self.collapsed_actors,
                        0,
                    );
                }
            });
    }

    fn scenes_content_ui(&mut self, ui: &mut egui::Ui) {
        if !self.is_composition {
            ui.vertical_centered(|ui| {
                ui.add_space(SPACE_XL * 3.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(egui_phosphor::regular::FILM_STRIP)
                            .size(ROW_L)
                            .color(TEXT_MUTED),
                    )
                    .selectable(false),
                );
                ui.add_space(SPACE_M);
                ui.add(
                    egui::Label::new(
                        RichText::new("No scenes — this is a single-scene file")
                            .size(FONT_SIZE_M)
                            .color(TEXT_SECONDARY),
                    )
                    .selectable(false),
                );
            });
            return;
        }

        ui.vertical(|ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);

                for scene_name in self.scene_names.clone() {
                    let is_active = self.active_scene.as_deref() == Some(scene_name.as_str());
                    let row_id = ui.id().with(&scene_name);
                    let edit_id = row_id.with("scene_name_edit");
                    let mut is_editing = ui.data(|d| d.get_temp::<bool>(edit_id)).unwrap_or(false);
                    let mut edit_buffer = ui
                        .data(|d| d.get_temp::<String>(edit_id.with("buf")))
                        .unwrap_or_else(|| scene_name.clone());

                    if is_editing {
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut edit_buffer)
                                .desired_width(ui.available_width())
                                .font(egui::TextStyle::Body),
                        );
                        let commit = response.lost_focus()
                            || ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if commit {
                            ui.data_mut(|d| d.insert_temp(edit_id, false));
                            if edit_buffer != scene_name && !edit_buffer.is_empty() {
                                self.actions.rename_scene = Some((scene_name.clone(), edit_buffer.clone()));
                            }
                            is_editing = false;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            ui.data_mut(|d| d.insert_temp(edit_id, false));
                            edit_buffer = scene_name.clone();
                            is_editing = false;
                        }
                        ui.data_mut(|d| d.insert_temp(edit_id.with("buf"), edit_buffer));
                    } else {
                        let response = components::Row::new(&scene_name)
                            .selected(is_active)
                            .show(ui, row_id);

                        if response.row_double_clicked || response.row_secondary_clicked {
                            ui.data_mut(|d| {
                                d.insert_temp(edit_id, true);
                                d.insert_temp(edit_id.with("buf"), scene_name.clone());
                            });
                        } else if response.row_clicked {
                            self.actions.select_scene = Some(scene_name);
                        }
                    }
                }
            });

            if !self.import_aliases.is_empty() {
                ui.add_space(16.0);
                ui.label(RichText::new("Imports").size(FONT_SIZE_S).color(TEXT_MUTED));
                ui.separator();

                for alias in &self.import_aliases {
                    let row_id = ui.id().with(format!("import_{}", alias));
                    let _response = components::Row::new(alias)
                        .icon(Some(egui_phosphor::regular::FILE_ARROW_DOWN))
                        .show(ui, row_id);
                }
            }

            ui.add_space(8.0);
            if ui.button("+ Add Scene").clicked() {
                self.actions.add_scene = true;
            }
        });
    }

    pub(super) fn editor_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            self.editor.set_diagnostics(self.diagnostics);
            let response = self.editor.show(ui);
            if response.changed() || self.editor.text() != self.source_dirty.as_str() {
                *self.source_dirty = self.editor.text().to_string();
                self.actions.editor_changed = true;
            }
            if let Some(time_s) = self.editor.pending_scrub_to_time.take() {
                self.actions.scrub_to = Some(time_s);
                if !self.preview.is_playing {
                    self.actions.toggle_playback = true;
                }
            }
            if let Some(line) = self.actions.scroll_to_line.take() {
                self.editor.scroll_to_line(line);
            }
        });
    }

    fn preview_screen_to_scene(&self, preview_rect: egui::Rect, screen: egui::Pos2) -> kurbo::Point {
        preview_screen_to_scene(self.scene_dimensions, preview_rect, screen)
    }

    fn preview_scene_to_screen(&self, preview_rect: egui::Rect, scene: kurbo::Point) -> egui::Pos2 {
        preview_scene_to_screen(self.scene_dimensions, preview_rect, scene)
    }

    /// Handle drag start/update/end for the preview.
    fn handle_preview_drag(
        &mut self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        response: &egui::Response,
    ) -> bool {
        let is_dragging = !matches!(self.drag_state, DragState::None);
        let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());
        let drag_started = response.drag_started()
            || (!is_dragging && ui.input(|i| i.pointer.primary_pressed()));

        if drag_started {
            if let (Some(actor), Some(mouse)) = (self.selected_actor.clone(), raw_pointer_pos) {
                let scene = self.preview_screen_to_scene(preview_rect, mouse);
                let props = self.get_actor_props(&actor);

                if let Some(ref p) = props {
                    let handle_world = preview::world_handle_positions(p);
                    let handle_screen: [Pos2; 8] =
                        std::array::from_fn(|i| self.preview_scene_to_screen(preview_rect, handle_world[i]));
                    if let Some(idx) = preview::hit_test_handle(mouse, &handle_screen) {
                        let anchor_local = preview::handle_anchor_local(idx, p.size);
                        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                        let (resize_mode, start_scale) = self
                            .timeline
                            .and_then(|t| t.get_track(&actor))
                            .map(|tr| {
                                let mode = if let Some(primitive) = animatix::primitives::find_primitive(
                                    animatix::timeline::actor_kind_meta(tr.kind).type_name,
                                ) {
                                    match primitive.resize_mode() {
                                        animatix::timeline::ResizeMode::Scale => preview::ResizeMode::Scale,
                                        _ => preview::ResizeMode::Size,
                                    }
                                } else {
                                    preview::ResizeMode::Size
                                };
                                (mode, tr.scale.get(time_ms, 1.0))
                            })
                            .unwrap_or((preview::ResizeMode::Size, 1.0));
                        *self.drag_state = DragState::Scale {
                            actor,
                            handle: idx,
                            start_scene: scene,
                            start_position: p.position,
                            start_size: p.size,
                            start_rotation: p.rotation,
                            anchor_local,
                            constrain_axis: preview::handle_constrains_axis(idx),
                            uniform_ratio: ui.input(|i| i.modifiers.shift),
                            resize_mode,
                            start_scale,
                        };
                        return true;
                    }

                    let rot_world = preview::rotation_handle_world(p);
                    let rot_screen = self.preview_scene_to_screen(preview_rect, rot_world);
                    if preview::hit_test_rotation_handle(mouse, rot_screen) {
                        let center = [p.position[0], p.position[1]];
                        let angle = ((scene.y - center[1] as f64) as f32)
                            .atan2((scene.x - center[0] as f64) as f32);
                        *self.drag_state = DragState::Rotate {
                            actor,
                            start_angle: angle,
                            start_rotation: p.rotation,
                            center,
                        };
                        return true;
                    }
                }

                let hit_body = props
                    .map(|p| {
                        let local_pt = [
                            (scene.x - p.position[0] as f64) as f32,
                            (scene.y - p.position[1] as f64) as f32,
                        ];
                        let cos = (-p.rotation).cos();
                        let sin = (-p.rotation).sin();
                        let lx = local_pt[0] * cos - local_pt[1] * sin;
                        let ly = local_pt[0] * sin + local_pt[1] * cos;
                        let hw = p.size[0] / 2.0;
                        let hh = p.size[1] / 2.0;
                        lx.abs() <= hw && ly.abs() <= hh
                    })
                    .unwrap_or(false);

                if hit_body
                    || self
                        .hit_regions
                        .iter()
                        .rev()
                        .any(|(label, bounds)| label == &actor && bounds.contains(scene))
                {
                    if self.is_layout_managed(&actor) {
                        if let Some((container, layout_type, source_index)) = self.find_layout_container(&actor) {
                            *self.drag_state = DragState::Reorder {
                                actor,
                                container,
                                source_index,
                                target_index: source_index,
                                start_mouse: scene,
                                layout_type,
                            };
                            return true;
                        }
                        return true;
                    }

                    let start_position = if let Some(ref p) = props {
                        p.position
                    } else {
                        self.hit_regions
                            .iter()
                            .find(|(l, _)| l == &actor)
                            .map(|(_, r)| [(r.x0 + r.x1) as f32 / 2.0, (r.y0 + r.y1) as f32 / 2.0])
                            .unwrap_or([0.0, 0.0])
                    };
                    *self.drag_state = DragState::Move {
                        actor,
                        start_scene: scene,
                        start_position,
                    };
                }
            }
        }

        if !is_dragging {
            // nothing — handled by match arms below
        } else if let Some(mouse) = raw_pointer_pos {
            let scene = self.preview_screen_to_scene(preview_rect, mouse);
            let shift = ui.input(|i| i.modifiers.shift);

            match self.drag_state.clone() {
                DragState::Move {
                    actor,
                    start_scene,
                    start_position,
                } => {
                    let dx = (scene.x - start_scene.x) as f32;
                    let dy = (scene.y - start_scene.y) as f32;
                    let (nx, ny) = if shift {
                        if dx.abs() > dy.abs() {
                            (start_position[0] + dx, start_position[1])
                        } else {
                            (start_position[0], start_position[1] + dy)
                        }
                    } else {
                        (start_position[0] + dx, start_position[1] + dy)
                    };

                    let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                    let binding = self
                        .timeline
                        .and_then(|t| t.get_track(&actor))
                        .map(|tr| tr.position_binding.get(time_ms, PositionBinding::Absolute))
                        .unwrap_or(PositionBinding::Absolute);

                    match binding {
                        PositionBinding::SceneAnchor { anchor, .. } => {
                            let anchor_pt = animatix::timeline::scene_anchor_point(anchor, self.scene_dimensions);
                            let new_offset = [nx - anchor_pt.x as f32, ny - anchor_pt.y as f32];
                            self.actions.property_edits.push(PropertyEdit {
                                actor,
                                property: "offset".into(),
                                value: PropertyValue::Vec2(new_offset),
                                create_keyframe: self.keyframe_mode,
                            });
                        }
                        PositionBinding::ScenePercent { .. } => {
                            let w = self.scene_dimensions.width.max(1) as f32;
                            let h = self.scene_dimensions.height.max(1) as f32;
                            self.actions.property_edits.push(PropertyEdit {
                                actor,
                                property: "at".into(),
                                value: PropertyValue::Vec2([nx / w, ny / h]),
                                create_keyframe: self.keyframe_mode,
                            });
                        }
                        _ => {
                            self.actions.property_edits.push(PropertyEdit {
                                actor,
                                property: "position".into(),
                                value: PropertyValue::Vec2([nx, ny]),
                                create_keyframe: self.keyframe_mode,
                            });
                        }
                    }
                }
                DragState::Scale {
                    actor,
                    handle,
                    start_scene,
                    start_position,
                    start_size,
                    start_rotation,
                    anchor_local,
                    constrain_axis,
                    uniform_ratio,
                    resize_mode,
                    start_scale,
                } => {
                    let dx_world = (scene.x - start_scene.x) as f32;
                    let dy_world = (scene.y - start_scene.y) as f32;
                    let cos = (-start_rotation).cos();
                    let sin = (-start_rotation).sin();
                    let dx_local = dx_world * cos - dy_world * sin;
                    let dy_local = dx_world * sin + dy_world * cos;

                    let sign = match handle {
                        0 => [-1.0_f32, -1.0],
                        1 => [1.0, -1.0],
                        2 => [1.0, 1.0],
                        3 => [-1.0, 1.0],
                        4 => [0.0, -1.0],
                        5 => [1.0, 0.0],
                        6 => [0.0, 1.0],
                        7 => [-1.0, 0.0],
                        _ => [1.0, 1.0],
                    };

                    let min_size = 10.0;
                    let mut new_w = start_size[0];
                    let mut new_h = start_size[1];

                    if sign[0] != 0.0 {
                        new_w = (start_size[0] + sign[0] * dx_local).max(min_size);
                    }
                    if sign[1] != 0.0 {
                        new_h = (start_size[1] + sign[1] * dy_local).max(min_size);
                    }

                    let force_uniform = resize_mode == preview::ResizeMode::Scale;
                    let uniform = shift || uniform_ratio || force_uniform;
                    if uniform {
                        let scale_w = new_w / start_size[0].max(1.0);
                        let scale_h = new_h / start_size[1].max(1.0);
                        let s = if constrain_axis && !force_uniform {
                            if sign[0] == 0.0 { scale_h } else { scale_w }
                        } else {
                            scale_w.max(scale_h)
                        };
                        new_w = (start_size[0] * s).max(min_size);
                        new_h = (start_size[1] * s).max(min_size);
                    }

                    let cos_rot = start_rotation.cos();
                    let sin_rot = start_rotation.sin();
                    let old_anchor_local = [anchor_local[0], anchor_local[1]];
                    let new_anchor_local = [
                        old_anchor_local[0] * new_w / start_size[0].max(1.0),
                        old_anchor_local[1] * new_h / start_size[1].max(1.0),
                    ];

                    let anchor_world_x = start_position[0]
                        + old_anchor_local[0] * cos_rot
                        - old_anchor_local[1] * sin_rot;
                    let anchor_world_y = start_position[1]
                        + old_anchor_local[0] * sin_rot
                        + old_anchor_local[1] * cos_rot;

                    let new_pos_x = anchor_world_x
                        - new_anchor_local[0] * cos_rot
                        + new_anchor_local[1] * sin_rot;
                    let new_pos_y = anchor_world_y
                        - new_anchor_local[0] * sin_rot
                        - new_anchor_local[1] * cos_rot;

                    if resize_mode == preview::ResizeMode::Scale {
                        let ratio = new_w / start_size[0].max(1.0);
                        let new_scale = (start_scale * ratio).max(0.01);
                        self.actions.property_edits.push(PropertyEdit {
                            actor: actor.clone(),
                            property: "scale".into(),
                            value: PropertyValue::Float(new_scale),
                            create_keyframe: self.keyframe_mode,
                        });
                    } else {
                        self.actions.property_edits.push(PropertyEdit {
                            actor: actor.clone(),
                            property: "size".into(),
                            value: PropertyValue::Vec2([new_w, new_h]),
                            create_keyframe: self.keyframe_mode,
                        });
                    }

                    let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                    let binding = self
                        .timeline
                        .and_then(|t| t.get_track(&actor))
                        .map(|tr| tr.position_binding.get(time_ms, PositionBinding::Absolute))
                        .unwrap_or(PositionBinding::Absolute);

                    match binding {
                        PositionBinding::SceneAnchor { anchor, .. } => {
                            let anchor_pt = animatix::timeline::scene_anchor_point(anchor, self.scene_dimensions);
                            let new_offset = [new_pos_x - anchor_pt.x as f32, new_pos_y - anchor_pt.y as f32];
                            self.actions.property_edits.push(PropertyEdit {
                                actor,
                                property: "offset".into(),
                                value: PropertyValue::Vec2(new_offset),
                                create_keyframe: self.keyframe_mode,
                            });
                        }
                        PositionBinding::ScenePercent { .. } => {
                            let w = self.scene_dimensions.width.max(1) as f32;
                            let h = self.scene_dimensions.height.max(1) as f32;
                            self.actions.property_edits.push(PropertyEdit {
                                actor,
                                property: "at".into(),
                                value: PropertyValue::Vec2([new_pos_x / w, new_pos_y / h]),
                                create_keyframe: self.keyframe_mode,
                            });
                        }
                        _ => {
                            self.actions.property_edits.push(PropertyEdit {
                                actor,
                                property: "position".into(),
                                value: PropertyValue::Vec2([new_pos_x, new_pos_y]),
                                create_keyframe: self.keyframe_mode,
                            });
                        }
                    }
                }
                DragState::Rotate {
                    actor,
                    start_angle,
                    start_rotation,
                    center,
                } => {
                    let angle = ((scene.y - center[1] as f64) as f32)
                        .atan2((scene.x - center[0] as f64) as f32);
                    let mut delta = angle - start_angle;
                    while delta > std::f32::consts::PI {
                        delta -= 2.0 * std::f32::consts::PI;
                    }
                    while delta < -std::f32::consts::PI {
                        delta += 2.0 * std::f32::consts::PI;
                    }
                    let mut new_rot = start_rotation + delta;
                    if shift {
                        let step = std::f32::consts::PI / 12.0;
                        new_rot = (new_rot / step).round() * step;
                    }
                    self.actions.property_edits.push(PropertyEdit {
                        actor,
                        property: "rotation".into(),
                        value: PropertyValue::Float(new_rot),
                        create_keyframe: self.keyframe_mode,
                    });
                }
                DragState::Reorder {
                    actor,
                    container,
                    source_index: _,
                    target_index: _,
                    start_mouse: _,
                    layout_type,
                } => {
                    let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                    if let Some(timeline) = self.timeline {
                        let order = timeline.get_child_order(&container, time_ms);
                        let siblings: Vec<String> = order.into_iter().filter(|l| l != &actor).collect();
                        let positions: Vec<f32> = siblings
                            .iter()
                            .map(|label| {
                                self.hit_regions
                                    .iter()
                                    .find(|(l, _)| l == label)
                                    .map(|(_, bounds)| {
                                        if layout_type == animatix::timeline::LayoutType::Row {
                                            (bounds.x0 + bounds.x1) as f32 / 2.0
                                        } else {
                                            (bounds.y0 + bounds.y1) as f32 / 2.0
                                        }
                                    })
                                    .or_else(|| {
                                        self.get_actor_props(label).map(|p| {
                                            if layout_type == animatix::timeline::LayoutType::Row {
                                                p.position[0]
                                            } else {
                                                p.position[1]
                                            }
                                        })
                                    })
                                    .unwrap_or(if layout_type == animatix::timeline::LayoutType::Row {
                                        scene.x as f32
                                    } else {
                                        scene.y as f32
                                    })
                            })
                            .collect();

                        let mouse_coord = if layout_type == animatix::timeline::LayoutType::Row {
                            scene.x as f32
                        } else {
                            scene.y as f32
                        };

                        let mut insert_at = positions.len();
                        for (idx, coord) in positions.iter().enumerate() {
                            if mouse_coord < *coord {
                                insert_at = idx;
                                break;
                            }
                        }
                        if let DragState::Reorder { target_index, .. } = &mut *self.drag_state {
                            *target_index = insert_at;
                        }
                    }
                }
                DragState::None => {}
            }
        }

        let pointer_released = ui.input(|i| i.pointer.any_released());
        if is_dragging
            && (response.drag_stopped() || pointer_released || (!ui.input(|i| i.pointer.any_down()) && is_dragging))
        {
            if let DragState::Reorder {
                actor,
                container,
                source_index,
                target_index,
                ..
            } = self.drag_state.clone()
            {
                if source_index != target_index {
                    let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                    if let Some(timeline) = self.timeline {
                        let mut new_order = timeline.get_child_order(&container, time_ms);
                        if let Some(pos) = new_order.iter().position(|label| label == &actor) {
                            let item = new_order.remove(pos);
                            let insert_at = target_index.min(new_order.len());
                            new_order.insert(insert_at, item);
                            self.actions.property_edits.push(PropertyEdit {
                                actor: container,
                                property: "child_order".into(),
                                value: PropertyValue::StringList(new_order),
                                create_keyframe: self.keyframe_mode,
                            });
                        }
                    }
                }
            }
            self.actions.drag_ended = true;
        }

        false
    }

    /// Handle click-to-select in the preview.
    fn handle_preview_selection(
        &mut self,
        ui: &mut egui::Ui,
        _preview_rect: egui::Rect,
        response: &egui::Response,
    ) {
        let is_dragging = !matches!(self.drag_state, DragState::None);

        if response.secondary_clicked() && !is_dragging {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let scene_dimensions = self.scene_dimensions;
                selection::handle_right_click(
                    self.selection,
                    self.hit_regions,
                    click_pos,
                    move |screen| preview_screen_to_scene(scene_dimensions, _preview_rect, screen),
                );
            }
        }

        let mut menu_item_clicked = false;
        if self.selection.context_menu_open {
            let (selected, close, _rect) = selection::draw_context_menu(
                ui,
                self.selection,
                self.selected_actor,
            );
            menu_item_clicked = close;
            if let Some(actor) = selected {
                self.actions.select_actor = Some(actor);
            }
            if close {
                self.selection.context_menu_open = false;
            }
        }

        let mut suppress_click = false;
        if self.selection.context_menu_open && !menu_item_clicked {
            if ui.input(|i| i.pointer.primary_clicked()) {
                self.selection.context_menu_open = false;
                suppress_click = true;
                *self.selected_actor = None;
            }
        }

        if response.clicked() && !is_dragging && !self.selection.context_menu_open && !suppress_click {
            if let Some(click_pos) = response.interact_pointer_pos() {
                let scene_dimensions = self.scene_dimensions;
                let selected = selection::handle_click(
                    self.selection,
                    self.hit_regions,
                    click_pos,
                    move |screen| preview_screen_to_scene(scene_dimensions, _preview_rect, screen),
                );
                if let Some(actor) = selected {
                    self.actions.select_actor = Some(actor);
                } else {
                    *self.selected_actor = None;
                }
            }
        }
    }

    /// Render cursor feedback for the preview.
    fn render_preview_cursor_feedback(&self, ui: &egui::Ui, preview_rect: egui::Rect) {
        let is_dragging = !matches!(self.drag_state, DragState::None);
        let raw_pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());

        if !is_dragging && !self.selection.context_menu_open {
            if let Some(mouse) = raw_pointer_pos {
                let scene = self.preview_screen_to_scene(preview_rect, mouse);

                let over_handle = self.selected_actor.as_ref().and_then(|a| {
                    let props = self.get_actor_props(a)?;
                    let handle_world = preview::world_handle_positions(&props);
                    let handle_screen: [Pos2; 8] =
                        std::array::from_fn(|i| self.preview_scene_to_screen(preview_rect, handle_world[i]));
                    if let Some(idx) = preview::hit_test_handle(mouse, &handle_screen) {
                        Some(idx)
                    } else {
                        let rot_world = preview::rotation_handle_world(&props);
                        let rot_screen = self.preview_scene_to_screen(preview_rect, rot_world);
                        if preview::hit_test_rotation_handle(mouse, rot_screen) {
                            Some(8usize)
                        } else {
                            None
                        }
                    }
                });

                if let Some(handle_idx) = over_handle {
                    let icon = match handle_idx {
                        0 => egui::CursorIcon::ResizeNwSe,
                        1 => egui::CursorIcon::ResizeNeSw,
                        2 => egui::CursorIcon::ResizeNwSe,
                        3 => egui::CursorIcon::ResizeNeSw,
                        4 => egui::CursorIcon::ResizeVertical,
                        5 => egui::CursorIcon::ResizeHorizontal,
                        6 => egui::CursorIcon::ResizeVertical,
                        7 => egui::CursorIcon::ResizeHorizontal,
                        _ => egui::CursorIcon::Crosshair,
                    };
                    ui.ctx().set_cursor_icon(icon);
                } else {
                    let is_over_selected = self
                        .selected_actor
                        .as_ref()
                        .and_then(|a| {
                            self.hit_regions
                                .iter()
                                .find(|(l, _)| l == a)
                                .map(|(_, b)| b.contains(scene))
                        })
                        .unwrap_or(false);

                    if is_over_selected {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                    } else if self.selection.hovered_actor.is_some() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                }
            }
        } else if !self.selection.context_menu_open {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
    }

    /// Render hover highlight and cycle indicator overlays.
    fn render_preview_overlays(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
    ) {
        if self.selection.context_menu_open {
            return;
        }

        let pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos()).filter(|p| preview_rect.contains(*p));
        if let Some(hovered) = self.selection.hovered_actor.as_ref() {
            if self.selected_actor.as_ref() != Some(hovered) {
                if let Some(hover_rect) = preview::selection_screen_rect(
                    hovered,
                    self.hit_regions,
                    preview_rect,
                    self.scene_dimensions,
                    preview_rect.size(),
                ) {
                    selection::draw_hover_highlight(ui.painter(), hovered, hover_rect);
                }
            }
        }

        if let Some(mouse) = pointer_pos {
            selection::draw_cycle_indicator(
                ui.painter(),
                mouse,
                self.selection.cycle_index,
                self.selection.click_candidates.len(),
            );
        }
    }

    /// Render the actual preview content (vello scene).
    fn render_preview_content(&self, ui: &mut egui::Ui, preview_rect: egui::Rect) {
        match self.preview_texture_id {
            Some(texture_id) => {
                ui.put(preview_rect, egui::Image::new((texture_id, preview_rect.size())));
            }
            None => {
                ui.painter().text(
                    preview_rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Preview initializing…",
                    egui::TextStyle::Body.resolve(ui.style()),
                    TEXT_MUTED,
                );
            }
        }
    }

    fn render_preview_selection_overlay(
        &self,
        ui: &mut egui::Ui,
        preview_rect: egui::Rect,
        is_dragging: bool,
    ) {
        if let Some(actor) = self.selected_actor.as_ref() {
            let props = self.get_actor_props(actor);
            let fallback = preview::selection_screen_rect(
                actor,
                self.hit_regions,
                preview_rect,
                self.scene_dimensions,
                preview_rect.size(),
            );
            preview::draw_selection_overlay(
                ui.painter(),
                props.as_ref(),
                fallback,
                is_dragging,
                preview_rect,
                self.scene_dimensions,
                preview_rect.size(),
            );
            if let DragState::Reorder {
                actor: drag_actor,
                container,
                target_index,
                layout_type,
                ..
            } = self.drag_state.clone()
            {
                if &drag_actor == actor {
                    let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                    if let Some(timeline) = self.timeline {
                        let order = timeline.get_child_order(&container, time_ms);
                        let siblings: Vec<(String, [f32; 2])> = order
                            .into_iter()
                            .filter(|label| label != actor)
                            .filter_map(|label| self.get_actor_props(&label).map(|p| (label, p.position)))
                            .collect();
                        if let Some(props) = props.as_ref() {
                            preview::draw_reorder_overlay(
                                ui.painter(),
                                props,
                                target_index,
                                &siblings,
                                preview_rect,
                                self.scene_dimensions,
                                preview_rect.size(),
                                layout_type == animatix::timeline::LayoutType::Row,
                            );
                        }
                    }
                }
            }
        }
    }

    pub(super) fn preview_ui(&mut self, ui: &mut egui::Ui) {
        const PLAYING_TEXT: Color32 = Color32::from_rgb(216, 249, 235);

        panel_frame().show(ui, |ui| {
        ui.vertical(|ui| {
            let header_avail = ui.available_width();
            let header_h = ROW_S;
            let (header_rect, _) = ui.allocate_exact_size(Vec2::new(header_avail, header_h), egui::Sense::hover());
            let baseline_y = header_rect.center().y;

            ui.painter().text(
                egui::pos2(header_rect.min.x, baseline_y),
                egui::Align2::LEFT_CENTER,
                "Preview",
                egui::FontId::new(FONT_SIZE_L, egui::FontFamily::Proportional),
                TEXT_SECONDARY,
            );

            let (badge_label, badge_fill, badge_text) = if self.preview.is_playing {
                ("Playing", GREEN, PLAYING_TEXT)
            } else {
                ("Paused", BORDER, TEXT_SECONDARY)
            };
            let badge_w = badge_label.len() as f32 * 7.0 + 16.0;
            let badge_rect = egui::Rect::from_min_size(
                egui::pos2(header_rect.max.x - badge_w, header_rect.min.y + 2.0),
                Vec2::new(badge_w, header_h - 4.0),
            );
            ui.painter().rect_filled(badge_rect, RADIUS_L, badge_fill);
            ui.painter().text(
                badge_rect.center(),
                egui::Align2::CENTER_CENTER,
                badge_label,
                egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
                badge_text,
            );

            ui.add_space(SPACE_S);

            let available = ui.available_size_before_wrap();
            let desired = fit_preview(
                self.scene_dimensions,
                Vec2::new(available.x.max(200.0), available.y.max(180.0)),
            );
            let (preview_rect, response) = ui.allocate_exact_size(desired, egui::Sense::click_and_drag());
            ui.painter().rect_stroke(
                preview_rect,
                RADIUS_L,
                Stroke::new(1.0, BORDER),
                egui::StrokeKind::Outside,
            );
            ui.painter().rect_filled(preview_rect, RADIUS_L, BG_BASE);

            let is_dragging = !matches!(self.drag_state, DragState::None);
            if self.handle_preview_drag(ui, preview_rect, &response) {
                return;
            }

            let pointer_pos = ui
                .ctx()
                .input(|i| i.pointer.latest_pos())
                .filter(|p| preview_rect.contains(*p));
            let scene_dimensions = self.scene_dimensions;
            let screen_to_scene = move |screen: egui::Pos2| preview_screen_to_scene(scene_dimensions, preview_rect, screen);

            if !self.selection.context_menu_open {
                selection::update_hover(
                    self.selection,
                    self.hit_regions,
                    pointer_pos,
                    screen_to_scene,
                    is_dragging,
                );
            } else {
                self.selection.hovered_actor = None;
            }

            self.handle_preview_selection(ui, preview_rect, &response);
            self.render_preview_cursor_feedback(ui, preview_rect);
            self.render_preview_overlays(ui, preview_rect);
            self.render_preview_content(ui, preview_rect);
            self.render_preview_selection_overlay(ui, preview_rect, is_dragging);

            // NOTE: errors are shown in the diagnostics banner above the canvas,
            // not as an overlay, to avoid duplicating the same message.
        });
        });
    }

    pub(super) fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            let current_time_s = self.preview.current_time_s;
            inspector::inspector_ui(ui, self.timeline, self.selected_actor, current_time_s, self.actions, self.keyframe_mode, self.scene_dimensions);
        });
    }
}

// ─── Layer Tree ─────────────────────────────────────────────────────────────

fn render_actor_tree(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    label: &str,
    selected_actor: &mut Option<String>,
    collapsed_actors: &mut HashSet<String>,
    depth: usize,
) {
    let Some(track) = timeline.get_track(label) else {
        return;
    };

    let is_selected = selected_actor.as_deref() == Some(label);
    let is_anonymous = label.starts_with("__anon");
    let has_children = !track.children.is_empty();
    let is_expanded = has_children && !collapsed_actors.contains(label);

    let (icon, display_label, label_color) = if is_anonymous {
        (
            Some(egui_phosphor::regular::GHOST),
            "anon",
            Some(TEXT_MUTED),
        )
    } else {
        let icon = Some(actor_icon_str(track.kind));
        (icon, label, None)
    };

    // Use the unique actor label (not display_label) for the row id
    // so that multiple anonymous actors don't share the same id.
    let row_id = ui.id().with(label);
    let label_owned = label.to_string();
    let response = components::Row::new(display_label)
        .indent(depth as f32 * 14.0)
        .selected(is_selected)
        .icon(icon)
        .label_color(label_color.unwrap_or(TEXT_SECONDARY))
        .has_children(has_children)
        .expanded(is_expanded)
        .show(ui, row_id);

    if response.chevron_clicked {
        if collapsed_actors.contains(&label_owned) {
            collapsed_actors.remove(&label_owned);
        } else {
            collapsed_actors.insert(label_owned.clone());
        }
    }

    if response.row_clicked {
        *selected_actor = Some(label.to_string());
    }

    // Children
    if is_expanded {
        for child_label in &track.children {
            render_actor_tree(
                ui,
                timeline,
                child_label,
                selected_actor,
                collapsed_actors,
                depth + 1,
            );
        }
    }
}
