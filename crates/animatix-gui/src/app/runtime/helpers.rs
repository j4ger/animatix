use super::*;

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
    use eparts::tokens::spatial::{RADIUS_M, RADIUS_XL};
    style.visuals.window_corner_radius = egui::CornerRadius::same(RADIUS_XL as u8);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(RADIUS_M as u8);
    style.visuals.popup_shadow = theme.elevation_raised();
    style.visuals.window_shadow = theme.elevation_overlay();
    style.visuals.window_stroke = egui::Stroke::new(1.0, theme.palette.border.subtle);
    style.visuals.hyperlink_color = theme.palette.accent.primary;

    // Disable selectable labels globally (we handle selection manually)
    style.interaction.selectable_labels = false;

    ctx.set_global_style(style);
}
