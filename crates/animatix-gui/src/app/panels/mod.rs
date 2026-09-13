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
    egui::Id::new("library_drag_payload")
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
    use super::nice_tick_interval;
    use super::{
        LibraryDragPayload, clear_library_drag, library_drag, library_drag_id,
        library_drag_label_base, set_library_drag,
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
        assert_eq!(library_drag_id(), egui::Id::new("library_drag_payload"));
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
    fn nice_tick_interval_normal_range() {
        // visible_range=100.0, target_ticks=10 → raw=10 → magnitude=10 → normalized=1 → nice_mul=1
        // → 10.0
        let interval = nice_tick_interval(100.0, 10.0);
        assert!((interval - 10.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_rounds_to_two() {
        // visible_range=50.0, target_ticks=10 → raw=5 → magnitude=1 → normalized=5 → nice_mul=5 →
        // 5.0
        let interval = nice_tick_interval(50.0, 10.0);
        assert!((interval - 5.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_small_values() {
        // visible_range=0.5, target_ticks=10 → raw=0.05 → magnitude=0.01 → normalized=5 →
        // nice_mul=5 → 0.05
        let interval = nice_tick_interval(0.5, 10.0);
        assert!(interval > 0.0);
        assert!((interval / 0.01 - 5.0).abs() < 0.001 || (interval / 0.05 - 1.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_zero_range() {
        // raw=0.0 → early return 1.0
        assert_eq!(nice_tick_interval(0.0, 10.0), 1.0);
    }

    #[test]
    fn nice_tick_interval_negative_range() {
        // abs(visible_range) used
        assert_eq!(nice_tick_interval(-100.0, 10.0), 10.0);
    }

    #[test]
    fn nice_tick_interval_large_range_gives_round_numbers() {
        // visible_range=10000.0, target_ticks=10 → raw=1000 → magnitude=100 → normalized=10 →
        // nice_mul=10 → 1000.0
        let interval = nice_tick_interval(10000.0, 10.0);
        assert!((interval - 1000.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_always_positive() {
        for &range in &[0.1, 1.0, 10.0, 100.0, 1000.0] {
            let interval = nice_tick_interval(range, 10.0);
            assert!(interval > 0.0, "interval must be positive for range={}", range);
        }
    }

    #[test]
    fn nice_tick_interval_boundary_near_one_point_five() {
        // raw just below 1.5 → nice_mul=1
        let interval = nice_tick_interval(14.9, 10.0);
        assert!((interval - 1.0).abs() < 0.001);

        // raw just above 1.5 → nice_mul=2
        let interval = nice_tick_interval(15.1, 10.0);
        // raw=1.51 → magnitude=1 → normalized=1.51 → nice_mul=2 → 2.0
        assert!((interval - 2.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_boundary_near_three_point_five() {
        // raw just below 3.5 → nice_mul=2
        let interval = nice_tick_interval(34.9, 10.0);
        assert!((interval - 2.0).abs() < 0.001);

        // raw just above 3.5 → nice_mul=5
        let interval = nice_tick_interval(35.1, 10.0);
        assert!((interval - 5.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_boundary_near_seven_point_five() {
        // raw just below 7.5 → nice_mul=5
        let interval = nice_tick_interval(74.9, 10.0);
        assert!((interval - 5.0).abs() < 0.001);

        // raw just above 7.5 → nice_mul=10
        let interval = nice_tick_interval(75.1, 10.0);
        assert!((interval - 10.0).abs() < 0.001);
    }
}
