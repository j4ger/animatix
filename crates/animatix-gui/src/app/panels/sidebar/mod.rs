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
use crate::app::panels::{CompactDrawer, SIDEBAR_TABS, SidebarTab};
use crate::app::{FileTreeEntry, PreviewPaneState};
use crate::editor::EditorBuffer;
use eparts::TextField;

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
    crate::app::panels::panel_frame().show(ui, |ui| {
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

mod assets;
mod components;
mod explorer;
mod layers;
mod scenes;

use assets::assets_content_ui;
use components::components_content_ui;
use explorer::explorer_content_ui;
use layers::layers_content_ui;
use scenes::scenes_content_ui;
