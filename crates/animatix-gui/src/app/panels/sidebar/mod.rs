//! Sidebar panel: file explorer tree and layer tree.
//!
//! Focused context struct borrows only the fields the sidebar needs.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use animatix::timeline::Timeline;
use animatix_syntax::to_source::ToSource;
use egui::Vec2;

use crate::app::FileTreeEntry;
use crate::app::commands::{ActionQueue, Command, ShellAction};
use crate::app::components::context_menu::{MenuEntry, render_menu};
use crate::app::components::row;
use crate::app::design_tokens::typography::TextRole;
use crate::app::panels::{CompactDrawer, SidebarTab};
use eparts::TextField;

/// Id used to persist the explorer filter string in egui's data store.
const EXPLORER_FILTER_ID: &str = "explorer_filter";

/// Id used to persist the layers filter string in egui's data store.
const LAYERS_FILTER_ID: &str = "layers_filter";

/// Shared context for the sidebar panel (tab bar + dispatch only).
///
/// This is a wide struct because it carries everything the sidebar tabs might
/// need. Individual tabs receive focused sub-contexts (e.g. `ExplorerContext`)
/// so their signatures don't depend on the full surface.
pub(crate) struct SidebarContext<'a> {
    pub document: &'a crate::app::document::DocumentSession,
    pub expanded_dirs: &'a mut HashSet<PathBuf>,
    pub file_tree: &'a [FileTreeEntry],
    pub commands: &'a mut ActionQueue,
    pub selected_actors: &'a mut HashSet<String>,
    pub collapsed_actors: &'a mut HashSet<String>,
    pub sidebar_tab: &'a mut SidebarTab,
    /// True when the window is narrow enough to render the compact icon rail
    /// instead of the tab bar + content.
    pub compact: bool,
    /// Compact-mode overlay drawer slot; a rail click opens the sidebar drawer.
    pub compact_drawer: &'a mut Option<CompactDrawer>,
}

// ─── Per-tab focused contexts ─────────────────────────────────────────────

pub(crate) struct ExplorerContext<'a> {
    pub current_file: &'a Path,
    pub expanded_dirs: &'a mut HashSet<PathBuf>,
    pub file_tree: &'a [FileTreeEntry],
    pub commands: &'a mut ActionQueue,
}

pub(crate) struct LayersContext<'a> {
    pub timeline: Option<&'a Timeline>,
    pub selected_actors: &'a mut HashSet<String>,
    pub collapsed_actors: &'a mut HashSet<String>,
}

pub(crate) struct ScenesContext<'a> {
    pub composition: Option<&'a animatix::composition::Composition>,
    pub active_scene: Option<&'a str>,
    pub commands: &'a mut ActionQueue,
}

mod assets;
mod components;
mod explorer;
mod impls;
mod layers;
mod scenes;

use assets::assets_content_ui;
use components::components_content_ui;
use explorer::explorer_content_ui;
pub(crate) use impls::{sidebar_tab_content_ui, sidebar_ui};
use layers::layers_content_ui;
use scenes::scene_picker_ui;

#[cfg(test)]
mod tests;
