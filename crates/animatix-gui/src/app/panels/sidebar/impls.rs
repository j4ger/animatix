//! Sidebar render glue: the docked panel, the compact icon rail, tab content
//! dispatch, and the Project and Outline views.

use crate::app::components::button::Button;
use crate::app::panels::{COMPACT_RAIL_WIDTH, CompactDrawer, SIDEBAR_TABS, SidebarTab};

use super::{
    EXPLORER_FILTER_ID, ExplorerContext, LAYERS_FILTER_ID, LayersContext, ScenesContext,
    SidebarContext, assets_content_ui, components_content_ui, explorer_content_ui,
    layers_content_ui, scene_picker_ui,
};

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

        ui.add_space(sp.base.space_2);
        render_sidebar_tab_bar(ui, &mut active_tab);
        ui.add_space(sp.base.space_4);

        if prev_tab != active_tab {
            // Clear the file-tree / layer filters when leaving their tabs.
            if active_tab != SidebarTab::Project {
                ui.data_mut(|d| d.remove::<String>(egui::Id::unique(EXPLORER_FILTER_ID)));
            }
            if active_tab != SidebarTab::Outline {
                ui.data_mut(|d| d.remove::<String>(egui::Id::unique(LAYERS_FILTER_ID)));
            }
        }
        ui.allocate_ui_with_layout(
            ui.available_size(),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
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
    let rail_width = COMPACT_RAIL_WIDTH;
    ui.allocate_ui_with_layout(
        egui::Vec2::new(rail_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.add_space(sp.base.space_3);
            for (tab, icon, id) in SIDEBAR_TABS {
                let label = crate::app::i18n::text(ui, id);
                // The dot tracks both the selected view and whether its drawer
                // is actually open, so the rail reflects what is on screen.
                let active =
                    *ctx.sidebar_tab == tab && *ctx.compact_drawer == Some(CompactDrawer::Sidebar);
                let resp = ui.add(Button::icon(icon).active(active).with_tooltip(label.clone()));
                resp.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), &label)
                });
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
    match active_tab {
        SidebarTab::Project => project_ui(ctx, ui),
        SidebarTab::Outline => {
            let active = ctx.document.active_timeline_ref();
            scene_picker_ui(
                &mut ScenesContext {
                    composition: ctx.document.composition.as_ref(),
                    active_scene: active.as_ref().and_then(|active| active.scene_name),
                    commands: ctx.commands,
                },
                ui,
            );
            layers_content_ui(
                &mut LayersContext {
                    timeline: active.map(|active| active.timeline),
                    selected_actors: ctx.selected_actors,
                    collapsed_actors: ctx.collapsed_actors,
                },
                ui,
            );
        },
    }
}

fn project_ui(ctx: &mut SidebarContext<'_>, ui: &mut egui::Ui) {
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let filter = super::explorer::explorer_filter_ui(ui);
    if ctx.file_tree_loading {
        ui.label(crate::app::i18n::text(ui, "gui-explorer-loading"));
    }
    egui::ScrollArea::vertical()
        .id_salt("navigator_project")
        .auto_shrink([false; 2])
        .show_viewport(ui, |ui, viewport| {
            explorer_content_ui(
                &mut ExplorerContext {
                    current_file: &ctx.document.file_path,
                    expanded_dirs: ctx.expanded_dirs,
                    file_tree: ctx.file_tree,
                    file_tree_revision: ctx.file_tree_revision,
                    commands: ctx.commands,
                },
                ui,
                &filter,
                viewport,
            );
            ui.add_space(sp.base.space_2);
            ui.separator();
            egui::CollapsingHeader::new(crate::app::i18n::text(ui, "gui-project-resources"))
                .id_salt("project_resources")
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new(crate::app::i18n::text(ui, "gui-inspector-assets"))
                            .small()
                            .strong(),
                    );
                    assets_content_ui(ctx.document, ui);
                    ui.add_space(sp.base.space_2);
                    ui.label(
                        egui::RichText::new(crate::app::i18n::text(ui, "gui-project-components"))
                            .small()
                            .strong(),
                    );
                    components_content_ui(&ctx.document.components, ui);
                });
        });
}

fn render_sidebar_tab_bar(ui: &mut egui::Ui, active_tab: &mut SidebarTab) {
    let labels = SIDEBAR_TABS.map(|entry| crate::app::i18n::text(ui, entry.2));
    let label_refs = labels.each_ref().map(String::as_str);
    let icons = SIDEBAR_TABS.map(|entry| entry.1);
    let mut selected = SIDEBAR_TABS.iter().position(|entry| entry.0 == *active_tab).unwrap_or(0);
    if eparts::TabBar::new(ui.next_auto_id(), &mut selected, &label_refs)
        .with_icons(&icons)
        .underlined()
        .uniform_content()
        .show(ui)
        .changed()
    {
        *active_tab = SIDEBAR_TABS[selected].0;
    }
}
