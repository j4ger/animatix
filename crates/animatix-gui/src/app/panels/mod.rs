#![allow(dead_code)]

pub mod behavior;
pub mod preview_canvas;
pub mod inspector;
pub mod timeline_panel;

fn default_actor_type() -> &'static str {
    animatix::primitives::actor_kind_registry()
        .iter()
        .find(|meta| {
            meta.category == animatix::timeline::ActorCategory::Shape && !meta.advanced
        })
        .map(|meta| meta.type_name)
        .unwrap_or("Rect")
}

fn transition_type_label(id: &str) -> &'static str {
    animatix::transition_registry::display_name(id)
}

/// Compute a "nice" tick interval for ruler marks.
/// Produces round numbers (1, 2, 5, 10, 20, 50, 100, ...).
fn nice_tick_interval(visible_range: f32, target_ticks: f32) -> f32 {
    let raw = (visible_range / target_ticks).abs();
    if raw <= 0.0 {
        return 1.0;
    }
    let magnitude = 10.0_f32.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let nice_mul = if normalized < 1.5 {
        1.0
    } else if normalized < 3.5 {
        2.0
    } else if normalized < 7.5 {
        5.0
    } else {
        10.0
    };
    nice_mul * magnitude
}

const RULER_SIZE: f32 = 20.0;
const SNAP_THRESHOLD: f32 = 5.0; // scene pixels

pub use crate::app::commands::{Command, CommandQueue, PropertyEdit, PropertyValue};
use crate::app::components;

use crate::app::icons::actor_icon_str;
use crate::app::theme::*;
use crate::app::preview::{self, selection, ActorProps, DragState, fit_preview};
use crate::app::{FileTreeEntry, PreviewPaneState};
use crate::editor::EditorBuffer;
use animatix::diagnostics::Diagnostic;
use animatix::timeline::{PositionBinding, SceneDimensions, Timeline, TrackAccessor};
use egui::{Color32, Pos2, RichText, Stroke, Vec2};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SidebarTab {
    Explorer,
    Layers,
    Scenes,
}

pub(crate) struct WorkspaceViewer<'a> {
    pub(super) scene_names: Vec<String>,
    pub(super) import_aliases: Vec<String>,
    pub(super) active_scene: Option<String>,
    pub(super) is_composition: bool,
    pub(super) composition: Option<&'a animatix::composition::Composition>,
    pub(super) current_file: &'a Path,
    pub(super) workspace_root: &'a Path,
    pub(super) expanded_dirs: &'a mut HashSet<PathBuf>,
    pub(super) file_tree: &'a [FileTreeEntry],
    pub(super) editor: &'a mut EditorBuffer,
    pub(super) preview: &'a mut PreviewPaneState,
    pub(super) panel_state: &'a mut crate::app::PanelState,
    pub(super) diagnostics: &'a [Diagnostic],
    pub(super) preview_texture_id: Option<egui::TextureId>,
    pub(super) commands: &'a mut CommandQueue,
    pub(super) source_dirty: &'a mut String,
    pub(super) scene_dimensions: SceneDimensions,
    pub(super) timeline: Option<&'a Timeline>,
    pub(super) selected_actors: &'a mut HashSet<String>,
    pub(super) hit_regions: &'a [(String, kurbo::Rect)],
    pub(super) drag_state: &'a mut DragState,
    pub(super) selection: &'a mut selection::SelectionState,
    /// When true, property edits should create keyframes instead of overwriting defaults.
    pub(super) keyframe_mode: bool,
    /// Actor labels that the user have explicitly collapsed in the layer tree.
    pub(super) collapsed_actors: &'a mut HashSet<String>,
    /// Whether grid snapping is enabled in the preview canvas.
    pub(super) grid_enabled: &'a mut bool,
    /// Grid size in pixels.
    pub(super) grid_size: &'a mut f32,
    /// Per-actor pivot offsets in object-local space (relative to actor centre).
    pub(super) pivot_offsets: &'a mut HashMap<String, [f32; 2]>,
    /// Active tool mode for preview interactions.
    pub(super) tool_mode: &'a mut preview::ToolMode,
    /// Rotation snap increment in degrees (Shift+rotate).
    pub(super) rotation_snap_degrees: f32,
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
    if let Some(new_tab) = components::pill_tab_bar(ui, *active_tab, &tabs) {
        *active_tab = new_tab;
    }
}

impl WorkspaceViewer<'_> {
    fn get_actor_props(&self, actor: &str) -> Option<ActorProps> {
        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
        self.get_actor_props_at_time(actor, time_ms)
    }

    fn get_actor_props_at_time(&self, actor: &str, time_ms: u64) -> Option<ActorProps> {
        let timeline = self.timeline.or_else(|| {
            let comp = self.composition?;
            let scene_name = self.active_scene.as_ref()?;
            comp.scenes.get(scene_name).map(|s| &s.timeline)
        })?;
        let track = timeline.get_track(actor)?;
        let half = track.size.as_ref().map(|pt| pt.evaluate(time_ms))?;
        let local_size = [half[0] * 2.0, half[1] * 2.0];
        let world_affine = timeline.actor_world_affine(actor, time_ms, self.scene_dimensions)?;
        let coeffs = world_affine.as_coeffs();
        let position = [coeffs[4] as f32, coeffs[5] as f32];
        let rotation = (coeffs[1] as f32).atan2(coeffs[0] as f32);
        let scale = ((coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt()) as f32;
        let size = [local_size[0] * scale, local_size[1] * scale];
        let pivot_offset = self.pivot_offsets.get(actor).copied().unwrap_or([0.0, 0.0]);
        Some(ActorProps { position, size, rotation, pivot_offset })
    }

    fn is_layout_managed(&self, actor: &str) -> bool {
        let timeline = self.timeline.or_else(|| {
            let comp = self.composition?;
            let scene_name = self.active_scene.as_ref()?;
            comp.scenes.get(scene_name).map(|s| &s.timeline)
        });
        let Some(timeline) = timeline else { return false; };
        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
        preview::is_layout_managed(actor, timeline, time_ms)
    }

    fn find_layout_container(&self, actor: &str) -> Option<(String, animatix::timeline::LayoutType, usize)> {
        let timeline = self.timeline.or_else(|| {
            let comp = self.composition?;
            let scene_name = self.active_scene.as_ref()?;
            comp.scenes.get(scene_name).map(|s| &s.timeline)
        })?;
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
                let response = components::Row::new(&entry.name)
                    .indent(entry.depth as f32 * 14.0)
                    .selected(is_selected)
                    .icon(icon)
                    .label_color(label_color.unwrap_or(TEXT_SECONDARY))
                    .has_children(has_children)
                    .expanded(is_expanded)
                    .show(ui, row_id);

                if response.chevron_clicked {
                    self.commands.push_back(Command::ToggleExpandDir(path.clone()));
                }
                if response.row_clicked {
                    if is_dir {
                        self.commands.push_back(Command::ToggleExpandDir(path));
                    } else {
                        self.commands.push_back(Command::OpenFile(path));
                    }
                }
            }
        });
    }

    fn layers_content_ui(&mut self, ui: &mut egui::Ui) {
        // For compositions, use the active scene's timeline
        let timeline = self.timeline.or_else(|| {
            let comp = self.composition?;
            let scene_name = self.active_scene.as_ref()?;
            comp.scenes.get(scene_name).map(|s| &s.timeline)
        });
        let Some(timeline) = timeline else {
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

        // Show which scene's actors are being displayed
        if self.is_composition {
            if let Some(scene_name) = self.active_scene.as_ref() {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("{} {}", egui_phosphor::regular::FILM_STRIP, scene_name))
                                .size(FONT_SIZE_S)
                                .color(TEXT_MUTED),
                        )
                        .selectable(false),
                    );
                });
                ui.add_space(4.0);
            }
        }

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
                            .size(FONT_SIZE_L)
                            .color(ACCENT_BLUE),
                    )
                    .clicked()
                {
                    let label = format!("rect1");
                    let pos = [
                        self.scene_dimensions.width as f32 / 2.0,
                        self.scene_dimensions.height as f32 / 2.0,
                    ];
                    self.commands.push_back(Command::CreateActor { ty: default_actor_type().into(), label, position: pos });
                }
            });
            return;
        }

        let time_ms = (self.preview.current_time_s * 1000.0) as u64;
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for root_label in root_nodes {
                    render_actor_tree(
                        ui,
                        timeline,
                        root_label,
self.selected_actors,
                        self.collapsed_actors,
                        &mut self.commands,
                        time_ms,
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
            let drag_id = ui.id().with("scene_drag");
            let drag_idx: Option<usize> = ui.data(|d| d.get_temp(drag_id));
            let pointer_pos = ui.ctx().input(|i| i.pointer.latest_pos());
            let mut drop_target: Option<usize> = None;
            // Track actual row top/bottom positions for accurate drop targeting
            let mut row_positions: Vec<(f32, f32)> = Vec::new();

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);

                for (idx, scene_name) in self.scene_names.clone().into_iter().enumerate() {
                    let row_top = ui.cursor().top();
                    let is_active = self.active_scene.as_deref() == Some(scene_name.as_str());
                    let row_id = ui.id().with(&scene_name);
                    let edit_id = row_id.with("scene_name_edit");
                    let mut is_editing = ui.data(|d| d.get_temp::<bool>(edit_id)).unwrap_or(false);
                    let mut edit_buffer = ui
                        .data(|d| d.get_temp::<String>(edit_id.with("buf")))
                        .unwrap_or_else(|| scene_name.clone());

                    // Drag handle
                    let handle_width = 18.0;
                    let row_height = crate::app::theme::ROW_M;
                    let handle_rect = ui.available_rect_before_wrap();
                    let handle_rect = egui::Rect::from_min_size(
                        egui::pos2(handle_rect.min.x, handle_rect.min.y),
                        egui::vec2(handle_width, row_height),
                    );
                    let handle_response = ui.interact(handle_rect, row_id.with("drag"), egui::Sense::drag());
                    ui.painter().text(
                        handle_rect.center(),
                        egui::Align2::CENTER_CENTER,
                        egui_phosphor::regular::DOTS_SIX_VERTICAL,
                        egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                        if handle_response.hovered() { TEXT_SECONDARY } else { TEXT_MUTED },
                    );
                    ui.add_space(handle_width);

                    if handle_response.drag_started() {
                        ui.data_mut(|d| d.insert_temp(drag_id, idx));
                    }
                    if handle_response.dragged() {
                        if pointer_pos.is_some() {
                            drop_target = Some(idx);
                        }
                    }
                    let pointer_released = ui.input(|i| i.pointer.any_released());
                    if pointer_released && drag_idx == Some(idx) {
                        ui.data_mut(|d| d.remove::<usize>(drag_id));
                        if let Some(dragged_idx) = drag_idx {
                            if let Some(pointer) = pointer_pos {
                                // Compute drop target from actual row positions
                                let mut target_idx = 0;
                                for (i, &(top, bottom)) in row_positions.iter().enumerate() {
                                    if pointer.y >= top && pointer.y <= bottom {
                                        target_idx = i;
                                        break;
                                    }
                                    if pointer.y < top {
                                        break;
                                    }
                                    target_idx = i;
                                }
                                let target_idx = target_idx.min(self.scene_names.len().saturating_sub(1));
                                if dragged_idx != target_idx {
                                    let mut new_order = self.scene_names.clone();
                                    let item = new_order.remove(dragged_idx);
                                    let insert_at = target_idx.min(new_order.len());
                                    new_order.insert(insert_at, item);
                                    self.commands.push_back(Command::ReorderScenes(new_order));
                                }
                            }
                        }
                    }

                    // Drop target indicator (uses handle_rect for the main row area)
                    if drag_idx.is_some() {
                        if let Some(pointer) = pointer_pos {
                            let row_top = handle_rect.min.y;
                            let row_bottom = handle_rect.max.y;
                            let mid = (row_top + row_bottom) / 2.0;
                            if pointer.y < mid && pointer.y >= row_top - 2.0 {
                                ui.painter().line_segment(
                                    [egui::pos2(handle_rect.min.x, row_top), egui::pos2(ui.available_width() + handle_rect.min.x, row_top)],
                                    Stroke::new(2.0, ACCENT_BLUE),
                                );
                            } else if pointer.y >= mid && pointer.y < row_bottom + 2.0 {
                                ui.painter().line_segment(
                                    [egui::pos2(handle_rect.min.x, row_bottom), egui::pos2(ui.available_width() + handle_rect.min.x, row_bottom)],
                                    Stroke::new(2.0, ACCENT_BLUE),
                                );
                            }
                        }
                    }

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
                                self.commands.push_back(Command::RenameScene { old_name: scene_name.clone(), new_name: edit_buffer.clone() });
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
                        let delete_id = row_id.with("delete");
                        let response = components::Row::new(&scene_name)
                            .selected(is_active)
                            .right(|ui| {
                                if components::icon_button(ui, egui_phosphor::regular::TRASH, "Delete scene").clicked() {
                                    ui.data_mut(|d| d.insert_temp(delete_id, true));
                                }
                            })
                            .show(ui, row_id);

                        if ui.data(|d| d.get_temp::<bool>(delete_id)).unwrap_or(false) {
                            self.commands.push_back(Command::DeleteScene(scene_name.clone()));
                            ui.data_mut(|d| d.remove::<bool>(delete_id));
                        }

                        if response.row_double_clicked || response.row_secondary_clicked {
                            ui.data_mut(|d| {
                                d.insert_temp(edit_id, true);
                                d.insert_temp(edit_id.with("buf"), scene_name.clone());
                            });
                        } else if response.row_clicked {
                            self.commands.push_back(Command::SelectScene(scene_name.clone()));
                        }
                    }

                    // Transition badge: show play target and transition type beneath the scene row
                    if let Some(comp) = self.composition {
                        if let Some(edge) = comp.edges.get(&scene_name) {
                            let trans_edit_id = row_id.with("trans_edit");

                            // If the transport bar requested opening this scene's transition editor, open it
                            if self.panel_state.open_transition_editor.as_deref() == Some(scene_name.as_str()) {
                                ui.data_mut(|d| d.insert_temp(trans_edit_id, true));
                                self.panel_state.open_transition_editor = None;
                            }

                            let is_editing_trans = ui.data(|d| d.get_temp::<bool>(trans_edit_id)).unwrap_or(false);

                            if is_editing_trans {
                                let mut new_target = edge.to_scene.clone();
                                let mut new_type = edge.transition.id.clone();
                                let mut new_duration_ms = edge.transition.duration_ms;
                                let mut new_easing = edge.transition.easing;

                                ui.add_space(24.0);
                                components::card(ui, |ui| {
                                    ui.set_width(ui.available_width().min(340.0));

                                    // Target scene dropdown
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("Target:").size(FONT_SIZE_S).color(TEXT_MUTED));
                                        egui::ComboBox::from_id_salt(trans_edit_id.with("target"))
                                            .width(160.0)
                                            .selected_text(&new_target)
                                            .show_ui(ui, |ui| {
                                                for s in &self.scene_names {
                                                    ui.selectable_value(&mut new_target, s.clone(), s.as_str());
                                                }
                                            });
                                    });

                                    ui.add_space(SPACE_S);

                                    // Transition type and duration in ms
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("Type:").size(FONT_SIZE_S).color(TEXT_MUTED));
                                        egui::ComboBox::from_id_salt(trans_edit_id.with("type"))
                                            .width(100.0)
                                            .selected_text(animatix::transition_registry::display_name(&new_type))
                                            .show_ui(ui, |ui| {
                                                for def in animatix::transition_registry::REGISTRY {
                                                    ui.selectable_value(&mut new_type, def.id.to_string(), def.display_name);
                                                }
                                            });
                                        ui.add_space(SPACE_M);
                                        ui.label(RichText::new("Duration:").size(FONT_SIZE_S).color(TEXT_MUTED));
                                        ui.add_sized(
                                            [60.0, 0.0],
                                            egui::DragValue::new(&mut new_duration_ms)
                                                .speed(10)
                                                .suffix(" ms")
                                                .range(0..=10_000u64),
                                        );
                                    });

                                    ui.add_space(SPACE_S);

                                    // Easing picker
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("Easing:").size(FONT_SIZE_S).color(TEXT_MUTED));
                                        components::easing_picker::easing_picker(
                                            ui,
                                            trans_edit_id.with("easing"),
                                            &mut new_easing,
                                        );
                                    });

                                    ui.add_space(SPACE_S);

                                    // Action buttons
                                    ui.horizontal(|ui| {
                                        ui.add_space(ui.available_width() - 80.0);
                                        if ui.button("✓").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                            self.commands.push_back(Command::SetTransition { from_scene: scene_name.clone(), transition: animatix::ast::Transition {
                                                id: new_type,
                                                duration_ms: new_duration_ms,
                                                easing: new_easing,
                                            } });
                                            self.commands.push_back(Command::SetPlayTarget { from_scene: scene_name.clone(), target: Some(new_target) });
                                            ui.data_mut(|d| d.insert_temp(trans_edit_id, false));
                                        }
                                        if ui.button("✕").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                            ui.data_mut(|d| d.insert_temp(trans_edit_id, false));
                                        }
                                    });
                                });
                            } else {
                                let transition_label = if edge.transition.duration_ms > 0 {
                                    format!(
                                        "→ {} [{} · {}ms]",
                                        edge.to_scene,
                                        transition_type_label(&edge.transition.id),
                                        edge.transition.duration_ms
                                    )
                                } else {
                                    format!("→ {}", edge.to_scene)
                                };
                                let trans_response = ui.horizontal(|ui| {
                                    ui.add_space(24.0);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(transition_label)
                                                .size(FONT_SIZE_S)
                                                .color(TEXT_MUTED),
                                        )
                                        .sense(egui::Sense::click()),
                                    )
                                });
                                if trans_response.inner.clicked() {
                                    ui.data_mut(|d| d.insert_temp(trans_edit_id, true));
                                }
                            }
                        }
                    }
                    // Record actual row bounds for accurate drop targeting
                    let row_bottom = ui.cursor().top();
                    row_positions.push((row_top, row_bottom));
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
                self.commands.push_back(Command::AddScene);
            }
        });
    }

    pub(super) fn editor_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            self.editor.set_diagnostics(self.diagnostics);
            let response = self.editor.show(ui);
            if response.changed() || self.editor.text() != self.source_dirty.as_str() {
                *self.source_dirty = self.editor.text().to_string();
                self.commands.push_back(Command::EditorChanged);
            }
            if let Some(time_s) = self.editor.pending_scrub_to_time.take() {
                self.commands.push_back(Command::ScrubTo(time_s));
                if !self.preview.is_playing {
                    self.commands.push_back(Command::TogglePlayback);
                }
            }
            // ScrollToLine commands are handled by the shell in app/mod.rs
        });
    }

    pub(super) fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            let current_time_s = self.preview.current_time_s;
            // For compositions, use the active scene's timeline.
            // During a transition, if the selected actor is in the other scene,
            // use that scene's timeline so the inspector shows correct properties.
            let timeline = self.timeline.or_else(|| {
                let comp = self.composition?;
                let active_scene = self.active_scene.as_ref()?;
                let active_has_actor = self.selected_actors.iter().next().is_some_and(|sel| {
                    comp.scenes.get(active_scene.as_str()).is_some_and(|s| s.timeline.has_actor(sel))
                });
                if !active_has_actor {
                    let (_, _, transition) = comp.evaluate(current_time_s);
                    if transition.is_some() {
                        for (name, scene) in &comp.scenes {
                            if name != active_scene {
                                if self.selected_actors.iter().any(|sel| scene.timeline.has_actor(sel)) {
                                    return Some(&scene.timeline);
                                }
                            }
                        }
                    }
                }
                comp.scenes.get(active_scene.as_str()).map(|s| &s.timeline)
            });
            inspector::inspector_ui(ui, timeline, self.selected_actors, current_time_s, &mut self.commands, self.keyframe_mode, self.scene_dimensions, self.pivot_offsets);
        });
    }

    pub(super) fn timeline_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            // For compositions, use the active scene's timeline
            let timeline = self.timeline.or_else(|| {
                let comp = self.composition?;
                let scene_name = self.active_scene.as_ref()?;
                comp.scenes.get(scene_name).map(|s| &s.timeline)
            });
            timeline_panel::timeline_panel_ui(
                ui,
                self.preview,
                timeline,
                self.composition,
                self.active_scene.as_deref(),
                self.commands,
            );
        });
    }
}

// ─── Layer Tree ─────────────────────────────────────────────────────────────

fn render_actor_tree(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    label: &str,
    selected_actors: &mut HashSet<String>,
    collapsed_actors: &mut HashSet<String>,
    commands: &mut CommandQueue,
    time_ms: u64,
    depth: usize,
) {
    let Some(track) = timeline.get_track(label) else {
        return;
    };

    let is_selected = selected_actors.contains(label);
    let is_anonymous = label.starts_with("__anon");
    let has_children = !track.children.is_empty();
    let is_expanded = has_children && !collapsed_actors.contains(label);
    let opacity = track.opacity.get(time_ms, 1.0);
    let is_visible = opacity > 0.001;

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

    // Visibility toggle (eye icon) on the right
    let eye_icon = if is_visible {
        egui_phosphor::regular::EYE
    } else {
        egui_phosphor::regular::EYE_CLOSED
    };
    let eye_color = if is_visible { TEXT_SECONDARY } else { TEXT_DISABLED };

    let response = components::Row::new(display_label)
        .indent(depth as f32 * 14.0)
        .selected(is_selected)
        .icon(icon)
        .label_color(label_color.unwrap_or(if is_visible { TEXT_SECONDARY } else { TEXT_DISABLED }))
        .has_children(has_children)
        .expanded(is_expanded)
        .sense(egui::Sense::click_and_drag())
        .right(|ui| {
            let eye_btn = components::icon_button_colored(
                ui,
                eye_icon,
                if is_visible { "Hide layer" } else { "Show layer" },
                eye_color,
                TEXT_PRIMARY,
            );
            if eye_btn.clicked() {
                let new_opacity = if is_visible { 0.0 } else { 1.0 };
                commands.push_back(Command::PropertyEdit(PropertyEdit {
                    actor: label.to_string(),
                    property: "opacity".into(),
                    value: PropertyValue::Float(new_opacity),
                    create_keyframe: false,
                }));
            }
        })
        .show(ui, row_id);

    // ── Drag-and-drop reparenting ──
    let drag_id = ui.id().with("layer_drag");
    let drag_data_id = drag_id.with("data");

    // Detect drag start on this row
    if response.drag_started && !is_anonymous {
        ui.data_mut(|d| d.insert_temp(drag_data_id, label.to_string()));
    }

    // Detect drop target
    let is_dragging = ui.data(|d| d.get_temp::<String>(drag_data_id)).is_some();
    let is_drop_target = is_dragging && response.hovered && !is_anonymous;
    if is_drop_target {
        let dragged = ui.data(|d| d.get_temp::<String>(drag_data_id)).unwrap_or_default();
        if dragged != label {
            // Highlight as drop target
            ui.painter().rect_stroke(
                response.row_rect.expand(1.0),
                2,
                Stroke::new(1.5, ACCENT_BLUE),
                egui::StrokeKind::Outside,
            );
        }
    }

    // Handle drop (pointer released while dragging)
    if is_dragging && ui.input(|i| i.pointer.any_released()) {
        let dragged = ui.data(|d| d.get_temp::<String>(drag_data_id)).unwrap_or_default();
        if !dragged.is_empty() && dragged != label {
            let pointer_pos = ui.input(|i| i.pointer.latest_pos());
            let over_this_row = pointer_pos.map_or(false, |p| response.row_rect.contains(p));
            if over_this_row && is_drop_target {
                // Dropped on this row — reparent under this actor
                commands.push_back(Command::ReparentActor { actor: dragged.clone(), new_parent: Some(label.to_string()) });
            } else if !over_this_row && depth == 0 {
                // Dropped outside any row at root level — reparent to top-level
                // Only the root-level rows handle this to avoid duplicates
                let over_any_root = pointer_pos.map_or(false, |p| {
                    // Check if pointer is within the scroll area at all
                    response.row_rect.expand(100.0).contains(p)
                });
                if over_any_root {
                    commands.push_back(Command::ReparentActor { actor: dragged.clone(), new_parent: None });
                }
            }
        }
        ui.data_mut(|d| d.remove::<String>(drag_data_id));
    }

    if response.chevron_clicked {
        if collapsed_actors.contains(&label_owned) {
            collapsed_actors.remove(&label_owned);
        } else {
            collapsed_actors.insert(label_owned.clone());
        }
    }

    if response.row_clicked {
        let modifiers = ui.ctx().input(|i| i.modifiers);
        let multi = modifiers.shift || modifiers.ctrl || modifiers.command;
        if multi {
            if selected_actors.contains(label) {
                selected_actors.remove(label);
            } else {
                selected_actors.insert(label.to_string());
            }
        } else {
            selected_actors.clear();
            selected_actors.insert(label.to_string());
        }
    }

    // Children
    if is_expanded {
        for child_label in &track.children {
            render_actor_tree(
                ui,
                timeline,
                child_label,
                selected_actors,
                collapsed_actors,
                commands,
                time_ms,
                depth + 1,
            );
        }
    }
}
