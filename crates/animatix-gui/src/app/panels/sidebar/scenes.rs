//! Read-only composition navigation. Ordering is authored in the DSL.
use super::*;
use eparts::widget::UiExt;

pub(super) fn scene_picker_ui(ctx: &mut ScenesContext<'_>, ui: &mut egui::Ui) {
    let Some(composition) = ctx.composition.filter(|c| c.declaration_order.len() > 1) else {
        return;
    };
    let active = ctx
        .active_scene
        .map(str::to_owned)
        .unwrap_or_else(|| crate::app::i18n::text(ui, "gui-scenes-pick"));
    egui::ComboBox::from_id_salt("navigator_scene")
        .selected_text(active)
        .width(ui.available_width())
        .wrap_mode(egui::TextWrapMode::Truncate)
        .show_ui(ui, |ui| {
            for name in &composition.declaration_order {
                let info = scene_info(ui, composition, name);
                let choice = ui
                    .stable_selectable_label(ctx.active_scene == Some(name.as_str()), name)
                    .on_hover_text(info);
                if choice.clicked() {
                    ctx.commands.push_back(Command::SelectScene(name.clone()).into());
                }
            }
        })
        .response
        .on_hover_text(crate::app::i18n::text(ui, "gui-scene-switch"));
}

fn scene_info(
    ui: &egui::Ui,
    composition: &animatix::composition::Composition,
    name: &str,
) -> String {
    let mut info = name.to_owned();
    if let Some(start) = composition.scene_start_times.get(name) {
        info.push('\n');
        info.push_str(&crate::app::i18n::render(
            ui,
            "gui-scenes-start",
            &[("time", format!("{start:.2}"))],
        ));
    }
    if let Some(edge) = composition.edges.get(name) {
        info.push('\n');
        info.push_str(&crate::app::i18n::render(
            ui,
            "gui-scenes-transition",
            &[
                ("scene", edge.to_scene.clone()),
                ("duration", edge.transition.duration_ms.to_string()),
            ],
        ));
    }
    info
}
