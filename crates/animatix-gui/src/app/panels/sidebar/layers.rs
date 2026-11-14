//! Read-only scene outline. Selection is workspace state, never a source edit.
use super::*;

pub(super) fn layers_content_ui(ctx: &mut LayersContext<'_>, ui: &mut egui::Ui) {
    let Some(timeline) = ctx.timeline else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::STACK,
            "No built scene",
            "Write a scene in the source editor.",
        );
        return;
    };
    let filter_id = egui::Id::unique(LAYERS_FILTER_ID);
    let mut filter = ui.data(|d| d.get_temp::<String>(filter_id)).unwrap_or_default();
    if TextField::new(&mut filter)
        .placeholder("Find an object…")
        .cleanable(true)
        .desired_width(ui.available_width())
        .show(ui)
        .changed
    {
        ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
    }
    if let Some(scene) = ctx.active_scene {
        ui.label(egui::RichText::new(scene).small());
    }
    ui.label(egui::RichText::new("Select to inspect · edit in source").small().weak());
    let filter = filter.to_lowercase();
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        for &id in timeline.world().root_entities() {
            if let Some(label) = timeline.get_entity_label(id) {
                actor_row(
                    ui,
                    timeline,
                    label,
                    ctx.selected_actors,
                    ctx.collapsed_actors,
                    &filter,
                    0,
                );
            }
        }
    });
}

fn matches_subtree(timeline: &Timeline, label: &str, filter: &str) -> bool {
    label.to_lowercase().contains(filter)
        || timeline
            .child_labels(label)
            .any(|child| matches_subtree(timeline, child, filter))
}

fn actor_row(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    label: &str,
    selected: &mut HashSet<String>,
    collapsed: &mut HashSet<String>,
    filter: &str,
    depth: usize,
) {
    if !filter.is_empty() && !matches_subtree(timeline, label, filter) {
        return;
    }
    let children: Vec<_> = timeline.child_labels(label).collect();
    let expanded = !collapsed.contains(label) || !filter.is_empty();
    ui.push_id(label, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(depth as f32 * 12.0);
            if !children.is_empty() {
                if ui.small_button(if expanded { "▾" } else { "▸" }).clicked() {
                    if expanded {
                        collapsed.insert(label.to_owned());
                    } else {
                        collapsed.remove(label);
                    }
                }
            } else {
                ui.add_space(17.0);
            }
            if let Some(id) = timeline.get_entity_id(label) {
                ui.label(crate::app::icons::actor_icon_for_track(id, timeline).as_ref());
            }
            let response = ui.selectable_label(selected.contains(label), label);
            if response.clicked() {
                let multi = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                if !multi {
                    selected.clear();
                }
                if multi && selected.contains(label) {
                    selected.remove(label);
                } else {
                    selected.insert(label.to_owned());
                }
            }
            response.context_menu(|ui| {
                if ui.button("Copy object name").clicked() {
                    ui.ctx().copy_text(label.to_owned());
                    ui.close();
                }
            });
        });
    });
    if expanded {
        for child in children {
            actor_row(ui, timeline, child, selected, collapsed, filter, depth + 1);
        }
    }
}
