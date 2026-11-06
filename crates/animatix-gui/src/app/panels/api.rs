//! Panel-level helpers shared by the app shell: sidebar view labels, the
//! library drag payload store, default-actor selection, ruler tick spacing,
//! and the uniform panel frame.

use animatix::primitives;

use super::{LibraryDragPayload, SidebarTab};

/// Human-readable label for a top-level sidebar view.
pub(crate) fn sidebar_tab_label(tab: SidebarTab) -> &'static str {
    match tab {
        SidebarTab::Project => "Project",
        SidebarTab::Outline => "Outline",
        SidebarTab::Library => "Library",
    }
}

/// Well-known egui data id for the in-flight library drag payload.
///
/// egui's data store lives on the shared `Context`, so a payload inserted by a
/// sidebar row is readable from the preview panel in the same frame.
pub(crate) fn library_drag_id() -> egui::Id {
    egui::Id::unique("library_drag_payload")
}

/// Store the in-flight library drag payload.
pub(crate) fn set_library_drag(ctx: &egui::Context, payload: LibraryDragPayload) {
    ctx.data_mut(|d| d.insert_temp(library_drag_id(), payload));
}

/// Read the in-flight library drag payload, if a library row is being dragged.
pub(crate) fn library_drag(ctx: &egui::Context) -> Option<LibraryDragPayload> {
    ctx.data(|d| d.get_temp(library_drag_id()))
}

/// Drop the in-flight library drag payload (after a drop, Escape, or a release
/// outside the canvas).
pub(crate) fn clear_library_drag(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<LibraryDragPayload>(library_drag_id()));
}

/// Label stem for a dropped payload: `Image`/`Svg` get lower-case asset stems,
/// components keep their declared name.
pub(crate) fn library_drag_label_base(payload: &LibraryDragPayload) -> String {
    match payload.ty.as_str() {
        "Image" => "image".to_string(),
        "Svg" => "svg".to_string(),
        other => other.to_string(),
    }
}

/// Returns the canonical default actor type: the first non-advanced Shape actor.
pub(crate) fn default_actor_type() -> &'static str {
    primitives::primitive_catalog()
        .iter()
        .find(|meta| meta.category == animatix::timeline::ActorCategory::Shape && !meta.advanced)
        .and_then(|meta| meta.static_type_name())
        .unwrap_or("Rect")
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
