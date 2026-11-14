//! Read-only composition navigation. Ordering is authored in the DSL.
use super::*;

pub(super) fn scenes_content_ui(ctx: &mut ScenesContext<'_>, ui: &mut egui::Ui) {
    let Some(composition) = ctx.composition else {
        layout::empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "Single scene",
            "Scene navigation appears for a composition.",
        );
        return;
    };
    ui.label(egui::RichText::new("Composition order · read only").small().weak());
    egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
        for name in &composition.declaration_order {
            let selected = ctx.active_scene == Some(name.as_str());
            if ui.selectable_label(selected, name).clicked() {
                ctx.commands.push_back(Command::SelectScene(name.clone()).into());
            }
            if let Some(start) = composition.scene_start_times.get(name) {
                ui.label(
                    egui::RichText::new(format!("starts at {start:.2}s global")).small().weak(),
                );
            }
            if let Some(edge) = composition.edges.get(name) {
                ui.label(
                    egui::RichText::new(format!(
                        "→ {} · {}ms transition",
                        edge.to_scene, edge.transition.duration_ms
                    ))
                    .small()
                    .weak(),
                );
            }
            ui.add_space(6.0);
        }
    });
}
