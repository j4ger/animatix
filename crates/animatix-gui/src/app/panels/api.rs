//! Panel-level helpers shared by the app shell: sidebar view labels, the
//! ruler tick spacing,
//! and the uniform panel frame.

use super::SidebarTab;

/// Human-readable label for a top-level sidebar view.
pub(crate) fn sidebar_tab_label(tab: SidebarTab) -> &'static str {
    match tab {
        SidebarTab::Project => "Project",
        SidebarTab::Outline => "Outline",
        SidebarTab::Library => "Library",
    }
}

/// Compute a "nice" tick interval for ruler marks.
/// Produces round numbers (1, 2, 5, 10, 20, 50, 100, ...).
pub(in crate::app) fn nice_tick_interval(visible_range: f32, target_ticks: f32) -> f32 {
    let raw = (visible_range / target_ticks).abs();
    if raw <= 0.0 {
        return 1.0;
    }
    let magnitude = 10.0_f32.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let nice_mul = if normalized < 1.5 {
        1.0
    } else if normalized < 3.5 {
        2.0
    } else if normalized < 7.5 {
        5.0
    } else {
        10.0
    };
    nice_mul * magnitude
}

/// Uniform panel frame: 8 px padding, transparent fill.
pub(crate) fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::same(8))
}
