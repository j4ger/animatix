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
pub(crate) use api::{panel_frame, sidebar_tab_label};

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

pub(super) const RULER_SIZE: f32 = 20.0;
