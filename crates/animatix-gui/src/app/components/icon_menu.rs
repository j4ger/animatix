/// Icon-only menu trigger with a readable tooltip and assistive-technology name.
/// Keep menu entries textual: the trigger is compact, the choices are discoverable.
pub(crate) fn icon_menu<R>(
    ui: &mut egui::Ui,
    icon: &'static str,
    label: &str,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::Response {
    let response = ui.add(crate::app::components::button::Button::icon(icon));
    egui::Popup::menu(&response).show(contents);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label));
    response.on_hover_text(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::components::button::Button;

    fn click_output(render: impl Fn(&mut egui::Ui) -> egui::Response) -> egui::FullOutput {
        let context = egui::Context::default();
        let mut center = egui::Pos2::ZERO;
        let mut initial = context.run_ui(egui::RawInput::default(), |ui| {
            center = render(ui).rect.center();
        });
        initial.textures_delta.clear();
        let input = |pressed| egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(center),
                egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        };
        let mut pressed = context.run_ui(input(true), |ui| {
            render(ui);
        });
        pressed.textures_delta.clear();
        let mut released = context.run_ui(input(false), |ui| {
            render(ui);
        });
        released.textures_delta.clear();
        released
    }

    #[test]
    fn icon_button_click_reports_tooltip_instead_of_font_glyph() {
        let output = click_output(|ui| {
            ui.add(
                Button::icon(egui_phosphor::regular::REPEAT)
                    .with_tooltip("Set loop to visible interval · preview only"),
            )
        });
        assert!(output.platform_output.events.iter().any(|event| matches!(event,
            egui::output::OutputEvent::Clicked(info) if info.label.as_deref() == Some("Set loop to visible interval · preview only")
        )));
    }

    #[test]
    fn icon_menu_click_reports_readable_name() {
        let output = click_output(|ui| {
            icon_menu(ui, egui_phosphor::regular::SQUARES_FOUR, "Workspace layout", |ui| {
                ui.label("Review");
            })
        });
        assert!(output.platform_output.events.iter().any(|event| matches!(event,
            egui::output::OutputEvent::Clicked(info) if info.label.as_deref() == Some("Workspace layout")
        )));
    }

    #[test]
    fn disabled_clear_loop_does_not_emit_click() {
        let output = click_output(|ui| {
            ui.add_enabled(
                false,
                Button::icon(egui_phosphor::regular::X).with_tooltip("Clear review loop"),
            )
        });
        assert!(
            !output
                .platform_output
                .events
                .iter()
                .any(|event| matches!(event, egui::output::OutputEvent::Clicked(_)))
        );
    }
}
