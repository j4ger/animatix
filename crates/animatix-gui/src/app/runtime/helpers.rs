use super::*;

pub(super) fn live_preview_status(
    preview: &PreviewPaneState,
    active_scene: Option<&str>,
) -> String {
    if let Some(scene) = active_scene {
        format!(
            "{} \u{2022} t = {:.2}s / {:.2}s",
            scene,
            preview.playback.current_time_s(),
            preview.playback.duration_s
        )
    } else {
        format!(
            "Live preview \u{2022} t = {:.2}s / {:.2}s",
            preview.playback.current_time_s(),
            preview.playback.duration_s
        )
    }
}

pub(crate) fn install_theme(ctx: &egui::Context, theme: &eparts::Theme, dark: bool) {
    let sp = spatial_from_ctx(ctx);
    let mut style = (*ctx.global_style()).clone();

    // Tighter spacing
    style.spacing.item_spacing = Vec2::new(sp.base.space_2, sp.base.space_2);
    style.spacing.button_padding = Vec2::new(sp.base.space_4, sp.base.space_2);
    style.spacing.window_margin = egui::Margin::same(sp.base.space_4 as i8);
    style.spacing.indent = sp.base.component.icon_slot_width;

    // Map the eparts Theme onto egui Visuals so stock widgets match the palette.
    style.visuals = theme.to_visuals(dark);

    // Disable selectable labels globally (we handle selection manually)
    style.interaction.selectable_labels = false;

    ctx.set_global_style(style);
}
