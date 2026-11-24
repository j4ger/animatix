//! Imported component inventory.

use super::*;

pub(super) fn components_content_ui(
    components: &HashMap<String, animatix_syntax::module::ComponentEntry>,
    ui: &mut egui::Ui,
) {
    let t = eparts::theme(ui);
    let sp = crate::app::design_tokens::spatial::spatial(ui);
    if components.is_empty() {
        ui.label(egui::RichText::new("No imported components.").small().weak());
        return;
    }

    ui.scope(|ui| {
        let mut names: Vec<&String> = components.keys().collect();
        names.sort();
        for name in names {
            let entry = &components[name];
            let row_id = ui.scope_id().with(name);
            row::Row::new(name)
                .icon(Some(egui_phosphor::regular::CUBE))
                .label_color(t.palette.text.secondary)
                .sense(egui::Sense::hover())
                .show(ui, row_id);

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
        }
    });
}
