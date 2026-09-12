//! Sidebar panel: file explorer tree and layer tree.
//!
//! Focused context struct borrows only the fields the sidebar needs.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use animatix::timeline::{SceneDimensions, Timeline};
use animatix_syntax::to_source::ToSource;
use egui::{RichText, Vec2};

use crate::app::commands::{
    ActionQueue, ActorCommand, Command, SceneCommand, ShellAction, ViewAction,
};
use crate::app::components::button::Button;
use crate::app::components::context_menu::{MenuEntry, render_menu};
use crate::app::components::{anim, layout, row, text_tooltip};
use crate::app::design_tokens::motion;
use crate::app::design_tokens::typography::TextRole;
use crate::app::panels::SidebarTab;
use crate::app::panels::{CompactDrawer, SIDEBAR_TABS};
use crate::app::{FileTreeEntry, PreviewPaneState};
use crate::editor::EditorBuffer;

/// Id used to persist the explorer filter string in egui's data store.
const EXPLORER_FILTER_ID: &str = "explorer_filter";

/// Id used to persist the layers filter string in egui's data store.
const LAYERS_FILTER_ID: &str = "layers_filter";

/// Shared context for the sidebar panel (tab bar + dispatch only).
///
/// This is a wide struct because it carries everything the sidebar tabs might
/// need. Individual tabs receive focused sub-contexts (e.g. `ExplorerContext`)
/// so their signatures don't depend on the full surface.
pub(crate) struct SidebarContext<'a> {
    pub active_scene: Option<&'a str>,
    pub is_composition: bool,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub current_file: &'a Path,
    pub expanded_dirs: &'a mut HashSet<PathBuf>,
    pub file_tree: &'a [FileTreeEntry],
    pub preview: &'a mut PreviewPaneState,
    pub commands: &'a mut ActionQueue,
    pub timeline: Option<&'a Timeline>,
    pub selected_actors: &'a mut HashSet<String>,
    pub collapsed_actors: &'a mut HashSet<String>,
    pub sidebar_tab: &'a mut SidebarTab,
    pub editor: &'a mut EditorBuffer,
    pub components: &'a HashMap<String, animatix_syntax::module::ComponentEntry>,
    pub asset_cache: Option<&'a animatix::timeline::assets::AssetCache>,
    pub scene_dimensions: SceneDimensions,
    /// True when the window is narrow enough to render the compact icon rail
    /// instead of the tab bar + content.
    pub compact: bool,
    /// Compact-mode overlay drawer slot; a rail click opens the sidebar drawer.
    pub compact_drawer: &'a mut Option<CompactDrawer>,
}

// ─── Per-tab focused contexts ─────────────────────────────────────────────

pub(crate) struct ExplorerContext<'a> {
    pub current_file: &'a Path,
    pub expanded_dirs: &'a mut HashSet<PathBuf>,
    pub file_tree: &'a [FileTreeEntry],
    pub commands: &'a mut ActionQueue,
}

pub(crate) struct LayersContext<'a> {
    pub timeline: Option<&'a Timeline>,
    pub active_scene: Option<&'a str>,
    pub selected_actors: &'a mut HashSet<String>,
    pub collapsed_actors: &'a mut HashSet<String>,
    pub commands: &'a mut ActionQueue,
    pub preview: &'a mut PreviewPaneState,
    pub scene_dimensions: SceneDimensions,
    pub is_composition: bool,
}

pub(crate) struct ScenesContext<'a> {
    pub composition: Option<&'a animatix::composition::Composition>,
    pub active_scene: Option<&'a str>,
    pub commands: &'a mut ActionQueue,
}

pub(crate) struct ComponentsContext<'a> {
    pub components: &'a HashMap<String, animatix_syntax::module::ComponentEntry>,
    pub commands: &'a mut ActionQueue,
    pub scene_dimensions: SceneDimensions,
    /// Source text for finding component definition lines (jump-to-definition).
    pub source_text: &'a str,
}

pub(crate) struct AssetsContext<'a> {
    pub asset_cache: Option<&'a animatix::timeline::assets::AssetCache>,
    pub commands: &'a mut ActionQueue,
    pub scene_dimensions: SceneDimensions,
}

pub(crate) fn sidebar_ui(ctx: &mut SidebarContext<'_>, ui: &mut egui::Ui) {
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    super::panel_frame().show(ui, |ui| {
        // Compact mode: a narrow icon rail replaces the tab bar + content. The
        // full content lives in the overlay drawer (opened from the rail).
        if ctx.compact {
            sidebar_rail_ui(ctx, ui);
            return;
        }

        let mut active_tab = *ctx.sidebar_tab;
        let prev_tab = *ctx.sidebar_tab;

        render_sidebar_tab_bar(ui, &mut active_tab);
        ui.add_space(sp.base.space_3);

        // Slide-in animation on tab switch
        let content_offset_id = ui.id().with("sidebar_slide");
        if prev_tab != active_tab {
            ui.ctx().animate_value_with_time(content_offset_id, 6.0, 0.0);
            // Clear the file-tree / layer filters when leaving their tabs.
            if active_tab != SidebarTab::Project {
                ui.data_mut(|d| d.remove::<String>(egui::Id::new(EXPLORER_FILTER_ID)));
            }
            if active_tab != SidebarTab::Outline {
                ui.data_mut(|d| d.remove::<String>(egui::Id::new(LAYERS_FILTER_ID)));
            }
        }
        let offset = anim::animate_toward(ui.ctx(), content_offset_id, 0.0, motion::PANEL);
        if offset > 0.01 {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }

        ui.allocate_ui_with_layout(
            ui.available_size(),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.add_space(offset);
                sidebar_tab_content_ui(ctx, ui, active_tab);
            },
        );

        *ctx.sidebar_tab = active_tab;
    });
}

/// Narrow vertical icon rail shown in compact mode.
///
/// Clicking an icon selects that view and opens the sidebar overlay drawer, so
/// every top-level view stays reachable without the tab bar.
fn sidebar_rail_ui(ctx: &mut SidebarContext<'_>, ui: &mut egui::Ui) {
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let rail_width = super::COMPACT_RAIL_WIDTH;
    ui.allocate_ui_with_layout(
        egui::Vec2::new(rail_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.add_space(sp.base.space_3);
            for (tab, icon, label) in SIDEBAR_TABS {
                // The dot tracks both the selected view and whether its drawer
                // is actually open, so the rail reflects what is on screen.
                let active =
                    *ctx.sidebar_tab == tab && *ctx.compact_drawer == Some(CompactDrawer::Sidebar);
                let resp = ui.add(Button::icon(icon).active(active));
                text_tooltip(ui, resp.id.with(("compact_rail", label)), &resp, label);
                if resp.clicked() {
                    *ctx.sidebar_tab = tab;
                    *ctx.compact_drawer = Some(CompactDrawer::Sidebar);
                }
                ui.add_space(sp.base.space_2);
            }
        },
    );
}

/// Render only the active tab's content (no tab bar).
///
/// Shared by the docked sidebar and the compact overlay drawer, so both render
/// identical content and dispatch identical commands.
pub(crate) fn sidebar_tab_content_ui(
    ctx: &mut SidebarContext<'_>,
    ui: &mut egui::Ui,
    active_tab: SidebarTab,
) {
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    match active_tab {
        SidebarTab::Project => {
            let section = section_switcher(
                ui,
                ui.id().with("sidebar_project_section"),
                &[
                    (ProjectSection::Files, egui_phosphor::regular::FOLDER, "Files"),
                    (ProjectSection::Assets, egui_phosphor::regular::IMAGES, "Assets"),
                ],
                ProjectSection::Files,
            );
            ui.add_space(sp.base.space_2);
            match section {
                ProjectSection::Files => {
                    let mut ectx = ExplorerContext {
                        current_file: ctx.current_file,
                        expanded_dirs: ctx.expanded_dirs,
                        file_tree: ctx.file_tree,
                        commands: ctx.commands,
                    };
                    explorer_content_ui(&mut ectx, ui);
                },
                ProjectSection::Assets => {
                    let mut actx = AssetsContext {
                        asset_cache: ctx.asset_cache,
                        commands: ctx.commands,
                        scene_dimensions: ctx.scene_dimensions,
                    };
                    assets_content_ui(&mut actx, ui);
                },
            }
        },
        SidebarTab::Outline => {
            let section = section_switcher(
                ui,
                ui.id().with("sidebar_outline_section"),
                &[
                    (OutlineSection::Layers, egui_phosphor::regular::STACK, "Layers"),
                    (OutlineSection::Scenes, egui_phosphor::regular::FILM_STRIP, "Scenes"),
                ],
                OutlineSection::Layers,
            );
            ui.add_space(sp.base.space_2);
            match section {
                OutlineSection::Layers => {
                    let mut lctx = LayersContext {
                        timeline: ctx.timeline,
                        active_scene: ctx.active_scene,
                        selected_actors: ctx.selected_actors,
                        collapsed_actors: ctx.collapsed_actors,
                        commands: ctx.commands,
                        preview: ctx.preview,
                        scene_dimensions: ctx.scene_dimensions,
                        is_composition: ctx.is_composition,
                    };
                    layers_content_ui(&mut lctx, ui);
                },
                OutlineSection::Scenes => {
                    let mut sctx = ScenesContext {
                        composition: ctx.composition,
                        active_scene: ctx.active_scene,
                        commands: ctx.commands,
                    };
                    scenes_content_ui(&mut sctx, ui);
                },
            }
        },
        SidebarTab::Library => {
            let mut cctx = ComponentsContext {
                components: ctx.components,
                commands: ctx.commands,
                scene_dimensions: ctx.scene_dimensions,
                source_text: ctx.editor.text(),
            };
            components_content_ui(&mut cctx, ui);
        },
    }
}

/// Sub-view within the merged Project tab.
#[derive(Clone, Copy, PartialEq)]
enum ProjectSection {
    Files,
    Assets,
}

/// Sub-view within the merged Outline tab.
#[derive(Clone, Copy, PartialEq)]
enum OutlineSection {
    Layers,
    Scenes,
}

/// Two-way section switcher for a merged sidebar tab.
///
/// Selection is kept in egui temp storage so merging tabs needs no extra
/// persistent state.
fn section_switcher<T: PartialEq + Copy + Send + Sync + 'static>(
    ui: &mut egui::Ui,
    id: egui::Id,
    entries: &[(T, &'static str, &'static str)],
    default: T,
) -> T {
    let current: T = ui.data_mut(|d| d.get_temp(id).unwrap_or(default));
    if let Some(new) = layout::pill_tab_bar(ui, current, entries) {
        ui.data_mut(|d| d.insert_temp(id, new));
        new
    } else {
        current
    }
}

fn render_sidebar_tab_bar(ui: &mut egui::Ui, active_tab: &mut SidebarTab) {
    if let Some(new_tab) = layout::pill_tab_bar(ui, *active_tab, &SIDEBAR_TABS) {
        *active_tab = new_tab;
    }
}

fn explorer_content_ui(ctx: &mut ExplorerContext<'_>, ui: &mut egui::Ui) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    // ── Filter input ────────────────────────────────────────────────────────
    let filter_id = egui::Id::new(EXPLORER_FILTER_ID);
    let mut filter = ui.data(|d| d.get_temp::<String>(filter_id)).unwrap_or_default();

    ui.horizontal(|ui| {
        ui.add_space(sp.base.space_2);
        let response = ui.add(
            egui::TextEdit::singleline(&mut filter)
                .hint_text("Filter files…")
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        }
        // If the field was cleared interactively, persist the empty string so
        // the stored value stays in sync (clearing is distinct from never-set).
        if filter.is_empty() && response.lost_focus() {
            ui.data_mut(|d| d.insert_temp(filter_id, String::new()));
        }
    });
    ui.add_space(sp.base.space_2);

    let filter_lower = filter.to_lowercase();
    let has_filter = !filter_lower.is_empty();

    // ── Pre-compute visibility ──────────────────────────────────────────────
    // When a filter is active we build a `show` mask.  The rules are:
    //   1. An entry is visible if its name contains the filter (case-insensitive).
    //   2. If a directory is visible (by name), all its descendants are visible.
    //   3. If any descendant is visible, the ancestor directory is also visible.
    // When no filter is active every entry passes.
    let show = if has_filter {
        let len = ctx.file_tree.len();
        let mut show = vec![false; len];

        // --- Pass 1: direct name match ---
        for (i, entry) in ctx.file_tree.iter().enumerate() {
            show[i] = entry.name.to_lowercase().contains(&filter_lower);
        }

        // --- Pass 2 (forward): expand matching directories to all children ---
        for i in 0..len {
            if show[i] && ctx.file_tree[i].is_dir {
                let parent_depth = ctx.file_tree[i].depth;
                for s in show[(i + 1)..].iter_mut().zip(ctx.file_tree[(i + 1)..].iter()) {
                    if s.1.depth <= parent_depth {
                        break;
                    }
                    *s.0 = true;
                }
            }
        }

        // --- Pass 3 (backward): show ancestors of any visible entry ---
        for i in (0..len).rev() {
            if ctx.file_tree[i].is_dir {
                let parent_depth = ctx.file_tree[i].depth;
                for s in show[(i + 1)..].iter().zip(ctx.file_tree[(i + 1)..].iter()) {
                    if s.1.depth <= parent_depth {
                        break;
                    }
                    if *s.0 {
                        show[i] = true;
                        break;
                    }
                }
            }
        }

        show
    } else {
        vec![true; ctx.file_tree.len()]
    };

    // ── Render visible tree entries ─────────────────────────────────────────
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
        for (i, entry) in ctx.file_tree.iter().enumerate() {
            if !show[i] {
                continue;
            }

            let is_selected = !entry.is_dir && entry.path == ctx.current_file;
            let is_expanded = entry.is_dir && ctx.expanded_dirs.contains(&entry.path);
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
                let color = if is_amx {
                    Some(t.palette.accent.primary)
                } else {
                    None
                };
                (Some(file_icon), color)
            };

            let row_id = ui.id().with(entry.path.display().to_string());
            let path = entry.path.clone();
            let is_dir = entry.is_dir;
            let response = row::Row::new(&entry.name)
                .indent(entry.depth as f32 * sp.base.component.icon_slot_width)
                .selected(is_selected)
                .icon(icon)
                .label_color(label_color.unwrap_or(t.palette.text.secondary))
                .has_children(has_children)
                .expanded(is_expanded)
                .show(ui, row_id);

            // Right-click context menu (use row response so we don't steal left-clicks)
            response.response.context_menu(|ui| {
                let entries = if !is_dir {
                    vec![MenuEntry::item_with_icon(
                        egui_phosphor::regular::FOLDER_OPEN,
                        "Open",
                    )]
                } else if is_expanded {
                    vec![MenuEntry::item_with_icon(
                        egui_phosphor::regular::CARET_UP,
                        "Collapse",
                    )]
                } else {
                    vec![MenuEntry::item_with_icon(
                        egui_phosphor::regular::CARET_DOWN,
                        "Expand",
                    )]
                };
                if render_menu(ui, &entries).is_some() {
                    if is_dir {
                        ctx.commands.push_back(ShellAction::Command(Command::ToggleExpandDir(
                            path.clone(),
                        )));
                    } else {
                        ctx.commands
                            .push_back(ShellAction::Command(Command::OpenFile(path.clone())));
                    }
                    ui.close();
                }
            });

            if response.chevron_clicked {
                ctx.commands
                    .push_back(ShellAction::Command(Command::ToggleExpandDir(path.clone())));
            }
            if response.row_clicked {
                if is_dir {
                    ctx.commands.push_back(ShellAction::Command(Command::ToggleExpandDir(path)));
                } else {
                    ctx.commands.push_back(ShellAction::Command(Command::OpenFile(path)));
                }
            }
        }
    });
}

fn scenes_content_ui(ctx: &mut ScenesContext<'_>, ui: &mut egui::Ui) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let Some(composition) = ctx.composition else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "No composition loaded",
            "Define multiple scenes with # SceneName to see them here.",
        );
        return;
    };

    let scene_names = &composition.declaration_order;

    // ── Add scene ─────────────────────────────────────────────────────────
    ui.horizontal(|ui| {
        ui.add_space(sp.base.space_2);
        let new_btn = ui.add(Button::ghost("New scene").with_icon(egui_phosphor::regular::PLUS));
        text_tooltip(
            ui,
            new_btn.id.with("new_scene_tip"),
            &new_btn,
            "Add a scene at the end of the composition",
        );
        if new_btn.clicked() {
            let mut n = scene_names.len() + 1;
            let mut name = format!("scene{n}");
            while scene_names.iter().any(|existing| existing == &name) {
                n += 1;
                name = format!("scene{n}");
            }
            ctx.commands.push_back(SceneCommand::AddScene(name).into());
        }
    });
    ui.add_space(sp.base.space_2);

    if scene_names.is_empty() {
        layout::empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "No scenes",
            "This composition has no scenes.",
        );
        return;
    }

    let drag_id = ui.id().with("scene_drag");
    let drag_data_id = drag_id.with("data");
    let drop_index_id = drag_id.with("drop_idx");

    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        for (idx, scene_name) in scene_names.iter().enumerate() {
            let is_active = ctx.active_scene == Some(scene_name.as_str());
            let row_id = ui.id().with(scene_name);

            // Duration hint
            let duration_hint = composition
                .scene_start_times
                .get(scene_name)
                .map(|start| {
                    let end = composition
                        .scene_start_times
                        .iter()
                        .filter(|(k, _)| *k != scene_name)
                        .map(|(_, v)| *v)
                        .filter(|v| *v > *start)
                        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                        .unwrap_or(composition.global_duration_s);
                    format!("{:.1}s – {:.1}s", start, end)
                })
                .unwrap_or_else(|| "–".to_string());

            // Transition hint (outgoing edge)
            let transition_hint = composition.edges.get(scene_name).map(|edge| {
                format!(
                    "{} {} ({:.0}ms)",
                    egui_phosphor::regular::ARROW_RIGHT,
                    edge.to_scene,
                    edge.transition.duration_ms
                )
            });

            let response = row::Row::new(scene_name)
                .selected(is_active)
                .icon(Some(egui_phosphor::regular::FILM_STRIP))
                .label_color(if is_active {
                    t.palette.accent.primary
                } else {
                    t.palette.text.secondary
                })
                .sense(egui::Sense::click_and_drag())
                .show(ui, row_id);

            // Drag start
            if response.drag_started {
                ui.data_mut(|d| d.insert_temp(drag_data_id, (idx, scene_name.clone())));
            }

            // Drop target detection
            let is_dragging = ui.data(|d| d.get_temp::<(usize, String)>(drag_data_id)).is_some();
            if is_dragging && response.hovered {
                let pointer = ui.ctx().input(|i| i.pointer.latest_pos());
                if let Some(p) = pointer {
                    let center = response.row_rect.center().y;
                    let new_idx = if p.y < center { idx } else { idx + 1 };
                    ui.data_mut(|d| d.insert_temp(drop_index_id, new_idx));
                    // Draw drop indicator line
                    let line_y = if p.y < center {
                        response.row_rect.top()
                    } else {
                        response.row_rect.bottom()
                    };
                    ui.painter().line_segment(
                        [
                            egui::pos2(response.row_rect.left(), line_y),
                            egui::pos2(response.row_rect.right(), line_y),
                        ],
                        egui::Stroke::new(2.0, t.palette.accent.primary),
                    );
                }
            }

            // Click to activate scene
            if response.row_clicked {
                ctx.commands.push_back(SceneCommand::SelectScene(scene_name.clone()).into());
            }

            // Context menu
            response.response.context_menu(|ui| {
                let entries = vec![
                    MenuEntry::item_with_icon(egui_phosphor::regular::CHECK, "Set as active"),
                    MenuEntry::separator(),
                    MenuEntry::item_with_icon(egui_phosphor::regular::COPY, "Duplicate scene"),
                    MenuEntry::item_with_icon(egui_phosphor::regular::TRASH, "Delete scene"),
                ];
                if let Some(menu_idx) = render_menu(ui, &entries) {
                    match menu_idx {
                        0 => ctx
                            .commands
                            .push_back(SceneCommand::SelectScene(scene_name.clone()).into()),
                        2 => ctx
                            .commands
                            .push_back(SceneCommand::DuplicateScene(scene_name.clone()).into()),
                        3 => ctx
                            .commands
                            .push_back(SceneCommand::DeleteScene(scene_name.clone()).into()),
                        _ => {},
                    }
                    ui.close();
                }
            });

            // Sub-label: duration + transition
            if is_active {
                ui.horizontal(|ui| {
                    ui.add_space(sp.base.component.icon_slot_width + sp.base.space_2);
                    ui.label(
                        RichText::new(&duration_hint)
                            .size(TextRole::Micro.size())
                            .color(t.palette.text.muted),
                    );
                });
                if let Some(hint) = transition_hint {
                    ui.horizontal(|ui| {
                        ui.add_space(sp.base.component.icon_slot_width + sp.base.space_2);
                        ui.label(
                            RichText::new(hint)
                                .size(TextRole::Micro.size())
                                .color(t.palette.text.muted),
                        );
                    });
                }
                ui.add_space(sp.base.space_2);
            }
        }

        // Handle drop (outside the loop so is_dragging is in scope)
        let drag_active = ui.data(|d| d.get_temp::<(usize, String)>(drag_data_id)).is_some();
        if drag_active && ui.input(|i| i.pointer.any_released()) {
            if let Some((from_idx, _dragged_name)) =
                ui.data(|d| d.get_temp::<(usize, String)>(drag_data_id))
            {
                let to_idx = ui.data(|d| d.get_temp::<usize>(drop_index_id)).unwrap_or(from_idx);
                if from_idx != to_idx && to_idx <= scene_names.len() {
                    let mut new_order = scene_names.clone();
                    let removed = new_order.remove(from_idx);
                    let insert_at = if to_idx > from_idx {
                        to_idx - 1
                    } else {
                        to_idx
                    };
                    new_order.insert(insert_at.min(new_order.len()), removed);
                    ctx.commands.push_back(SceneCommand::ReorderScenes(new_order).into());
                }
                ui.data_mut(|d| {
                    d.remove::<(usize, String)>(drag_data_id);
                    d.remove::<usize>(drop_index_id);
                });
            }
        }
    });
}

/// Temp-data id for the inline layer rename bar.
fn layer_rename_id() -> egui::Id {
    egui::Id::new("layer_rename_target")
}

/// Begin renaming a layer: seed the edit buffer for the rename bar.
fn start_layer_rename(ui: &mut egui::Ui, label: &str) {
    let id = layer_rename_id();
    ui.data_mut(|d| {
        d.insert_temp(id, label.to_string());
        d.insert_temp(id.with("buf"), label.to_string());
    });
}

fn layers_content_ui(ctx: &mut LayersContext<'_>, ui: &mut egui::Ui) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let Some(timeline) = ctx.timeline else {
        layout::empty_state(ui, egui_phosphor::regular::FILM_STRIP, "No timeline loaded", "");
        return;
    };

    // ── Filter input ──────────────────────────────────────────────────────
    let filter_id = egui::Id::new(LAYERS_FILTER_ID);
    let mut filter = ui.data(|d| d.get_temp::<String>(filter_id)).unwrap_or_default();

    ui.horizontal(|ui| {
        ui.add_space(sp.base.space_2);
        let response = ui.add(
            egui::TextEdit::singleline(&mut filter)
                .hint_text("Filter layers…")
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        }
        if filter.is_empty() && response.lost_focus() {
            ui.data_mut(|d| d.insert_temp(filter_id, String::new()));
        }
    });
    ui.add_space(sp.base.space_2);

    // ── Inline rename bar (context-menu "Rename…" or double-click a layer) ──
    let rename_id = layer_rename_id();
    if let Some(target) = ui.data(|d| d.get_temp::<String>(rename_id)) {
        let buf_id = rename_id.with("buf");
        let mut buf = ui.data(|d| d.get_temp::<String>(buf_id)).unwrap_or_else(|| target.clone());
        ui.horizontal(|ui| {
            ui.add_space(sp.base.space_2);
            ui.add(
                egui::Label::new(
                    RichText::new(format!("{} Rename", egui_phosphor::regular::PENCIL_SIMPLE))
                        .size(TextRole::BodyS.size())
                        .color(t.palette.text.muted),
                )
                .selectable(false),
            );
            let response = ui.add(
                egui::TextEdit::singleline(&mut buf)
                    .hint_text("New name")
                    .desired_width(f32::INFINITY),
            );
            response.request_focus();
            if response.lost_focus() {
                ui.data_mut(|d| d.remove::<String>(rename_id));
                ui.data_mut(|d| d.remove::<String>(buf_id));
                let new_label = buf.trim().to_string();
                if !new_label.is_empty() && new_label != target {
                    ctx.commands.push_back(
                        ActorCommand::RenameActor {
                            old_label: target.clone(),
                            new_label,
                        }
                        .into(),
                    );
                }
            } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                ui.data_mut(|d| d.remove::<String>(rename_id));
                ui.data_mut(|d| d.remove::<String>(buf_id));
            } else {
                ui.data_mut(|d| d.insert_temp(buf_id, buf.clone()));
            }
        });
        ui.add_space(sp.base.space_2);
    }

    let filter_lower = filter.to_lowercase();
    let has_filter = !filter_lower.is_empty();

    // Show which scene's actors are being displayed
    if ctx.is_composition {
        if let Some(scene_name) = ctx.active_scene.as_ref() {
            ui.horizontal(|ui| {
                ui.add_space(sp.base.space_4);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "{} {}",
                            egui_phosphor::regular::FILM_STRIP,
                            scene_name
                        ))
                        .size(TextRole::BodyS.size())
                        .color(t.palette.text.muted),
                    )
                    .selectable(false),
                );
            });
            ui.add_space(sp.base.space_2);
        }
    }

    let root_nodes = timeline.root_actor_labels();
    if root_nodes.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(sp.base.space_5 * 3.0);
            ui.add(
                egui::Label::new(
                    RichText::new(egui_phosphor::regular::FILM_STRIP)
                        .size(sp.base.row_l)
                        .color(t.palette.text.muted),
                )
                .selectable(false),
            );
            ui.add_space(sp.base.space_3);
            ui.add(
                egui::Label::new(
                    RichText::new("No actors in scene")
                        .size(TextRole::Title.size())
                        .color(t.palette.text.secondary),
                )
                .selectable(false),
            );
            ui.add_space(sp.base.space_5);
            if ui
                .button(
                    RichText::new(format!("{} Add Actor", egui_phosphor::regular::PLUS))
                        .size(TextRole::Title.size())
                        .color(t.palette.accent.primary),
                )
                .clicked()
            {
                let label = "rect1".to_string();
                let pos = [
                    ctx.scene_dimensions.width as f32 / 2.0,
                    ctx.scene_dimensions.height as f32 / 2.0,
                ];
                ctx.commands.push_back(
                    ActorCommand::CreateActor {
                        ty: super::default_actor_type().into(),
                        label,
                        position: pos,
                        props: vec![],
                    }
                    .into(),
                );
            }
        });
        return;
    }

    let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        for root_label in root_nodes {
            render_actor_tree(
                ui,
                timeline,
                root_label,
                ctx.selected_actors,
                ctx.collapsed_actors,
                ctx.commands,
                time_ms,
                0,
                &filter_lower,
                has_filter,
            );
        }
    });
}

// ─── Layer Tree ─────────────────────────────────────────────────────────────

fn render_actor_tree(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    label: &str,
    selected_actors: &mut HashSet<String>,
    collapsed_actors: &mut HashSet<String>,
    commands: &mut ActionQueue,
    _time_ms: u64,
    depth: usize,
    filter_lower: &str,
    has_filter: bool,
) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let Some(track) = timeline.get_track(label) else {
        return;
    };

    let is_selected = selected_actors.contains(label);
    let is_anonymous = label.starts_with("__anon");
    let has_children = !track.children.is_empty();

    // ── Filter logic ─────────────────────────────────────────────────────
    // When a filter is active, decide whether this actor (and its subtree)
    // should be visible.  An actor is visible if:
    //   1. Its own label matches the filter, OR
    //   2. Any of its descendants match the filter.
    // Matching directories/actors are force-expanded so their descendants
    // remain visible (mirrors explorer_content_ui's show-mask logic).
    // NOTE: selected_actors is intentionally NOT cleared when filtering;
    // hidden-but-selected actors stay selected so they survive filter changes.
    // Drag-reparent drops are disabled while filtered to avoid reparenting
    // through partially-visible tree branches (ambiguous drop targets).
    let (is_expanded, should_show) = if has_filter {
        let base_expanded = has_children && !collapsed_actors.contains(label);
        let self_matches = label.to_lowercase().contains(filter_lower);
        let descendant_matches = track
            .children
            .iter()
            .any(|child| actor_matches_filter(timeline, child, filter_lower));
        let show = self_matches || descendant_matches;
        let expanded = self_matches || descendant_matches || base_expanded;
        (expanded, show)
    } else {
        let is_expanded = has_children && !collapsed_actors.contains(label);
        (is_expanded, true)
    };

    if has_filter && !should_show {
        return;
    }

    let is_visible = track.visible;

    let (icon, display_label, label_color) = if is_anonymous {
        (Some(egui_phosphor::regular::GHOST), "anon", Some(t.palette.text.muted))
    } else {
        let icon = Some(crate::app::icons::actor_icon_for_track(track, timeline));
        (icon, label, None)
    };

    let row_id = ui.id().with(label);
    let label_owned = label.to_string();

    // Visibility toggle (eye icon) on the right
    let eye_icon = if is_visible {
        egui_phosphor::regular::EYE
    } else {
        egui_phosphor::regular::EYE_CLOSED
    };
    let eye_color = if is_visible {
        t.palette.text.secondary
    } else {
        t.palette.text.disabled
    };

    let is_locked = track.locked;
    let lock_icon = if is_locked {
        egui_phosphor::regular::LOCK_KEY
    } else {
        egui_phosphor::regular::LOCK_KEY_OPEN
    };
    let lock_color = if is_locked {
        t.palette.status.warning
    } else {
        t.palette.text.disabled
    };

    let response = row::Row::new(display_label)
        .indent(depth as f32 * sp.base.component.icon_slot_width)
        .selected(is_selected)
        .icon(icon)
        .label_color(label_color.unwrap_or(if is_visible {
            t.palette.text.secondary
        } else {
            t.palette.text.disabled
        }))
        .has_children(has_children)
        .expanded(is_expanded)
        .sense(egui::Sense::click_and_drag())
        .right(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_1, 0.0);
            let eye_btn = ui.add(
                Button::icon(eye_icon)
                    .with_tooltip(if is_visible {
                        "Hide layer"
                    } else {
                        "Show layer"
                    })
                    .icon_color(eye_color)
                    .hover_icon_color(t.palette.text.primary),
            );
            if eye_btn.clicked() {
                commands.push_back(ActorCommand::ToggleActorVisibility(label.to_string()).into());
            }
            let lock_btn = ui.add(
                Button::icon(lock_icon)
                    .with_tooltip(if is_locked {
                        "Unlock layer"
                    } else {
                        "Lock layer"
                    })
                    .icon_color(lock_color)
                    .hover_icon_color(t.palette.text.primary),
            );
            if lock_btn.clicked() {
                commands.push_back(ActorCommand::ToggleActorLock(label.to_string()).into());
            }
        })
        .show(ui, row_id);

    // ── Drag-and-drop reparenting ──
    // Disabled while a filter is active: the partial tree makes drop targets
    // ambiguous (hidden ancestors/descendants can't be valid drop destinations).
    let drag_id = ui.id().with("layer_drag");
    let drag_data_id = drag_id.with("data");

    if !has_filter && response.drag_started && !is_anonymous {
        ui.data_mut(|d| d.insert_temp(drag_data_id, label.to_string()));
    }

    let is_dragging = ui.data(|d| d.get_temp::<String>(drag_data_id)).is_some();
    let is_drop_target = !has_filter && is_dragging && response.hovered && !is_anonymous;
    if is_drop_target {
        let dragged = ui.data(|d| d.get_temp::<String>(drag_data_id)).unwrap_or_default();
        if dragged != label {
            ui.painter().rect_stroke(
                response.row_rect.expand(1.0),
                2,
                egui::Stroke::new(1.5, t.palette.accent.primary),
                egui::StrokeKind::Outside,
            );
        }
    }

    // Drop-to-root indicator: show a line at the top of depth-0 rows when
    // the pointer is within the expanded drop zone but not over the row.
    if is_dragging && depth == 0 {
        let pointer_pos = ui.input(|i| i.pointer.latest_pos());
        let in_drop_zone = pointer_pos.is_some_and(|p| {
            !response.row_rect.contains(p) && response.row_rect.expand(8.0).contains(p)
        });
        if in_drop_zone {
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(response.row_rect.left(), response.row_rect.top()),
                    egui::pos2(response.row_rect.right(), response.row_rect.top() + 2.0),
                ),
                0.0,
                t.palette.accent.primary,
            );
        }
    }

    if !has_filter && is_dragging && ui.input(|i| i.pointer.any_released()) {
        let dragged = ui.data(|d| d.get_temp::<String>(drag_data_id)).unwrap_or_default();
        if !dragged.is_empty() && dragged != label {
            let pointer_pos = ui.input(|i| i.pointer.latest_pos());
            let over_this_row = pointer_pos.is_some_and(|p| response.row_rect.contains(p));
            if over_this_row && is_drop_target {
                commands.push_back(ShellAction::Command(Command::ReparentActor {
                    actor: dragged.clone(),
                    new_parent: Some(label.to_string()),
                }));
            } else if !over_this_row && depth == 0 {
                let over_any_root =
                    pointer_pos.is_some_and(|p| response.row_rect.expand(8.0).contains(p));
                if over_any_root {
                    commands.push_back(ShellAction::Command(Command::ReparentActor {
                        actor: dragged.clone(),
                        new_parent: None,
                    }));
                }
            }
        }
        ui.data_mut(|d| d.remove::<String>(drag_data_id));
    }

    // Right-click context menu for layer rows (use row response so we don't steal left-clicks)
    response.response.context_menu(|ui| {
        let has_multi = selected_actors.len() >= 2;

        // Each row carries an explicit action identity, so a click can never
        // fall through to a destructive action when the menu layout changes.
        #[derive(Clone, Copy)]
        enum LayerMenuAction {
            Duplicate,
            Rename,
            Align(crate::app::commands::Align),
            Distribute(crate::app::commands::Axis),
            Group,
            Ungroup,
            Delete,
        }

        use egui_phosphor::regular as icons;

        let mut menu: Vec<(MenuEntry, Option<LayerMenuAction>)> = Vec::new();
        menu.push((
            MenuEntry::item_with_icon(icons::COPY, "Duplicate"),
            Some(LayerMenuAction::Duplicate),
        ));
        menu.push((
            MenuEntry::item_with_icon(icons::PENCIL_SIMPLE, "Rename…"),
            Some(LayerMenuAction::Rename),
        ));
        if has_multi {
            use crate::app::commands::{Align, Axis};
            menu.push((MenuEntry::separator(), None));
            menu.push((
                MenuEntry::item_with_icon(icons::ALIGN_LEFT, "Align Left"),
                Some(LayerMenuAction::Align(Align::Left)),
            ));
            menu.push((
                MenuEntry::item_with_icon(icons::ALIGN_CENTER_HORIZONTAL_SIMPLE, "Align Center"),
                Some(LayerMenuAction::Align(Align::Center)),
            ));
            menu.push((
                MenuEntry::item_with_icon(icons::ALIGN_RIGHT, "Align Right"),
                Some(LayerMenuAction::Align(Align::Right)),
            ));
            menu.push((
                MenuEntry::item_with_icon(icons::ALIGN_TOP, "Align Top"),
                Some(LayerMenuAction::Align(Align::Top)),
            ));
            menu.push((
                MenuEntry::item_with_icon(icons::ALIGN_CENTER_VERTICAL_SIMPLE, "Align Middle"),
                Some(LayerMenuAction::Align(Align::Middle)),
            ));
            menu.push((
                MenuEntry::item_with_icon(icons::ALIGN_BOTTOM, "Align Bottom"),
                Some(LayerMenuAction::Align(Align::Bottom)),
            ));
            menu.push((MenuEntry::separator(), None));
            menu.push((
                MenuEntry::item_with_icon(
                    icons::ARROWS_OUT_LINE_HORIZONTAL,
                    "Distribute Horizontally",
                ),
                Some(LayerMenuAction::Distribute(Axis::Horizontal)),
            ));
            menu.push((
                MenuEntry::item_with_icon(icons::ARROWS_OUT_LINE_VERTICAL, "Distribute Vertically"),
                Some(LayerMenuAction::Distribute(Axis::Vertical)),
            ));
        }
        menu.push((MenuEntry::separator(), None));
        menu.push((
            MenuEntry::item_with_icon(icons::SELECTION_PLUS, "Group"),
            Some(LayerMenuAction::Group),
        ));
        menu.push((
            MenuEntry::item_with_icon(icons::SQUARES_FOUR, "Ungroup"),
            Some(LayerMenuAction::Ungroup),
        ));
        menu.push((MenuEntry::separator(), None));
        menu.push((
            MenuEntry::item_with_icon(icons::TRASH, "Delete"),
            Some(LayerMenuAction::Delete),
        ));

        let entries: Vec<MenuEntry> = menu.iter().map(|(entry, _)| entry.clone()).collect();
        if let Some(idx) = render_menu(ui, &entries) {
            match menu.get(idx).and_then(|(_, action)| *action) {
                Some(LayerMenuAction::Duplicate) => commands
                    .push_back(ShellAction::Command(Command::DuplicateActor(label.to_string()))),
                Some(LayerMenuAction::Rename) => start_layer_rename(ui, label),
                Some(LayerMenuAction::Align(align)) => {
                    commands.push_back(ShellAction::Command(Command::AlignActors(align)))
                },
                Some(LayerMenuAction::Distribute(axis)) => {
                    commands.push_back(ShellAction::Command(Command::DistributeActors(axis)))
                },
                Some(LayerMenuAction::Group) => {
                    commands.push_back(ActorCommand::GroupSelectedActors.into())
                },
                Some(LayerMenuAction::Ungroup) => {
                    commands.push_back(ActorCommand::UngroupSelectedActors.into())
                },
                Some(LayerMenuAction::Delete) => {
                    selected_actors.clear();
                    selected_actors.insert(label.to_string());
                    commands.push_back(ActorCommand::DeleteSelectedActors.into());
                },
                None => {}, // Separator or out-of-range index: no action.
            }
            ui.close();
        }
    });

    // Double-click a layer name to rename it inline in the panel header bar.
    if response.response.double_clicked() && !is_anonymous {
        start_layer_rename(ui, label);
    }

    if response.chevron_clicked {
        if collapsed_actors.contains(&label_owned) {
            collapsed_actors.remove(&label_owned);
        } else {
            collapsed_actors.insert(label_owned.clone());
        }
    }

    if response.row_clicked || response.drag_started {
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
                _time_ms,
                depth + 1,
                filter_lower,
                has_filter,
            );
        }
    }
}

/// Recursively check whether any actor in the subtree rooted at `label`
/// matches `filter_lower` (case-insensitive substring on the actor label).
fn actor_matches_filter(timeline: &Timeline, label: &str, filter_lower: &str) -> bool {
    let Some(track) = timeline.get_track(label) else {
        return false;
    };
    if label.to_lowercase().contains(filter_lower) {
        return true;
    }
    track
        .children
        .iter()
        .any(|child| actor_matches_filter(timeline, child, filter_lower))
}

// ─── Components Tab ───────────────────────────────────────────────────────

fn components_content_ui(ctx: &mut ComponentsContext<'_>, ui: &mut egui::Ui) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    if ctx.components.is_empty() {
        layout::empty_state(
            ui,
            egui_phosphor::regular::CUBE,
            "No components",
            "Import modules with pub component definitions to see them here.",
        );
        return;
    }

    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        let mut names: Vec<&String> = ctx.components.keys().collect();
        names.sort();
        for name in names {
            let entry = &ctx.components[name];
            let row_id = ui.id().with(name);
            let response = row::Row::new(name)
                .icon(Some(egui_phosphor::regular::CUBE))
                .label_color(t.palette.text.secondary)
                .sense(egui::Sense::click_and_drag())
                .right(|ui| {
                    let jump_btn = ui.add(
                        egui::Button::new(
                            egui::RichText::new(egui_phosphor::regular::ARROW_SQUARE_OUT)
                                .size(TextRole::Micro.size())
                                .color(t.palette.text.muted),
                        )
                        .frame(false),
                    );
                    text_tooltip(ui, jump_btn.id.with("jump_tip"), &jump_btn, "Jump to definition");
                    if jump_btn.clicked() {
                        let patterns = [
                            format!("pub component {}", name),
                            format!("component {}", name),
                        ];
                        let found_line = ctx
                            .source_text
                            .lines()
                            .position(|line| patterns.iter().any(|p| line.trim().starts_with(p)));
                        if let Some(line) = found_line {
                            ctx.commands
                                .push_back(ShellAction::Command(Command::ScrollToLine(line, 0)));
                            // The editor is no longer a sidebar tab; open the
                            // Code tab in the detail region instead.
                            ctx.commands.push_back(ShellAction::View(ViewAction::ShowCode));
                        }
                    }
                })
                .show(ui, row_id);

            if response.response.double_clicked() {
                // Instantiate component with default props
                let label = crate::app::utils::labels::unique_label(None, name);
                let pos = [
                    ctx.scene_dimensions.width as f32 / 2.0,
                    ctx.scene_dimensions.height as f32 / 2.0,
                ];
                ctx.commands.push_back(
                    ActorCommand::CreateActor {
                        ty: (*name).clone(),
                        label,
                        position: pos,
                        props: vec![],
                    }
                    .into(),
                );
            }

            // Drag-to-place: hand the preview panel the actor to create at the
            // drop point. The payload mirrors the double-click instantiate above.
            if response.drag_started {
                super::set_library_drag(
                    ui.ctx(),
                    super::LibraryDragPayload {
                        ty: (*name).clone(),
                        props: vec![],
                    },
                );
            }

            // Slots display
            let slots: Vec<String> = entry
                .definition
                .body
                .iter()
                .filter_map(|stmt| {
                    if let animatix_syntax::ast::Stmt::ActorDecl {
                        label, children, ..
                    } = stmt
                    {
                        if children.iter().any(|item| {
                            matches!(item, animatix_syntax::ast::InlineItem::SlotMarker)
                        }) {
                            Some(label.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            if !slots.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(sp.base.component.icon_slot_width + sp.base.space_2);
                    ui.label(
                        egui::RichText::new(format!("@slots: {}", slots.join(", ")))
                            .size(TextRole::Micro.size())
                            .color(t.palette.accent.cyan),
                    );
                });
            }

            // Show params as sub-label
            if !entry.definition.params.is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(sp.base.component.icon_slot_width + sp.base.space_2);
                    let params: Vec<String> = entry
                        .definition
                        .params
                        .iter()
                        .map(|p| {
                            let default = p
                                .default
                                .as_ref()
                                .map(|v| format!(" = {}", v.to_source()))
                                .unwrap_or_default();
                            format!("{}{}", p.name, default)
                        })
                        .collect();
                    ui.label(
                        egui::RichText::new(params.join(", "))
                            .size(TextRole::Micro.size())
                            .color(t.palette.text.muted),
                    );
                });
            }

            response.response.context_menu(|ui| {
                let entries = vec![MenuEntry::item_with_icon(
                    egui_phosphor::regular::PLUS,
                    "Instantiate",
                )];
                if render_menu(ui, &entries).is_some() {
                    let label = crate::app::utils::labels::unique_label(None, name);
                    let pos = [
                        ctx.scene_dimensions.width as f32 / 2.0,
                        ctx.scene_dimensions.height as f32 / 2.0,
                    ];
                    ctx.commands.push_back(
                        ActorCommand::CreateActor {
                            ty: (*name).clone(),
                            label,
                            position: pos,
                            props: vec![],
                        }
                        .into(),
                    );
                    ui.close();
                }
            });
        }
    });
}

// ─── Assets Tab ───────────────────────────────────────────────────────────

fn assets_content_ui(ctx: &mut AssetsContext<'_>, ui: &mut egui::Ui) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let Some(cache) = ctx.asset_cache else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::IMAGES,
            "No assets loaded",
            "Add Image or SVG actors to populate the asset cache.",
        );
        return;
    };

    let images: Vec<(String, &animatix::timeline::image::SceneImage)> =
        cache.images().map(|(k, v)| (k.clone(), v)).collect();
    let svgs: Vec<(String, &Vec<animatix::timeline::vello_path::VelloPath>)> =
        cache.svg_paths().map(|(k, v)| (k.clone(), v)).collect();

    if images.is_empty() && svgs.is_empty() {
        layout::empty_state(
            ui,
            egui_phosphor::regular::IMAGES,
            "No assets loaded",
            "Add Image or SVG actors to populate the asset cache.",
        );
        return;
    }

    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        if !images.is_empty() {
            layout::section_header(ui, egui_phosphor::regular::IMAGE, "Images", Some(images.len()));
            for (path, _img) in &images {
                let row_id = ui.id().with(path);
                let filename =
                    std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
                let response = row::Row::new(filename)
                    .icon(Some(egui_phosphor::regular::IMAGE))
                    .label_color(t.palette.text.secondary)
                    .sense(egui::Sense::click_and_drag())
                    .show(ui, row_id);

                let props = vec![animatix_syntax::ast::Property {
                    name: "url".into(),
                    value: animatix_syntax::ast::Expr::Str(path.clone()),
                    value_span: None,
                    trailing_comment: None,
                }];
                if response.response.double_clicked() {
                    let label = crate::app::utils::labels::unique_label(None, "image");
                    let pos = [
                        ctx.scene_dimensions.width as f32 / 2.0,
                        ctx.scene_dimensions.height as f32 / 2.0,
                    ];
                    ctx.commands.push_back(
                        ActorCommand::CreateActor {
                            ty: "Image".into(),
                            label,
                            position: pos,
                            props: props.clone(),
                        }
                        .into(),
                    );
                }

                // Drag-to-place onto the canvas (see `components_content_ui`).
                if response.drag_started {
                    super::set_library_drag(
                        ui.ctx(),
                        super::LibraryDragPayload {
                            ty: "Image".into(),
                            props: props.clone(),
                        },
                    );
                }
            }
            ui.add_space(sp.base.space_3);
        }

        if !svgs.is_empty() {
            layout::section_header(ui, egui_phosphor::regular::FILE_SVG, "SVGs", Some(svgs.len()));
            for (path, _svg) in &svgs {
                let row_id = ui.id().with(path);
                let filename =
                    std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
                let response = row::Row::new(filename)
                    .icon(Some(egui_phosphor::regular::FILE_SVG))
                    .label_color(t.palette.text.secondary)
                    .sense(egui::Sense::click_and_drag())
                    .show(ui, row_id);

                let props = vec![animatix_syntax::ast::Property {
                    name: "url".into(),
                    value: animatix_syntax::ast::Expr::Str(path.clone()),
                    value_span: None,
                    trailing_comment: None,
                }];
                if response.response.double_clicked() {
                    let label = crate::app::utils::labels::unique_label(None, "svg");
                    let pos = [
                        ctx.scene_dimensions.width as f32 / 2.0,
                        ctx.scene_dimensions.height as f32 / 2.0,
                    ];
                    ctx.commands.push_back(
                        ActorCommand::CreateActor {
                            ty: "Svg".into(),
                            label,
                            position: pos,
                            props: props.clone(),
                        }
                        .into(),
                    );
                }

                // Drag-to-place onto the canvas (see `components_content_ui`).
                if response.drag_started {
                    super::set_library_drag(
                        ui.ctx(),
                        super::LibraryDragPayload {
                            ty: "Svg".into(),
                            props: props.clone(),
                        },
                    );
                }
            }
        }
    });
}
