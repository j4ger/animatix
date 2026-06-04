//! Sidebar panel: file explorer tree and layer tree.
//!
//! Focused context struct borrows only the fields the sidebar needs.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use egui::{RichText, Vec2};

use crate::app::commands::{ActionQueue, Command, ShellAction};
use crate::app::components::{button, layout, row};
use crate::app::components::context_menu::{render_menu, MenuEntry};
use crate::app::design_tokens::*;
use crate::app::icons::actor_icon_str;
use crate::app::panels::SidebarTab;
use crate::app::{FileTreeEntry, PreviewPaneState};
use crate::editor::EditorBuffer;
use animatix_syntax::diagnostics::Diagnostic;
use animatix_syntax::to_source::ToSource;
use animatix::timeline::{Timeline, SceneDimensions};

/// Id used to persist the explorer filter string in egui's data store.
const EXPLORER_FILTER_ID: &str = "explorer_filter";

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
    pub diagnostics: &'a [Diagnostic],
    pub source_dirty: &'a mut String,
    pub is_playing: bool,
    pub components: &'a HashMap<String, animatix_syntax::module::ComponentEntry>,
    pub asset_cache: Option<&'a animatix::timeline::assets::AssetCache>,
    pub scene_dimensions: SceneDimensions,
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
    pub composition: Option<&'a animatix::composition::Composition>,
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

pub(crate) struct EditorContext<'a> {
    pub editor: &'a mut EditorBuffer,
    pub diagnostics: &'a [Diagnostic],
    pub source_dirty: &'a mut String,
    pub commands: &'a mut ActionQueue,
    pub preview: &'a mut PreviewPaneState,
    pub is_playing: bool,
}

pub(crate) struct ComponentsContext<'a> {
    pub components: &'a HashMap<String, animatix_syntax::module::ComponentEntry>,
    pub commands: &'a mut ActionQueue,
    pub scene_dimensions: SceneDimensions,
}

pub(crate) struct AssetsContext<'a> {
    pub asset_cache: Option<&'a animatix::timeline::assets::AssetCache>,
    pub commands: &'a mut ActionQueue,
    pub scene_dimensions: SceneDimensions,
}

pub(crate) fn sidebar_ui(ctx: &mut SidebarContext<'_>, ui: &mut egui::Ui) {
    super::panel_frame().show(ui, |ui| {
        let mut active_tab = *ctx.sidebar_tab;
        let prev_tab = *ctx.sidebar_tab;

        render_sidebar_tab_bar(ui, &mut active_tab);
        ui.add_space(SPACE_M);

        // Slide-in animation on tab switch
        let content_offset_id = ui.id().with("sidebar_slide");
        if prev_tab != active_tab {
            ui.ctx().animate_value_with_time(content_offset_id, 6.0, 0.0);
            // Clear explorer filter when switching away from the Explorer tab
            if active_tab != SidebarTab::Explorer {
                ui.data_mut(|d| d.remove::<String>(egui::Id::new(EXPLORER_FILTER_ID)));
            }
        }
        let offset = ui.ctx().animate_value_with_time(content_offset_id, 0.0, 0.12);
        if offset > 0.01 {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
        }

        ui.allocate_ui_with_layout(
            ui.available_size(),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.add_space(offset);
                match active_tab {
                    SidebarTab::Explorer => {
                        let mut ectx = ExplorerContext {
                            current_file: ctx.current_file,
                            expanded_dirs: ctx.expanded_dirs,
                            file_tree: ctx.file_tree,
                            commands: ctx.commands,
                        };
                        explorer_content_ui(&mut ectx, ui);
                    }
                    SidebarTab::Layers => {
                        let mut lctx = LayersContext {
                            timeline: ctx.timeline,
                            composition: ctx.composition,
                            active_scene: ctx.active_scene,
                            selected_actors: ctx.selected_actors,
                            collapsed_actors: ctx.collapsed_actors,
                            commands: ctx.commands,
                            preview: ctx.preview,
                            scene_dimensions: ctx.scene_dimensions,
                            is_composition: ctx.is_composition,
                        };
                        layers_content_ui(&mut lctx, ui);
                    }
                    SidebarTab::Scenes => {
                        let mut sctx = ScenesContext {
                            composition: ctx.composition,
                            active_scene: ctx.active_scene,
                            commands: ctx.commands,
                        };
                        scenes_content_ui(&mut sctx, ui);
                    }
                    SidebarTab::Editor => {
                        let mut ectx = EditorContext {
                            editor: ctx.editor,
                            diagnostics: ctx.diagnostics,
                            source_dirty: ctx.source_dirty,
                            commands: ctx.commands,
                            preview: ctx.preview,
                            is_playing: ctx.is_playing,
                        };
                        editor_content_ui(&mut ectx, ui);
                    }
                    SidebarTab::Components => {
                        let mut cctx = ComponentsContext {
                            components: ctx.components,
                            commands: ctx.commands,
                            scene_dimensions: ctx.scene_dimensions,
                        };
                        components_content_ui(&mut cctx, ui);
                    }
                    SidebarTab::Assets => {
                        let mut actx = AssetsContext {
                            asset_cache: ctx.asset_cache,
                            commands: ctx.commands,
                            scene_dimensions: ctx.scene_dimensions,
                        };
                        assets_content_ui(&mut actx, ui);
                    }
                }
            },
        );

        *ctx.sidebar_tab = active_tab;
    });
}

fn render_sidebar_tab_bar(ui: &mut egui::Ui, active_tab: &mut SidebarTab) {
    let tabs = [
        (SidebarTab::Explorer, egui_phosphor::regular::FOLDER, "Explorer"),
        (SidebarTab::Layers, egui_phosphor::regular::STACK, "Layers"),
        (SidebarTab::Scenes, egui_phosphor::regular::FILM_STRIP, "Scenes"),
        (SidebarTab::Components, egui_phosphor::regular::CUBE, "Components"),
        (SidebarTab::Assets, egui_phosphor::regular::IMAGES, "Assets"),
        (SidebarTab::Editor, egui_phosphor::regular::PENCIL_SIMPLE, "Editor"),
    ];
    if let Some(new_tab) = layout::pill_tab_bar(ui, *active_tab, &tabs) {
        *active_tab = new_tab;
    }
}

fn editor_content_ui(ctx: &mut EditorContext<'_>, ui: &mut egui::Ui) {
    ctx.editor.set_diagnostics(ctx.diagnostics);
    let response = ctx.editor.show(ui);
    if response.changed() || ctx.editor.text() != ctx.source_dirty.as_str() {
        *ctx.source_dirty = ctx.editor.text().to_string();
        ctx.commands.push_back(ShellAction::Command(Command::EditorChanged));
    }
    if let Some(time_s) = ctx.editor.pending_scrub_to_time.take() {
        ctx.commands.push_back(ShellAction::Command(Command::ScrubTo(time_s)));
        if !ctx.is_playing {
            ctx.commands.push_back(ShellAction::Command(Command::TogglePlayback));
        }
    }
}

fn explorer_content_ui(ctx: &mut ExplorerContext<'_>, ui: &mut egui::Ui) {
    // ── Filter input ────────────────────────────────────────────────────────
    let filter_id = egui::Id::new(EXPLORER_FILTER_ID);
    let mut filter = ui.data(|d| d.get_temp::<String>(filter_id)).unwrap_or_default();

    ui.horizontal(|ui| {
        ui.add_space(SPACE_S);
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
    ui.add_space(SPACE_S);

    // Import module button
    ui.horizontal(|ui| {
        ui.add_space(SPACE_S);
        if ui
            .button(
                RichText::new(format!("{} Import module", egui_phosphor::regular::DOWNLOAD_SIMPLE))
                    .size(FONT_SIZE_S)
                    .color(ACCENT_BLUE),
            )
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new().add_filter("Animatix", &["amx"]).pick_file() {
                if let Ok(relative) = path.strip_prefix(&ctx.current_file.parent().unwrap_or(std::path::Path::new("."))) {
                    let rel_str = relative.to_string_lossy().to_string();
                    ctx.commands.push_back(ShellAction::Command(Command::ImportModule(rel_str)));
                } else {
                    let abs_str = path.to_string_lossy().to_string();
                    ctx.commands.push_back(ShellAction::Command(Command::ImportModule(abs_str)));
                }
            }
        }
    });
    ui.add_space(SPACE_S);

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
                let color = if is_amx { Some(ACCENT_BLUE) } else { None };
                (Some(file_icon), color)
            };

            let row_id = ui.id().with(entry.path.display().to_string());
            let path = entry.path.clone();
            let is_dir = entry.is_dir;
            let response = row::Row::new(&entry.name)
                .indent(entry.depth as f32 * ICON_SLOT_WIDTH)
                .selected(is_selected)
                .icon(icon)
                .label_color(label_color.unwrap_or(TEXT_SECONDARY))
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
                        ctx.commands.push_back(ShellAction::Command(Command::ToggleExpandDir(path.clone())));
                    } else {
                        ctx.commands.push_back(ShellAction::Command(Command::OpenFile(path.clone())));
                    }
                    ui.close();
                }
            });

            if response.chevron_clicked {
                ctx.commands.push_back(ShellAction::Command(Command::ToggleExpandDir(path.clone())));
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

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
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
                let transition_hint = composition
                    .edges
                    .get(scene_name)
                    .map(|edge| {
                        format!("{} {} ({:.0}ms)", egui_phosphor::regular::ARROW_RIGHT, edge.to_scene, edge.transition.duration_ms)
                    });

                let response = row::Row::new(scene_name)
                    .selected(is_active)
                    .icon(Some(egui_phosphor::regular::FILM_STRIP))
                    .label_color(if is_active { ACCENT_BLUE } else { TEXT_SECONDARY })
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
                        let line_y = if p.y < center { response.row_rect.top() } else { response.row_rect.bottom() };
                        ui.painter().line_segment(
                            [egui::pos2(response.row_rect.left(), line_y), egui::pos2(response.row_rect.right(), line_y)],
                            egui::Stroke::new(2.0, ACCENT_BLUE),
                        );
                    }
                }

                // Click to activate scene
                if response.row_clicked {
                    ctx.commands.push_back(ShellAction::Command(Command::SelectScene(scene_name.clone())));
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
                            0 => ctx.commands.push_back(ShellAction::Command(Command::SelectScene(scene_name.clone()))),
                            2 => ctx.commands.push_back(ShellAction::Command(Command::DuplicateScene(scene_name.clone()))),
                            3 => ctx.commands.push_back(ShellAction::Command(Command::DeleteScene(scene_name.clone()))),
                            _ => {}
                        }
                        ui.close();
                    }
                });

                // Sub-label: duration + transition
                if is_active {
                    ui.horizontal(|ui| {
                        ui.add_space(ICON_SLOT_WIDTH + SPACE_S);
                        ui.label(
                            RichText::new(&duration_hint)
                                .size(FONT_SIZE_XS)
                                .color(TEXT_MUTED),
                        );
                    });
                    if let Some(hint) = transition_hint {
                        ui.horizontal(|ui| {
                            ui.add_space(ICON_SLOT_WIDTH + SPACE_S);
                            ui.label(
                                RichText::new(hint)
                                    .size(FONT_SIZE_XS)
                                    .color(TEXT_MUTED),
                            );
                        });
                    }
                    ui.add_space(SPACE_S);
                }
            }

            // Handle drop (outside the loop so is_dragging is in scope)
            let drag_active = ui.data(|d| d.get_temp::<(usize, String)>(drag_data_id)).is_some();
            if drag_active && ui.input(|i| i.pointer.any_released()) {
                if let Some((from_idx, _dragged_name)) = ui.data(|d| d.get_temp::<(usize, String)>(drag_data_id)) {
                    let to_idx = ui.data(|d| d.get_temp::<usize>(drop_index_id)).unwrap_or(from_idx);
                    if from_idx != to_idx && to_idx <= scene_names.len() {
                        let mut new_order = scene_names.clone();
                        let removed = new_order.remove(from_idx);
                        let insert_at = if to_idx > from_idx { to_idx - 1 } else { to_idx };
                        new_order.insert(insert_at.min(new_order.len()), removed);
                        ctx.commands.push_back(ShellAction::Command(Command::ReorderScenes(new_order)));
                    }
                    ui.data_mut(|d| {
                        d.remove::<(usize, String)>(drag_data_id);
                        d.remove::<usize>(drop_index_id);
                    });
                }
            }
        });
}

fn layers_content_ui(ctx: &mut LayersContext<'_>, ui: &mut egui::Ui) {
    // For compositions, use the active scene's timeline
    let timeline = ctx.timeline.or_else(|| {
        let comp = ctx.composition?;
        let scene_name = ctx.active_scene?;
        comp.scenes.get(scene_name).map(|s| &s.timeline)
    });
    let Some(timeline) = timeline else {
        layout::empty_state(ui, egui_phosphor::regular::FILM_STRIP, "No timeline loaded", "");
        return;
    };

    // Show which scene's actors are being displayed
    if ctx.is_composition {
        if let Some(scene_name) = ctx.active_scene.as_ref() {
            ui.horizontal(|ui| {
                ui.add_space(SPACE_L);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} {}", egui_phosphor::regular::FILM_STRIP, scene_name))
                            .size(FONT_SIZE_S)
                            .color(TEXT_MUTED),
                    )
                    .selectable(false),
                );
            });
            ui.add_space(SPACE_S);
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
            ui.add_space(SPACE_XL);
            if ui
                .button(
                    RichText::new(format!("{} Add Actor", egui_phosphor::regular::PLUS))
                        .size(FONT_SIZE_L)
                        .color(ACCENT_BLUE),
                )
                .clicked()
            {
                let label = "rect1".to_string();
                let pos = [
                    ctx.scene_dimensions.width as f32 / 2.0,
                    ctx.scene_dimensions.height as f32 / 2.0,
                ];
                ctx.commands.push_back(ShellAction::Command(Command::CreateActor {
                    ty: super::default_actor_type().into(),
                    label,
                    position: pos,
                    props: vec![],
                }));
            }
        });
        return;
    }

    let time_ms = (ctx.preview.playback.current_time_s() * 1000.0) as u64;
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
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
) {
    

    let Some(track) = timeline.get_track(label) else {
        return;
    };

    let is_selected = selected_actors.contains(label);
    let is_anonymous = label.starts_with("__anon");
    let has_children = !track.children.is_empty();
    let is_expanded = has_children && !collapsed_actors.contains(label);
    let is_visible = track.visible;

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

    let row_id = ui.id().with(label);
    let label_owned = label.to_string();

    // Visibility toggle (eye icon) on the right
    let eye_icon = if is_visible {
        egui_phosphor::regular::EYE
    } else {
        egui_phosphor::regular::EYE_CLOSED
    };
    let eye_color = if is_visible { TEXT_SECONDARY } else { TEXT_DISABLED };

    let is_locked = track.locked;
    let lock_icon = if is_locked {
        egui_phosphor::regular::LOCK_KEY
    } else {
        egui_phosphor::regular::LOCK_KEY_OPEN
    };
    let lock_color = if is_locked { AMBER } else { TEXT_DISABLED };

    let response = row::Row::new(display_label)
        .indent(depth as f32 * ICON_SLOT_WIDTH)
        .selected(is_selected)
        .icon(icon)
        .label_color(label_color.unwrap_or(if is_visible { TEXT_SECONDARY } else { TEXT_DISABLED }))
        .has_children(has_children)
        .expanded(is_expanded)
        .sense(egui::Sense::click_and_drag())
        .right(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(SPACE_XS, 0.0);
            let eye_btn = button::icon_button_colored(
                ui,
                eye_icon,
                if is_visible { "Hide layer" } else { "Show layer" },
                eye_color,
                TEXT_PRIMARY,
            );
            if eye_btn.clicked() {
                commands.push_back(ShellAction::Command(Command::ToggleActorVisibility(label.to_string())));
            }
            let lock_btn = button::icon_button_colored(
                ui,
                lock_icon,
                if is_locked { "Unlock layer" } else { "Lock layer" },
                lock_color,
                TEXT_PRIMARY,
            );
            if lock_btn.clicked() {
                commands.push_back(ShellAction::Command(Command::ToggleActorLock(label.to_string())));
            }
        })
        .show(ui, row_id);

    // ── Drag-and-drop reparenting ──
    let drag_id = ui.id().with("layer_drag");
    let drag_data_id = drag_id.with("data");

    if response.drag_started && !is_anonymous {
        ui.data_mut(|d| d.insert_temp(drag_data_id, label.to_string()));
    }

    let is_dragging = ui.data(|d| d.get_temp::<String>(drag_data_id)).is_some();
    let is_drop_target = is_dragging && response.hovered && !is_anonymous;
    if is_drop_target {
        let dragged = ui.data(|d| d.get_temp::<String>(drag_data_id)).unwrap_or_default();
        if dragged != label {
            ui.painter().rect_stroke(
                response.row_rect.expand(1.0),
                2,
                egui::Stroke::new(1.5, ACCENT_BLUE),
                egui::StrokeKind::Outside,
            );
        }
    }

    if is_dragging && ui.input(|i| i.pointer.any_released()) {
        let dragged = ui.data(|d| d.get_temp::<String>(drag_data_id)).unwrap_or_default();
        if !dragged.is_empty() && dragged != label {
            let pointer_pos = ui.input(|i| i.pointer.latest_pos());
            let over_this_row = pointer_pos.is_some_and(|p| response.row_rect.contains(p));
            if over_this_row && is_drop_target {
                commands.push_back(ShellAction::Command(Command::ReparentActor { actor: dragged.clone(), new_parent: Some(label.to_string()) }));
            } else if !over_this_row && depth == 0 {
                let over_any_root = pointer_pos.is_some_and(|p| {
                    response.row_rect.expand(100.0).contains(p)
                });
                if over_any_root {
                    commands.push_back(ShellAction::Command(Command::ReparentActor { actor: dragged.clone(), new_parent: None }));
                }
            }
        }
        ui.data_mut(|d| d.remove::<String>(drag_data_id));
    }

    // Right-click context menu for layer rows (use row response so we don't steal left-clicks)
    response.response.context_menu(|ui| {
        let entries = vec![
            MenuEntry::item_with_icon(egui_phosphor::regular::COPY, "Duplicate"),
            MenuEntry::item_with_icon(egui_phosphor::regular::TRASH, "Delete"),
        ];
        if let Some(idx) = render_menu(ui, &entries) {
            match idx {
                0 => commands.push_back(ShellAction::Command(Command::DuplicateActor(label.to_string()))),
                1 => {
                    selected_actors.clear();
                    selected_actors.insert(label.to_string());
                    commands.push_back(ShellAction::Command(Command::DeleteSelectedActors));
                }
                _ => {}
            }
            ui.close();
        }
    });

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
            );
        }
    }
}

// ─── Components Tab ───────────────────────────────────────────────────────

fn components_content_ui(ctx: &mut ComponentsContext<'_>, ui: &mut egui::Ui) {
    if ctx.components.is_empty() {
        layout::empty_state(
            ui,
            egui_phosphor::regular::CUBE,
            "No components",
            "Import modules with pub component definitions to see them here.",
        );
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            let mut names: Vec<&String> = ctx.components.keys().collect();
            names.sort();
            for name in names {
                let entry = &ctx.components[name];
                let row_id = ui.id().with(name);
                let response = row::Row::new(name)
                    .icon(Some(egui_phosphor::regular::CUBE))
                    .label_color(TEXT_SECONDARY)
                    .show(ui, row_id);

                if response.row_clicked {
                    // Instantiate component with default props
                    let label = crate::app::utils::labels::unique_label(None, name);
                    let pos = [
                        ctx.scene_dimensions.width as f32 / 2.0,
                        ctx.scene_dimensions.height as f32 / 2.0,
                    ];
                    ctx.commands.push_back(ShellAction::Command(Command::CreateActor {
                        ty: (*name).clone(),
                        label,
                        position: pos,
                        props: vec![],
                    }));
                }

                // Show params as sub-label
                if !entry.definition.params.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(ICON_SLOT_WIDTH + SPACE_S);
                        let params: Vec<String> = entry
                            .definition
                            .params
                            .iter()
                            .map(|p| {
                                let default = p.default.as_ref().map(|v| format!(" = {}", v.to_source())).unwrap_or_default();
                                format!("{}{}", p.name, default)
                            })
                            .collect();
                        ui.label(
                            egui::RichText::new(params.join(", "))
                                .size(FONT_SIZE_XS)
                                .color(TEXT_MUTED),
                        );
                    });
                }

                response.response.context_menu(|ui| {
                    let entries = vec![
                        MenuEntry::item_with_icon(egui_phosphor::regular::PLUS, "Instantiate"),
                    ];
                    if render_menu(ui, &entries).is_some() {
                        let label = crate::app::utils::labels::unique_label(None, name);
                        let pos = [
                            ctx.scene_dimensions.width as f32 / 2.0,
                            ctx.scene_dimensions.height as f32 / 2.0,
                        ];
                        ctx.commands.push_back(ShellAction::Command(Command::CreateActor {
                            ty: (*name).clone(),
                            label,
                            position: pos,
                            props: vec![],
                        }));
                        ui.close();
                    }
                });
            }
        });
}

// ─── Assets Tab ───────────────────────────────────────────────────────────

fn assets_content_ui(ctx: &mut AssetsContext<'_>, ui: &mut egui::Ui) {
    let Some(cache) = ctx.asset_cache else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::IMAGES,
            "No assets loaded",
            "Add Image or SVG actors to populate the asset cache.",
        );
        return;
    };

    let images: Vec<(String, &animatix::timeline::image::SceneImage)> = cache.images().map(|(k, v)| (k.clone(), v)).collect();
    let svgs: Vec<(String, &Vec<animatix::timeline::vello_path::VelloPath>)> = cache.svg_paths().map(|(k, v)| (k.clone(), v)).collect();

    if images.is_empty() && svgs.is_empty() {
        layout::empty_state(
            ui,
            egui_phosphor::regular::IMAGES,
            "No assets loaded",
            "Add Image or SVG actors to populate the asset cache.",
        );
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            if !images.is_empty() {
                layout::section_header(ui, egui_phosphor::regular::IMAGE, "Images", Some(images.len()));
                for (path, _img) in &images {
                    let row_id = ui.id().with(path);
                    let filename = std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
                    let response = row::Row::new(filename)
                        .icon(Some(egui_phosphor::regular::IMAGE))
                        .label_color(TEXT_SECONDARY)
                        .show(ui, row_id);

                    if response.row_clicked {
                        let label = crate::app::utils::labels::unique_label(None, "image");
                        let pos = [
                            ctx.scene_dimensions.width as f32 / 2.0,
                            ctx.scene_dimensions.height as f32 / 2.0,
                        ];
                        ctx.commands.push_back(ShellAction::Command(Command::CreateActor {
                            ty: "Image".into(),
                            label,
                            position: pos,
                            props: vec![animatix_syntax::ast::Property {
                                name: "url".into(),
                                value: animatix_syntax::ast::Expr::Str(path.clone()),
                                value_span: None,
                                trailing_comment: None,
                            }],
                        }));
                    }
                }
                ui.add_space(SPACE_M);
            }

            if !svgs.is_empty() {
                layout::section_header(ui, egui_phosphor::regular::FILE_SVG, "SVGs", Some(svgs.len()));
                for (path, _svg) in &svgs {
                    let row_id = ui.id().with(path);
                    let filename = std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path);
                    let response = row::Row::new(filename)
                        .icon(Some(egui_phosphor::regular::FILE_SVG))
                        .label_color(TEXT_SECONDARY)
                        .show(ui, row_id);

                    if response.row_clicked {
                        let label = crate::app::utils::labels::unique_label(None, "svg");
                        let pos = [
                            ctx.scene_dimensions.width as f32 / 2.0,
                            ctx.scene_dimensions.height as f32 / 2.0,
                        ];
                        ctx.commands.push_back(ShellAction::Command(Command::CreateActor {
                            ty: "Svg".into(),
                            label,
                            position: pos,
                            props: vec![animatix_syntax::ast::Property {
                                name: "url".into(),
                                value: animatix_syntax::ast::Expr::Str(path.clone()),
                                value_span: None,
                                trailing_comment: None,
                            }],
                        }));
                    }
                }
            }
        });
}