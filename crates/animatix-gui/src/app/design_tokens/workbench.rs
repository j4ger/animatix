//! Native workbench colors layered over the shared widget theme.

use egui::Color32;
use eparts::{AppThemeChoice, Theme};

pub(crate) fn theme(choice: AppThemeChoice, system_is_dark: Option<bool>) -> Theme {
    let mut theme = choice.resolve(system_is_dark);
    let dark = choice.is_dark(system_is_dark);
    let palette = &mut theme.palette;
    if dark {
        // The dark matte recedes behind the rendered scene. Panels are
        // lighter and cooler so their boundaries stay visible without boxes.
        palette.surface.base = Color32::from_rgb(13, 16, 22);
        palette.surface.panel = Color32::from_rgb(24, 28, 36);
        palette.text.muted = Color32::from_rgb(143, 154, 173);
    } else {
        palette.surface.base = Color32::from_rgb(233, 237, 243);
        palette.surface.panel = Color32::from_rgb(252, 253, 255);
        palette.text.muted = Color32::from_rgb(89, 99, 117);
    }

    let primary = &mut theme.components.button.primary;
    primary.normal.fg = palette.text.on_accent;
    primary.hover.fg = palette.text.on_accent;
    primary.active.fg = Color32::WHITE;
    primary.selected.fg = Color32::WHITE;

    // A selected layout uses the accent underline and a high-contrast label.
    let ghost = &mut theme.components.button.ghost;
    ghost.active.fg = palette.text.primary;
    ghost.selected.fg = palette.text.primary;
    theme
}
