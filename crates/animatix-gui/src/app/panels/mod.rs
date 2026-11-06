//! Docking panel implementations — layout, frames, and egui response glue.
//!
//! Ownership boundary: `panels/` owns frame layout and the top-level egui
//! response loop. `preview/` owns screen/scene coordinate transforms and
//! canvas interaction. `shell/` owns modal dialogs and floating overlays.

pub mod behavior;
pub mod curve_plot;
pub mod curves_panel;
pub mod inspector;
pub mod timeline_panel;

pub mod editor;
pub mod preview_panel;
pub mod sidebar;

use animatix::primitives;

pub use crate::app::commands::{PropertyEdit, PropertyValue};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SidebarTab {
    /// Files + assets (merged Explorer/Assets).
    Project,
    /// Composition structure: layers + scenes (merged Layers/Scenes).
    Outline,
    /// Reusable pieces: components.
    Library,
}

/// Top-level sidebar views, shared by the tab bar and the compact icon rail so
/// both stay in sync (icon + tooltip come from one definition).
pub(crate) const SIDEBAR_TABS: [(SidebarTab, &str, &str); 3] = [
    (SidebarTab::Project, egui_phosphor::regular::FOLDER, "Project"),
    (SidebarTab::Outline, egui_phosphor::regular::STACK, "Outline"),
    (SidebarTab::Library, egui_phosphor::regular::CUBE, "Library"),
];

/// Human-readable label for a top-level sidebar view.
pub(crate) fn sidebar_tab_label(tab: SidebarTab) -> &'static str {
    match tab {
        SidebarTab::Project => "Project",
        SidebarTab::Outline => "Outline",
        SidebarTab::Library => "Library",
    }
}

/// Width of the compact-mode sidebar icon rail.
pub(crate) const COMPACT_RAIL_WIDTH: f32 = 48.0;

/// Compact-mode overlay drawers. Only one is open at a time, so a single
/// `Option<CompactDrawer>` covers both (they sit on opposite edges).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompactDrawer {
    /// Sidebar content, promoted to a left overlay (opened from the icon rail).
    Sidebar,
    /// Inspector / Code, promoted to a right overlay.
    Detail,
}

/// An in-flight Library/Asset row drag, carrying the actor to create at the
/// drop point.
///
/// Built by the sidebar rows exactly like the double-click instantiate path
/// (see `components_content_ui` / `assets_content_ui`) and consumed by the
/// preview panel, which owns the scene transform.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LibraryDragPayload {
    /// Actor type name (a component name, or `"Image"` / `"Svg"`).
    pub ty: String,
    /// Properties to attach (e.g. the asset `url`).
    pub props: Vec<animatix_syntax::ast::Property>,
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
pub(super) fn nice_tick_interval(visible_range: f32, target_ticks: f32) -> f32 {
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

pub(super) const RULER_SIZE: f32 = 20.0;

/// Uniform panel frame: 8 px padding, transparent fill.
pub(crate) fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::same(8))
}

#[cfg(test)]
mod tests {
    use super::{
        LibraryDragPayload, clear_library_drag, library_drag, library_drag_id,
        library_drag_label_base, nice_tick_interval, set_library_drag,
    };

    fn url_property(url: &str) -> animatix_syntax::ast::Property {
        animatix_syntax::ast::Property {
            name: "url".into(),
            value: animatix_syntax::ast::Expr::Str(url.to_string()),
            value_span: None,
            trailing_comment: None,
        }
    }

    #[test]
    fn library_drag_payload_roundtrips_through_context_data() {
        let ctx = egui::Context::default();
        assert!(library_drag(&ctx).is_none());

        let payload = LibraryDragPayload {
            ty: "Image".into(),
            props: vec![url_property("a.png")],
        };
        set_library_drag(&ctx, payload.clone());
        assert_eq!(library_drag(&ctx), Some(payload));

        clear_library_drag(&ctx);
        assert!(library_drag(&ctx).is_none(), "cleared payload must not linger");
    }

    #[test]
    fn library_drag_id_is_stable() {
        // The sidebar writes and the preview panel reads this id; it must not
        // drift between the two call sites.
        assert_eq!(library_drag_id(), egui::Id::unique("library_drag_payload"));
    }

    #[test]
    fn library_drag_label_base_maps_assets_to_lowercase_stems() {
        let image = LibraryDragPayload {
            ty: "Image".into(),
            props: vec![],
        };
        assert_eq!(library_drag_label_base(&image), "image");
        let svg = LibraryDragPayload {
            ty: "Svg".into(),
            props: vec![],
        };
        assert_eq!(library_drag_label_base(&svg), "svg");
        // Component types keep their declared name.
        let component = LibraryDragPayload {
            ty: "Badge".into(),
            props: vec![],
        };
        assert_eq!(library_drag_label_base(&component), "Badge");
    }

    #[test]
    fn nice_tick_interval_covers_ranges_and_boundaries() {
        let cases = [
            // (visible_range, expected interval at 10 target ticks)
            (100.0, 10.0),
            (50.0, 5.0),
            (0.5, 0.05),
            (0.0, 1.0),
            (-100.0, 10.0),
            (10000.0, 1000.0),
            (0.1, 0.01),
            (1.0, 0.1),
            (10.0, 1.0),
            (1000.0, 100.0),
            // Boundaries around the 1.5 / 3.5 / 7.5 nice-multiple cutoffs.
            (14.9, 1.0),
            (15.1, 2.0),
            (34.9, 2.0),
            (35.1, 5.0),
            (74.9, 5.0),
            (75.1, 10.0),
        ];
        for (range, expected) in cases {
            let interval = nice_tick_interval(range, 10.0);
            assert!(interval > 0.0, "interval must be positive for range={range}");
            assert!(
                (interval - expected).abs() < 0.001,
                "range={range}: expected {expected}, got {interval}"
            );
        }
    }
}
