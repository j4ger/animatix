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
    let filter = filter.to_lowercase();
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
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
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    let width = ui.available_width();
    let indent = (depth as f32 * sp.base.component.icon_slot_width).min((width - 72.0).max(0.0));
    let icon = timeline
        .get_entity_id(label)
        .map(|id| crate::app::icons::actor_icon_for_track(id, timeline));
    let id = ui.scope_id().with(label);
    let response = row::Row::new(label)
        .indent(indent)
        .icon(icon.as_deref())
        .has_children(!children.is_empty())
        .expanded(expanded)
        .selected(selected.contains(label))
        .show(ui, id);
    if response.chevron_clicked {
        if expanded {
            collapsed.insert(label.to_owned());
        } else {
            collapsed.remove(label);
        }
    } else if response.row_clicked {
        let multi = ui.input(|i| i.modifiers.shift || i.modifiers.command);
        if !multi {
            selected.clear();
        }
        if !multi || !selected.remove(label) {
            selected.insert(label.to_owned());
        }
    }
    response.response.on_hover_text(label).context_menu(|ui| {
        if ui.button("Copy object name").clicked() {
            ui.ctx().copy_text(label.to_owned());
            ui.close();
        }
    });
    if expanded {
        for child in children {
            actor_row(ui, timeline, child, selected, collapsed, filter, depth + 1);
        }
    }
}
