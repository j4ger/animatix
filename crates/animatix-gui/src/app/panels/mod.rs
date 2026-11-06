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

mod api;
#[cfg(test)]
mod tests;

pub use crate::app::commands::{PropertyEdit, PropertyValue};

pub(super) use api::nice_tick_interval;
pub(crate) use api::{
    clear_library_drag, default_actor_type, library_drag, library_drag_label_base, panel_frame,
    set_library_drag, sidebar_tab_label,
};

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

pub(super) const RULER_SIZE: f32 = 20.0;
