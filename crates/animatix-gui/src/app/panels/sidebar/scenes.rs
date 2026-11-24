//! Read-only composition navigation. Ordering is authored in the DSL.
use super::*;
use eparts::widget::UiExt;

pub(super) fn scene_picker_ui(ctx: &mut ScenesContext<'_>, ui: &mut egui::Ui) {
    let Some(composition) = ctx.composition.filter(|c| c.declaration_order.len() > 1) else {
        return;
    };
    let active = ctx.active_scene.unwrap_or("Select a scene");
    egui::ComboBox::from_id_salt("navigator_scene")
        .selected_text(active)
        .width(ui.available_width())
        .wrap_mode(egui::TextWrapMode::Truncate)
        .show_ui(ui, |ui| {
            for name in &composition.declaration_order {
                let info = scene_info(composition, name);
                let choice = ui
                    .stable_selectable_label(ctx.active_scene == Some(name.as_str()), name)
                    .on_hover_text(info);
                if choice.clicked() {
                    ctx.commands.push_back(Command::SelectScene(name.clone()).into());
                }
            }
        })
        .response
        .on_hover_text("Switch scene");
}

fn scene_info(composition: &animatix::composition::Composition, name: &str) -> String {
    let mut info = name.to_owned();
    if let Some(start) = composition.scene_start_times.get(name) {
        info.push_str(&format!("\nStarts at {start:.2}s global"));
    }
    if let Some(edge) = composition.edges.get(name) {
        info.push_str(&format!(
            "\n→ {} · {}ms transition",
            edge.to_scene, edge.transition.duration_ms
        ));
    }
    info
}
